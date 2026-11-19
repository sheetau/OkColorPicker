use crate::model::{PickerModel, stepped_channel};
use okcolor_core::{Gamut, GamutMapping, GamutProjection, GamutSlice, Oklch};
use zgui::reactive::RenderEffect;
use zgui::{
    geom::{DevicePx, Point, Rect},
    prelude::*,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SurfaceKind {
    Slice,
    Lightness,
    Chroma,
    Hue,
    Alpha,
}

#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
pub struct ColorParams {
    pub channels: [f32; 4],
    pub gamut: f32,
    pub kind: f32,
    pub mapping: f32,
    pub max_chroma: f32,
    pub wide: f32,
    pub show_p3: f32,
    pub show_rec2020: f32,
    pub outline_dark: f32,
    pub checker_offset: [f32; 2],
    pub padding: [f32; 2],
}

static COLOR_SURFACE: ShaderEffect<ColorParams> = shader! {
    name: "okcolor-surface",
    mode: Paint,
    params: ColorParams,
    path: "src/color_surface.wgsl",
};

pub struct ColorSurface {
    shader: ShaderHandle<ColorParams>,
    params: ColorParams,
}

impl ColorSurface {
    pub fn new(params: ColorParams) -> Self {
        Self {
            shader: COLOR_SURFACE.register(),
            params,
        }
    }
}

impl CustomElement for ColorSurface {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: cx
                .known_width
                .unwrap_or(if self.params.kind > 4.5 { 22.0 } else { 290.0 } * cx.scale),
            height: cx
                .known_height
                .unwrap_or(if self.params.kind == 0.0 { 130.0 } else { 22.0 } * cx.scale),
            ..CustomMeasured::default()
        }
    }
    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        self.shader.set_params(self.params);
        let whole = Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), painter.size());
        painter.effect(whole, 0.0, &self.shader);
    }
}

impl SurfaceKind {
    /// The expensive color surface subscribes only to the axes it actually depends on.
    pub fn params(self, model: PickerModel) -> ColorParams {
        let l = if self == Self::Lightness {
            0.0
        } else {
            model.l.get()
        };
        let c = if self == Self::Chroma {
            0.0
        } else {
            model.c.get()
        };
        let h = if self == Self::Hue {
            0.0
        } else {
            model.h.get()
        };
        ColorParams {
            channels: [l as f32, c.min(10000.0) as f32, h as f32, 1.0],
            gamut: match model.gamut.get() {
                Gamut::Srgb => 0.0,
                Gamut::DisplayP3 => 1.0,
                Gamut::Rec2020 => 2.0,
            },
            kind: self as u8 as f32,
            outline_dark: if self == Self::Slice { 0.0 } else { 1.0 },
            mapping: if model.mapping.get() == GamutMapping::Clip {
                1.0
            } else {
                0.0
            },
            max_chroma: model.max_chroma.get() as f32,
            show_p3: if model.show_p3.get() { 1.0 } else { 0.0 },
            show_rec2020: if model.show_rec2020.get() { 1.0 } else { 0.0 },
            wide: if model.display.get().wide_output {
                1.0
            } else {
                0.0
            },
            ..Default::default()
        }
    }

    pub fn pick(self, model: PickerModel, x: f64, y: f64) -> Option<Oklch> {
        let [mut l, mut c, mut h, mut a] = model.current().components();
        if self == Self::Slice {
            return GamutSlice::new(
                h,
                model.max_chroma.get_untracked(),
                model.gamut.get_untracked(),
            )
            .ok()?
            .pick(x, y, a);
        }
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Lightness => l = x,
            Self::Chroma => c = x * model.max_chroma.get_untracked(),
            Self::Hue => h = (x * 360.0).min(360.0_f64.next_down()),
            Self::Alpha => a = x,
            Self::Slice => unreachable!(),
        }
        Oklch::new(l, c, h, a).ok()
    }

    fn snapped(self, model: PickerModel, x: f64, y: f64, width: f64) -> Option<Oklch> {
        if !matches!(self, Self::Lightness | Self::Chroma | Self::Hue) || !(0.0..1.0).contains(&x) {
            return self.pick(model, x, y);
        }
        let at = self.pick(model, x, y)?;
        let mut closest = 4.0 / width;
        let mut result = at;
        for (gamut, visible) in [
            (Gamut::Srgb, true),
            (Gamut::DisplayP3, model.show_p3.get_untracked()),
            (Gamut::Rec2020, model.show_rec2020.get_untracked()),
        ] {
            if !visible {
                continue;
            }
            let inside = at.in_gamut(gamut);
            for direction in [-1.0, 1.0] {
                let mut near = x;
                for pixel in 1..=4 {
                    let mut far = (x + direction * f64::from(pixel) / width).clamp(0.0, 1.0);
                    if self.pick(model, far, y)?.in_gamut(gamut) != inside {
                        for _ in 0..12 {
                            let mid = (near + far) * 0.5;
                            if self.pick(model, mid, y)?.in_gamut(gamut) == inside {
                                near = mid
                            } else {
                                far = mid
                            }
                        }
                        let distance = (near - x).abs();
                        if distance <= closest {
                            closest = distance;
                            result = self.pick(model, if inside { near } else { far }, y)?;
                        }
                        break;
                    }
                    near = far;
                }
            }
        }
        Some(result)
    }

    pub fn marker(self, model: PickerModel) -> [f64; 2] {
        match self {
            Self::Slice => [model.c.get() / model.max_chroma.get(), 1.0 - model.l.get()],
            Self::Lightness => [model.l.get(), 1.0],
            Self::Chroma => [model.c.get() / model.max_chroma.get(), 1.0],
            Self::Hue => [model.h.get() / 360.0, 1.0],
            Self::Alpha => [model.alpha.get(), 1.0],
        }
    }
}

