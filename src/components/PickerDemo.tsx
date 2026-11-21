import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { basePath } from '../site/config.mjs';
import { MAX_CHROMA, clamp, clipped, decimal, fallbackDistance, hex, hueDirection, initialColor, inGamut, makeContour, preview, project } from './picker-color.mjs';
import { drawHue, drawPlane, plotHeight, plotWidth } from './picker-surfaces.mjs';
import styles from './picker-demo.module.css';

type Color = typeof initialColor;
type Control = 'plane' | 'hue';
type PointEvent = PointerEvent<SVGRectElement>;
const icons = basePath + '/picker/';

function Icon({ name, x, y, size, ink = 'muted' }: {
  name: string; x: number; y: number; size: number; ink?: 'muted' | 'original';
}) {
  return <image aria-hidden="true" href={icons + name + '.svg'} x={x} y={y} width={size} height={size} className={styles[ink]} />;
}

export default function PickerDemo({ lang, onColorChange }: { lang: 'ja' | 'en'; onColorChange: (color: Color) => void }) {
  const [color, setColor] = useState<Color>(initialColor);
  const planeCanvas = useRef<HTMLCanvasElement>(null);
  const hueCanvas = useRef<HTMLCanvasElement>(null);
  const gesture = useRef<{ start: Color; pointer: number } | null>(null);
  const guide = useId();
  const { l, c, h } = color;
  const direction = useMemo(() => hueDirection(h), [h]);
  const contours = useMemo(() => [0, 1, 2].map(gamut => makeContour(direction, gamut, plotHeight * 2)), [direction]);
  const projected = preview(l, c, direction);
  const display = hex(projected);
  const clipRgb = clipped(l, c, direction);
  const gamutIndex = [0, 1, 2].find(g => inGamut(l, c, direction, g)) ?? 3;
  const gamutLabel = ['sRGB', 'P3', 'Rec.2020', 'Out of gamut'][gamutIndex];
  const secondFallback = gamutIndex !== 0 && fallbackDistance(projected, clipRgb) >= 5;
  const channels = [decimal(l), decimal(c), decimal(h), '100%'];
  const code = 'oklch(' + channels.slice(0, 3).join(' ') + ')';
  const markerX = clamp(c / MAX_CHROMA) * plotWidth;
  const markerY = (1 - l) * plotHeight;
  const ja = lang === 'ja';
  const initialPlane = h === initialColor.h;
  const initialHue = l === initialColor.l && c === initialColor.c;

  useEffect(() => { onColorChange(color); }, [color, onColorChange]);
  useEffect(() => {
    if (planeCanvas.current) drawPlane(planeCanvas.current, direction, contours);
  }, [contours, direction]);
  useEffect(() => {
    if (hueCanvas.current) drawHue(hueCanvas.current, l, c);
  }, [l, c]);

  function point(event: PointEvent, control: Control, dragging: boolean) {
    const box = event.currentTarget.getBoundingClientRect();
    const x = (event.clientX - box.left) / box.width;
    if (control === 'hue') return { ...color, h: 360 * clamp(x) };
    const y = (event.clientY - box.top) / box.height;
    const lightness = 1 - y, chroma = x * MAX_CHROMA;
    if (x >= 0 && x <= 1 && y >= 0 && y <= 1 && inGamut(lightness, chroma, direction, 2))
      return { l: lightness, c: chroma, h };
    if (dragging) return { ...project(x, y, contours[2], box.width / box.height, direction), h };
    return null;
  }
  function cancel() {
    if (gesture.current) setColor(gesture.current.start);
    gesture.current = null;
  }
  function pointerHandlers(control: Control) {
    return {
      onPointerDown(event: PointEvent) {
        if (event.button !== 0 || !event.isPrimary || gesture.current) return;
        const next = point(event, control, false);
        if (!next) return;
        event.currentTarget.focus({ preventScroll: true });
        event.currentTarget.setPointerCapture(event.pointerId);
        gesture.current = { pointer: event.pointerId, start: color };
        setColor(next);
      },
      onPointerMove(event: PointEvent) {
        if (gesture.current?.pointer !== event.pointerId) return;
        const next = point(event, control, true);
        if (next) setColor(next);
      },
      onPointerUp(event: PointEvent) {
        if (gesture.current?.pointer !== event.pointerId) return;
        const next = point(event, control, true);
        if (next) setColor(next);
        gesture.current = null;
        event.currentTarget.releasePointerCapture(event.pointerId);
      },
      onLostPointerCapture: cancel,
      onPointerCancel: cancel,
      onKeyDown(event: KeyboardEvent<SVGRectElement>) {
        if (event.key === 'Escape' && gesture.current) { cancel(); return; }
        const vertical = event.key === 'ArrowUp' || event.key === 'ArrowDown';
        if (!vertical && event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
        event.preventDefault();
        const sign = event.key === 'ArrowUp' || event.key === 'ArrowRight' ? 1 : -1;
        const step = sign * (event.shiftKey ? 10 : 1) * (event.ctrlKey ? 0.1 : 1);
        let next: Color;
        if (control === 'hue') next = { ...color, h: clamp(h + step, 0, 360) };
        else {
          next = vertical ? { ...color, l: clamp(l + step * 0.01) } : { ...color, c: clamp(c + step * 0.001, 0, MAX_CHROMA) };
          if (!inGamut(next.l, next.c, direction, 2)) return;
        }
        setColor(next);
      },
    };
  }

  return (
    <svg viewBox="12 81 308 430" className={styles.picker} role="group" aria-label={ja ? 'OkColorPicker ピッカーデモ' : 'OkColorPicker demo'}>
      <desc id={guide}>{ja ? '明度と彩度は色域内をドラッグするか、上下と左右の矢印キーで調整します。色相は下のスライダーで調整します。' : 'Drag inside the gamut, or use the up and down arrows for lightness and left and right for chroma. Use the slider below to adjust hue.'}</desc>

      <g transform="translate(21 84)">
        {initialPlane ? <image href={icons + 'initial-plane.png'} width={plotWidth} height={plotHeight} /> :
          <foreignObject width={plotWidth} height={plotHeight}><canvas ref={planeCanvas} className={styles.surface} width={plotWidth * 2} height={plotHeight * 2} /></foreignObject>}
        <g className={styles.axes}>
          <path d={'M' + clamp(markerX, 0.5, plotWidth - 0.5) + ' 0v' + plotHeight} />
          <path d={'M0 ' + clamp(markerY, 0.5, plotHeight - 0.5) + 'h' + plotWidth} />
        </g>
        <g className={styles.caption}>
          <text x={markerX > plotWidth - 14 ? 2 : plotWidth - 2} y={clamp(markerY + 3, 8, plotHeight - 2)} textAnchor={markerX > plotWidth - 14 ? 'start' : 'end'}>L</text>
          <text x={clamp(markerX, 3, plotWidth - 3)} y="194" textAnchor="middle">C</text>
          <text x={markerX < 65 ? plotWidth : 0} y="194" textAnchor={markerX < 65 ? 'end' : 'start'}>{gamutLabel}</text>
        </g>
        <rect className={styles.planeControl} width={plotWidth} height={plotHeight} tabIndex={0} role="group"
          aria-describedby={guide} aria-label={(ja ? '明度と彩度' : 'Lightness and chroma') + ', L ' + channels[0] + ', C ' + channels[1]}
          {...pointerHandlers('plane')} />
      </g>
      <g transform="translate(21 290)">
        {initialHue ? <image href={icons + 'initial-hue.png'} width={plotWidth} height="20" preserveAspectRatio="none" /> :
          <foreignObject width={plotWidth} height="20"><canvas className={styles.surface} width={plotWidth * 2} height="20" ref={hueCanvas} /></foreignObject>}
        <path d="M-7.5 19.5 0 12l7.5 7.5z" transform={'translate(' + h / 360 * plotWidth + ' 0)'} fill={display} stroke="white" />
        <rect className={styles.hueControl} y="-3" width={plotWidth} height="26" tabIndex={0} role="slider"
          aria-label={ja ? '色相' : 'Hue'} aria-valuemin={0} aria-valuemax={360} aria-valuenow={Number(channels[2])}
          aria-valuetext={channels[2] + '°'} {...pointerHandlers('hue')} />
      </g>

      <rect x="45.5" y="354.5" width="39" height="39" fill="#000" stroke="#282828" />
      {gamutIndex === 0 ? <rect x="21" y="330" width="40" height="40" fill={display} /> : <g>
        <Icon name="unavailable" x={21} y={330} size={40} ink="original" />
        <Icon name="monitor" x={95} y={secondFallback ? 329 : 340} size={20} />
        <rect x="120" y={secondFallback ? 329 : 340} width="20" height="20" fill={display} />
        {secondFallback && <><Icon name="cube" x={95} y={351} size={20} /><rect x="120" y="351" width="20" height="20" fill={hex(clipRgb)} /></>}
      </g>}
      <Icon name="swap" x={69} y={330} size={16} />
      <Icon name="eyedropper" x={21} y={378} size={16} />
      <rect x="21" y="413" width="20" height="20" fill="#fff" />
      <Icon name="transparent" x={21} y={413} size={20} ink="original" />
      <rect x="43.5" y="413.5" width="19" height="19" fill="#000" stroke="#282828" />
      <rect x="65" y="413" width="20" height="20" fill="white" />
      <g className={styles.mono}>
        {channels.map((value, i) => <g key={i}>
          <text className={styles.label} x="235" y={345 + i * 27} textAnchor="end">{['L', 'C', 'H', 'A'][i]}</text>
          <text x="305" y={345 + i * 27} textAnchor="end">{value}</text>
        </g>)}
        <text className={styles.label} x="21" y="469">O</text><text x="39" y="469">{code}</text>
        <text className={styles.label} x="21" y="498">R</text><text x="39" y="498">{display}</text>
      </g>
      <Icon name="copy" x={287} y={454} size={24} />
      <Icon name="chevron" x={266} y={486} size={16} /><Icon name="copy" x={287} y={483} size={24} />
    </svg>
  );
}
