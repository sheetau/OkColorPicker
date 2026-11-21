import { useEffect, useRef } from "react";
import { boundary, clamp, clipped, hex, hueDirection } from "./picker-color.mjs";

const symbols = ".:-=+*#%@";
const levels = (symbols.length - 1) * 4 + 1;
const inkRows = 8;
const stepTime = 1 / 60;
const followPerStep = 1 - Math.exp(-stepTime / 0.085);

// Each row adds a little more of the picker's chroma at the same OKLab L.
// Intermediate columns soften the handoff between two adjacent characters.
function makeAtlas(tile, color) {
  const sheet = document.createElement("canvas");
  sheet.width = levels * tile;
  sheet.height = inkRows * tile;
  const brush = sheet.getContext("2d");
  brush.font = `500 ${tile * 0.82}px ui-monospace, Consolas, monospace`;
  brush.textAlign = "center";
  brush.textBaseline = "middle";
  brush.globalCompositeOperation = "lighter";
  const hue = hueDirection(color.h);
  const chroma = Math.min(color.c, 0.19, boundary(0.75, hue));
  for (let row = 0; row < inkRows; row++) {
    brush.fillStyle = hex(clipped(0.75, chroma * row / (inkRows - 1), hue));
    for (let col = 0; col < levels; col++) {
      const index = Math.floor(col / 4), fraction = (col % 4) / 4;
      const x = (col + 0.5) * tile, y = (row + 0.5) * tile;
      brush.globalAlpha = 1 - fraction;
      brush.fillText(symbols[index], x, y);
      if (fraction) {
        brush.globalAlpha = fraction;
        brush.fillText(symbols[index + 1], x, y);
      }
    }
  }
  return sheet;
}

// Smoothly varying water depth bends the swell and refracts a passing wake.
const depths = new Float32Array(64 * 64);
let seed = 7243;
for (let i = 0; i < depths.length; i++) {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  depths[i] = seed / 0x80000000 - 1;
}
function depthAt(x, y) {
  const col = Math.floor(x), row = Math.floor(y);
  const fx = x - col, fy = y - row;
  const u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy);
  const left = col & 63, right = (col + 1) & 63;
  const top = (row & 63) * 64, bottom = ((row + 1) & 63) * 64;
  const a = depths[top + left] * (1 - u) + depths[top + right] * u;
  const b = depths[bottom + left] * (1 - u) + depths[bottom + right] * u;
  return a * (1 - v) + b * v;
}

function swell(x, y, t) {
  const along = x * 0.82 + y * 0.57, across = y * 0.82 - x * 0.57;
  const bend = depthAt(along / 370 - t * 0.038, across / 210 + t * 0.043);
  const drift = depthAt(7 + across / 320 - t * 0.052, along / 280 + bend * 0.7);
  const phase = along * 0.017 + 2.4 * bend + 0.65 * drift - t * 0.56;
  const crossPhase = 0.010 * along - 0.013 * across + drift * 1.8 - t * 0.44;
  return (25 + 9 * drift) * (Math.sin(phase) + 0.15 * Math.sin(2 * phase - 0.6))
    + (11 - 5 * bend) * Math.sin(crossPhase)
    + 4.5 * Math.sin(phase * 1.72 + crossPhase * 0.43 - t * 0.18);
}