/// A color sample uses the same GPU preview policy as the editing surfaces.
#[component]
pub fn ColorSwatch(model: PickerModel, color: Memo<Oklch>, outline_dark: bool) -> impl IntoView {
    color_sample(model, color, 5.0, outline_dark, || [0.0; 2])
}

#[component]
pub fn ColorMarker(
    model: PickerModel,
    color: Memo<Oklch>,
    kind: SurfaceKind,
    bar: NodeRef,
) -> impl IntoView {
    let size = bar.observe_border_size();
    let scale = use_window().scale();
    color_sample(model, color, 6.0, false, move || {
        let Some(size) = size.get() else {
            return [0.0; 2];
        };
        let marker = kind.marker(model);
        [
            size.width.0 * marker[0].clamp(0.0, 1.0) as f32 - 8.0 * scale.get(),
            size.height.0 - 8.0 * scale.get(),
        ]
    })
}

fn color_sample(
    model: PickerModel,
    color: Memo<Oklch>,
    kind: f32,
    outline_dark: bool,
    checker_offset: impl Fn() -> [f32; 2] + 'static,
) -> impl IntoView {
    let (surface, handle) = zgui::custom::custom(ColorSurface::new(ColorParams {
        kind,
        ..Default::default()
    }));
    let effect = RenderEffect::new(move |_| {
        let [l, c, h, alpha] = color.get().components();
        let params = ColorParams {
            channels: [l as f32, c.min(10000.0) as f32, h as f32, alpha as f32],
            kind,
            outline_dark: if outline_dark { 1.0 } else { 0.0 },
            checker_offset: checker_offset(),
            mapping: if model.mapping.get() == GamutMapping::Clip {
                1.0
            } else {
                0.0
            },
            wide: if model.display.get().wide_output {
                1.0
            } else {
                0.0
            },
            ..Default::default()
        };
        handle.update(|surface| surface.params = params);
        handle.repaint();
    });
    on_cleanup_local(move || drop(effect));
    surface.class("color-surface").into_view()
}

