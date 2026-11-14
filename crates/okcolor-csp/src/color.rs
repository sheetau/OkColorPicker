//! Color payloads use unsigned 32-bit normalized channels, not display integers.
use crate::{
    Error,
    frame::{Frame, Kind},
};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorState {
    pub rgb: [f64; 3],
    pub slot: u8,
    pub transparent: bool,
    pub can_set: bool,
    pub manipulating: bool,
}

/// Partial protocol updates are merged only with observations from this socket.
/// Unknown channels never acquire an implicit zero. Invalid updates are atomic.
#[derive(Clone, Default)]
pub struct ColorCache {
    hsv: [[Option<u32>; 3]; 2],
    rgb: [Option<u32>; 3],
    slot: Option<u8>,
    hsv_selected: Option<bool>,
    transparent: Option<bool>,
    can_set: Option<bool>,
    manipulating: Option<bool>,
}
impl ColorCache {
    pub fn apply(&mut self, frame: &Frame) -> Result<Option<ColorState>, Error> {
        if !matches!(frame.kind, Kind::Command | Kind::Success)
            || !matches!(
                frame.command.as_str(),
                "SyncColorCircleUIState" | "SyncSubViewUIState"
            )
        {
            return Ok(None);
        }
        let Some(detail) = frame.detail.as_ref() else {
            return Ok(None);
        };
        if !frame.data.is_empty() {
            return Err(Error::InvalidColorState);
        }
        let object = detail.as_object().ok_or(Error::InvalidColorState)?;
        if object.contains_key("ServerSelectedTabKind") {
            return Ok(None);
        }
        let mut next = self.clone();
        if let Some(value) = object.get("CurrentColorIndex") {
            let slot = value
                .as_u64()
                .filter(|v| *v <= 1)
                .ok_or(Error::InvalidColorState)? as u8;
            if next.slot != Some(slot) {
                next.rgb = [None; 3];
            }
            next.slot = Some(slot);
        }
        update_bool(object, "IsCurrentColorTransparent", &mut next.transparent)?;
        update_bool(object, "CanExecuteSetterCommand", &mut next.can_set)?;
        update_bool(object, "IsManipulating", &mut next.manipulating)?;
        if frame.command == "SyncColorCircleUIState" {
            if let Some(value) = object.get("ColorSelectionModel") {
                let selected = Some(match value.as_str() {
                    Some("HSV") => true,
                    Some("HLS" | "Unknown") => false,
                    _ => return Err(Error::InvalidColorState),
                });
                if next.hsv_selected != selected {
                    next.hsv = [[None; 3]; 2];
                    next.rgb = [None; 3];
                }
                next.hsv_selected = selected;
            }
            for (slot, prefix) in ["HSVColorMain", "HSVColorSub"].iter().enumerate() {
                for (channel, suffix) in ["H", "S", "V"].iter().enumerate() {
                    update_channel(
                        object,
                        &format!("{prefix}{suffix}"),
                        &mut next.hsv[slot][channel],
                    )?;
                }
            }
        } else {
            for (channel, name) in ["CurrentColorRed", "CurrentColorGreen", "CurrentColorBlue"]
                .iter()
                .enumerate()
            {
                update_channel(object, name, &mut next.rgb[channel])?;
            }
        }
        let color = next.current(frame.command == "SyncSubViewUIState");
        *self = next;
        Ok(color)
    }
    fn current(&self, from_rgb: bool) -> Option<ColorState> {
        let slot = self.slot?;
        let rgb = if from_rgb {
            complete(self.rgb)?.map(unit)
        } else if self.hsv_selected? {
            hsv_to_rgb(complete(self.hsv[usize::from(slot)])?.map(unit))
        } else {
            return None;
        };
        Some(ColorState {
            rgb,
            slot,
            transparent: self.transparent?,
            can_set: self.can_set?,
            manipulating: self.manipulating?,
        })
    }
    pub(crate) fn rgb_state(&self) -> Option<ColorState> {
        self.current(true)
    }
}
fn complete(channels: [Option<u32>; 3]) -> Option<[u32; 3]> {
    Some([channels[0]?, channels[1]?, channels[2]?])
}
fn update_bool(
    object: &Map<String, Value>,
    name: &str,
    target: &mut Option<bool>,
) -> Result<(), Error> {
    if let Some(value) = object.get(name) {
        *target = Some(value.as_bool().ok_or(Error::InvalidColorState)?);
    }
    Ok(())
}
fn update_channel(
    object: &Map<String, Value>,
    name: &str,
    target: &mut Option<u32>,
) -> Result<(), Error> {
    if let Some(value) = object.get(name) {
        *target = Some(
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(Error::InvalidColorState)?,
        );
    }
    Ok(())
}
fn unit(value: u32) -> f64 {
    f64::from(value) / f64::from(u32::MAX)
}

fn hsv_to_rgb([h, s, v]: [f64; 3]) -> [f64; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let components = match h as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    components.map(|channel| channel + v - c)
}

/// Bounded RGB, explicit foreground/background slot and CSP's binary selector.
/// Alpha and brush opacity deliberately have no representation here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorCommand {
    pub(crate) hsv: [u32; 3],
    pub(crate) slot: u8,
    pub(crate) transparent: bool,
}
impl ColorCommand {
    pub fn new(rgb: [f64; 3], slot: u8, transparent: bool) -> Result<Self, Error> {
        if slot > 1
            || rgb
                .iter()
                .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            return Err(Error::InvalidColorState);
        }
        let [r, g, b] = rgb;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        let s = if max == 0.0 { 0.0 } else { delta / max };
        Ok(Self {
            hsv: [h, s, max].map(|c| (c * f64::from(u32::MAX)).round() as u32),
            slot,
            transparent,
        })
    }
    pub fn rgb(self) -> [f64; 3] {
        hsv_to_rgb(self.hsv.map(unit))
    }
    pub fn slot(self) -> u8 {
        self.slot
    }
    pub fn transparent(self) -> bool {
        self.transparent
    }
}