function makeSurface(width, height, pitch) {
  // One extra sample on each edge supplies the normals of the visible cells.
  const cols = Math.ceil(width / pitch) + 2, rows = Math.ceil(height / pitch) + 2;
  const count = cols * rows;
  const water = new Float32Array(count), velocity = new Float32Array(count);
  const surface = new Float32Array(count);
  const travel = new Float32Array(count), damping = new Float32Array(count);
  const originX = (width - (cols - 1) * pitch) / 2;
  const originY = (height - (rows - 1) * pitch) / 2;
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) {
      const i = row * cols + col;
      const depth = depthAt((originX + col * pitch) / 185, (originY + row * pitch) / 210);
      travel[i] = ((244 + 58 * depth) * stepTime / pitch) ** 2;
      const edge = Math.min(col, row, cols - 1 - col, rows - 1 - row);
      damping[i] = 0.976 - 0.13 * Math.max(0, 1 - edge / 5) ** 2;
    }
  }

  function press(point, vx, vy, tap = false) {
    const speed = Math.hypot(vx, vy);
    if (!tap && speed < 6) return;
    const force = speed / (420 + speed);
    const across = Math.max(23 + 8 * force, 1.5 * pitch);
    const along = across + 32 * force;
    const nx = speed ? vx / speed : 1, ny = speed ? vy / speed : 0;
    const pressure = tap ? 132 : 54 * force ** 1.5;
    const reach = 3.5 * along;
    const left = Math.max(1, Math.floor((point.x - reach - originX) / pitch));
    const right = Math.min(cols - 2, Math.ceil((point.x + reach - originX) / pitch));
    const top = Math.max(1, Math.floor((point.y - reach - originY) / pitch));
    const bottom = Math.min(rows - 2, Math.ceil((point.y + reach - originY) / pitch));
    for (let row = top; row <= bottom; row++) {
      for (let col = left; col <= right; col++) {
        const x = originX + col * pitch - point.x, y = originY + row * pitch - point.y;
        const u = (x * nx + y * ny) / along, v = (y * nx - x * ny) / across;
        // The Laplacian of a moving pressure footprint depresses the middle
        // and raises its shoulders. Both propagate in the same height field.
        const curvature = (u * u - 1) / (along * along) + (v * v - 1) / (across * across);
        const i = row * cols + col;
        velocity[i] += travel[i] * pitch * pitch * pressure * Math.exp(-0.5 * (u * u + v * v)) * curvature;
      }
    }
  }

  function advance() {
    // Integrate velocity from the previous height field before moving any cell.
    for (let row = 1; row < rows - 1; row++) {
      for (let i = row * cols + 1; i < (row + 1) * cols - 1; i++) {
        const sides = water[i - 1] + water[i + 1] + water[i - cols] + water[i + cols];
        const corners = water[i - cols - 1] + water[i - cols + 1] + water[i + cols - 1] + water[i + cols + 1];
        const curvature = (4 * sides + corners - 20 * water[i]) / 6;
        velocity[i] = damping[i] * velocity[i] + travel[i] * curvature;
      }
    }
    for (let i = 0; i < count; i++) water[i] += velocity[i];
  }

  function paint(ctx, atlas, tile, time) {
    for (let row = 0; row < rows; row++) {
      const y = originY + row * pitch;
      for (let col = 0; col < cols; col++) {
        const i = row * cols + col;
        // Refract the swell on each passing wave front. Its two slopes bend
        // the crest in opposite directions as they travel through a cell.
        const interior = col > 0 && col < cols - 1 && row > 0 && row < rows - 1;
        const gx = interior ? (water[i + 1] - water[i - 1]) / (2 * pitch) : 0;
        const gy = interior ? (water[i + cols] - water[i - cols]) / (2 * pitch) : 0;
        const bend = 52 / Math.sqrt(1 + 2.5 * (gx * gx + gy * gy));
        surface[i] = swell(originX + col * pitch - bend * gx, y - bend * gy, time) + water[i];
      }
    }

    for (let row = 1; row < rows - 1; row++) {
      for (let col = 1; col < cols - 1; col++) {
        const i = row * cols + col;
        const slopeX = (surface[i + 1] - surface[i - 1]) / (2 * pitch);
        const slopeY = (surface[i + cols] - surface[i - cols]) / (2 * pitch);
        const normalLength = Math.sqrt(1 + slopeX * slopeX + slopeY * slopeY);
        const facing = 0.82 * slopeX + 0.57 * slopeY;
        const diffuse = Math.max(0, (0.50 + 0.866 * facing) / normalLength);
        const reflection = Math.max(0, (0.866 + 0.50 * facing) / normalLength);
        const shoulder = clamp(0.5 + surface[i] / 85);
        const glint = reflection ** 44;
        const level = clamp(0.035 + 0.15 * shoulder + 0.34 * diffuse ** 1.6 + 0.49 * glint);
        const shine = Math.round(level * (levels - 1));
        const lift = pitch * 0.44 * Math.tanh(surface[i] / 42);
        const shift = pitch * 0.22 * slopeX / normalLength;
        const wakeSlope = (Math.abs(water[i + 1] - water[i - 1]) + Math.abs(water[i + cols] - water[i - cols])) / pitch;
        const wake = clamp(wakeSlope * 1.8 + Math.abs(water[i]) * 0.055 + Math.abs(velocity[i]) * 0.10);
        const ink = Math.round(wake * (inkRows - 1));
        const ambient = 0.075 + 0.18 * level;
        ctx.globalAlpha = ambient * (0.84 + 0.16 * Math.min(1, wake * (inkRows - 1))) + 0.10 * wake;
        ctx.drawImage(atlas, shine * tile, ink * tile, tile, tile,
          originX + (col - 0.5) * pitch - shift, originY + (row - 0.5) * pitch - lift, pitch, pitch);
      }
    }
  }

  function settle() {
    water.fill(0);
    velocity.fill(0);
  }
  return { press, advance, paint, settle };
}