#[component]
pub fn ColorControl(model: PickerModel, kind: SurfaceKind, node: NodeRef) -> impl IntoView {
    let (surface, handle) = zgui::custom::custom(ColorSurface::new(ColorParams::default()));
    let effect = RenderEffect::new(move |_| {
        let params = kind.params(model);
        handle.update(|surface| surface.params = params);
        handle.repaint();
    });
    on_cleanup_local(move || drop(effect));
    let dragging = RwSignal::new(false);
    let projection = RwSignal::new_local(None::<GamutProjection>);
    let scale = use_window().scale();
    let pick = move |x, y| {
        let width = node.window_bounds()?.size.width.0 / scale.get_untracked();
        kind.snapped(model, x, y, f64::from(width))
    };
    let drag = move |x, y| {
        if kind != SurfaceKind::Slice {
            return pick(x, y);
        }
        let bounds = node.window_bounds()?;
        projection.with_untracked(|projection| {
            projection.as_ref()?.project(
                x,
                y,
                model.alpha.get_untracked(),
                f64::from(bounds.size.width.0 / bounds.size.height.0),
            )
        })
    };
    let coordinate =
        move |position: zgui::geom::Point<zgui::geom::CssPx, zgui::geom::Css>,
              bounds: Option<zgui::geom::Rect<DevicePx, zgui::geom::Device>>| {
            let bounds = bounds?;
            if bounds.size.width.0 <= 0.0 || bounds.size.height.0 <= 0.0 {
                return None;
            }
            let scale = scale.get_untracked();
            if kind != SurfaceKind::Slice {
                let window = node.window_root()?.window_bounds()?;
                if !window.contains(Point::new(
                    DevicePx(position.x.0 * scale),
                    DevicePx(position.y.0 * scale),
                )) {
                    return None;
                }
            }
            Some((
                f64::from((position.x.0 * scale - bounds.origin.x.0) / bounds.size.width.0),
                f64::from((position.y.0 * scale - bounds.origin.y.0) / bounds.size.height.0),
            ))
        };
    let down = move |cx: &mut EventCx<'_, events::PointerDown>| {
        if !cx.primary || cx.button != Some(PointerButton::Primary) {
            return;
        }
        if let Some((x, y)) = coordinate(cx.position, node.window_bounds())
            && let Some(color) = pick(x, y)
            && model.begin()
        {
            if kind == SurfaceKind::Slice {
                projection.set(Some(
                    GamutSlice::new(
                        model.h.get_untracked(),
                        model.max_chroma.get_untracked(),
                        model.gamut.get_untracked(),
                    )
                    .expect("validated slice")
                    .projection(),
                ));
            }
            dragging.set(true);
            cx.capture_pointer();
            model.preview(color);
        }
    };
    let moved = move |cx: &mut EventCx<'_, events::PointerMove>| {
        if cx.primary
            && dragging.get_untracked()
            && let Some((x, y)) = coordinate(cx.position, node.window_bounds())
            && let Some(color) = drag(x, y)
        {
            model.preview(color);
        }
    };
    let up = move |cx: &mut EventCx<'_, events::PointerUp>| {
        if cx.primary && cx.button == Some(PointerButton::Primary) && dragging.get_untracked() {
            if let Some((x, y)) = coordinate(cx.position, node.window_bounds())
                && let Some(color) = drag(x, y)
            {
                model.preview(color);
            }
            dragging.set(false);
            cx.release_pointer();
            model.commit();
        }
    };
    let keys = move |cx: &mut EventCx<'_, events::KeyDown>| {
        if cx.key == Key::Named(NamedKey::Escape) && dragging.get_untracked() {
            dragging.set(false);
            cx.release_pointer();
            model.cancel();
            cx.prevent_default();
            return;
        }
        let direction = match cx.key {
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => -1.0,
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => 1.0,
            _ => return,
        };
        let index = match kind {
            SurfaceKind::Lightness => 0,
            SurfaceKind::Chroma => 1,
            SurfaceKind::Hue => 2,
            SurfaceKind::Alpha => 3,
            SurfaceKind::Slice => {
                if matches!(cx.key, Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown)) {
                    0
                } else {
                    1
                }
            }
        };
        let value = stepped_channel(
            index,
            model.current().components()[index],
            direction,
            cx.modifiers,
        );
        if kind == SurfaceKind::Slice {
            let mut channels = model.current().components();
            channels[index] = value;
            if let Ok(color) = Oklch::new(channels[0], channels[1], channels[2], channels[3])
                && color.in_gamut(model.gamut.get_untracked())
            {
                model.select(color);
            }
        } else {
            model.set_channel(index, value);
        }
        cx.prevent_default();
        cx.stop_propagation();
    };
    let extent = node.observe_border_size();
    let label = match kind {
        SurfaceKind::Slice => "Lightness and chroma",
        SurfaceKind::Lightness => "Lightness",
        SurfaceKind::Chroma => "Chroma",
        SurfaceKind::Hue => "Hue",
        SurfaceKind::Alpha => "Alpha",
    };
    view! {
        box(class="color-control",
            node_ref=node,
            class:slice=kind==SurfaceKind::Slice,
            tabindex=Focus::Sequential,
            a11y:label=label,
            on:pointer_down=down,
            on:pointer_move=moved,
            on:pointer_up=up,
            on:key_down=keys,
            on:pointer_cancel=move |_| { if dragging.get_untracked() {dragging.set(false);model.cancel();} }) {
            {surface.class("color-surface").into_view()}
            if move ||kind==SurfaceKind::Slice {
                text(class="axis-l",style:top=move ||{
                    let height = extent.get().map_or(152.0,|s|s.height.0/scale.get());
                    Some(format!("{}px",((1.0-model.l.get())*f64::from(height)-5.0).clamp(0.0,f64::from((height-10.0).max(0.0)))))
                }) {"L"}
            } else {
                box(class="color-marker",style:left=move ||Some(format!("{}%",kind.marker(model)[0].clamp(0.0,1.0)*100.0))) {
                    ColorMarker(model=model,kind=kind,bar=node,color=Memo::new(move |_| {
                        let [l,c,h,a]=model.color().components();
                        Oklch::new(l,c,h,if kind==SurfaceKind::Alpha {a} else {1.0}).expect("valid marker color")
                    }))
                }
            }
        }
    }
}