export default function SymbolWaves({ colorRef, heroRef }) {
  const canvasRef = useRef(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    let width, height, tile, field, atlas, lastColor, visible = true;
    let frame = 0, lastDraw = 0, time = 0, accumulator = 0, wakeRemaining = 0;
    let pointer = null, tip = null, tap = null;

    function draw() {
      if (lastColor !== colorRef.current) {
        lastColor = colorRef.current;
        atlas = makeAtlas(tile, lastColor);
      }
      ctx.clearRect(0, 0, width, height);
      field.paint(ctx, atlas, tile, time);
    }
    function resize() {
      const bottom = heroRef.current.getBoundingClientRect().bottom - canvas.getBoundingClientRect().top;
      canvas.style.setProperty("--hero-bottom", `${bottom}px`);
      const scale = Math.min(window.devicePixelRatio || 1, 2);
      if (width === canvas.clientWidth && height === canvas.clientHeight && canvas.width === Math.round(width * scale)) return;
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.round(width * scale);
      canvas.height = Math.round(height * scale);
      ctx.setTransform(scale, 0, 0, scale, 0, 0);
      const pitch = Math.max(14, Math.ceil(Math.sqrt(width * height / 9800)));
      tile = Math.ceil(pitch * scale);
      field = makeSurface(width, height, pitch);
      lastColor = null;
      draw();
    }
    function tick(now) {
      const delta = (now - lastDraw) / 1000;
      if (delta >= 1 / 30 - 0.001) {
        lastDraw = now;
        const elapsed = Math.min(0.08, delta);
        time += elapsed;
        if (wakeRemaining > 0) {
          accumulator += elapsed;
          while (accumulator >= stepTime) {
            if (tap) {
              field.press(tap, 0, 0, true);
              tap = null;
            }
            if (pointer && tip) {
              // Ease the load along the stroke. It sheds its wake as it slows.
              const dx = (pointer.x - tip.x) * followPerStep, dy = (pointer.y - tip.y) * followPerStep;
              tip.x += dx;
              tip.y += dy;
              field.press(tip, dx / stepTime, dy / stepTime);
            }
            field.advance();
            accumulator -= stepTime;
          }
          wakeRemaining -= elapsed;
          if (wakeRemaining <= 0) field.settle();
        }
        draw();
      }
      frame = requestAnimationFrame(tick);
    }
    function forgetPointer() {
      pointer = tip = null;
    }
    function refresh() {
      cancelAnimationFrame(frame);
      forgetPointer();
      tap = null;
      if (wakeRemaining > 0) field.settle();
      accumulator = wakeRemaining = 0;
      lastDraw = performance.now();
      if (!visible || document.hidden) return;
      if (reduced.matches) draw();
      else frame = requestAnimationFrame(tick);
    }
    function input(event) {
      if (!event.isPrimary || !visible || reduced.matches || document.hidden) return;
      const rect = canvas.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      if (x < 0 || x >= width || y < 0 || y >= height) {
        forgetPointer();
        return;
      }
      if (event.type === "pointerdown") forgetPointer();
      pointer = { x, y };
      tip ??= { x, y };
      if (event.type === "pointerdown") tap = pointer;
      wakeRemaining = 7;
    }
    function leave(event) {
      if (!event.relatedTarget) forgetPointer();
    }
    resize();
    refresh();
    const observer = new ResizeObserver(resize);
    observer.observe(heroRef.current);
    observer.observe(canvas.parentElement);
    const visibility = new IntersectionObserver(([entry]) => {
      if (visible !== entry.isIntersecting) {
        visible = entry.isIntersecting;
        refresh();
      }
    });
    visibility.observe(canvas);
    window.addEventListener("pointermove", input, { passive: true });
    window.addEventListener("pointerdown", input, { passive: true });
    window.addEventListener("pointerout", leave);
    window.addEventListener("pointercancel", forgetPointer);
    window.addEventListener("blur", forgetPointer);
    window.addEventListener("scroll", forgetPointer, { passive: true });
    document.addEventListener("visibilitychange", refresh);
    reduced.addEventListener("change", refresh);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      visibility.disconnect();
      window.removeEventListener("pointermove", input);
      window.removeEventListener("pointerdown", input);
      window.removeEventListener("pointerout", leave);
      window.removeEventListener("pointercancel", forgetPointer);
      window.removeEventListener("blur", forgetPointer);
      window.removeEventListener("scroll", forgetPointer);
      document.removeEventListener("visibilitychange", refresh);
      reduced.removeEventListener("change", refresh);
    };
  }, [colorRef, heroRef]);

  return <canvas className="symbol-waves" ref={canvasRef} aria-hidden="true" />;
}
