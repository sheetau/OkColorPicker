// D65 matrices and local-MINDE preview policy from okcolor-core.
export const MAX_CHROMA = 0.47;
export const initialColor = { l: 0.741341, c: 0.126212, h: 190.854638 }; // sRGB #0ec4be
export const clamp = (value, min = 0, max = 1) => Math.min(max, Math.max(min, value));
export const decimal = (value) => String(Number(value.toFixed(4)));

const lmsToXyz = [
  [1.2268798758, -0.5578149945, 0.2813910457],
  [-0.0405757452, 1.1122868033, -0.0717110581],
  [-0.0763729367, -0.4214933324, 1.5869240198],
];
const fromXyz = [
  [[3.2409699419, -1.5373831776, -0.4986107603], [-0.9692436363, 1.8759675015, 0.0415550574], [0.0556300797, -0.2039769589, 1.0569715142]],
  [[2.4934969119, -0.9313836179, -0.4027107845], [-0.8294889696, 1.7626640603, 0.0236246858], [0.0358458302, -0.0761723893, 0.9568845240]],
  [[1.7166511880, -0.3556707838, -0.2533662814], [-0.6666843518, 1.6164812366, 0.0157685458], [0.0176398574, -0.0427706133, 0.9421031212]],
];
const lmsToRgb = fromXyz.map(matrix => matrix.map(row =>
  row.map((_, i) => row.reduce((sum, value, j) => sum + value * lmsToXyz[j][i], 0))));
const multiply = (matrix, x, y, z) => matrix.map(([a, b, c]) => a * x + b * y + c * z);
export const hueDirection = h => [Math.cos(h * Math.PI / 180), Math.sin(h * Math.PI / 180)];

function linearRgb(l, c, direction, gamut = 0) {
  const a = c * direction[0], b = c * direction[1];
  return multiply(lmsToRgb[gamut],
    (l + 0.3963377774 * a + 0.2158037573 * b) ** 3,
    (l - 0.1055613458 * a - 0.0638541728 * b) ** 3,
    (l - 0.0894841775 * a - 1.2914855480 * b) ** 3);
}

const inside = rgb => rgb.every(v => v >= -1e-7 && v <= 1.0000001);
export const inGamut = (l, c, direction, gamut) => inside(linearRgb(l, c, direction, gamut));
export function boundary(l, direction, gamut) {
  if (l <= 0 || l >= 1) return 0;
  let low = 0, high = MAX_CHROMA;
  for (let i = 0; i < 20; i++) {
    const mid = (low + high) / 2;
    if (inGamut(l, mid, direction, gamut)) low = mid;
    else high = mid;
  }
  return low;
}

function toLab([r, g, b]) {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542683 * l + 0.7936177747 * m - 0.0040720430 * s,
    1.9779985324 * l - 2.4285922420 * m + 0.4505937096 * s,
    0.0259040425 * l + 0.7827717125 * m - 0.8086757549 * s,
  ];
}
const clip = rgb => rgb.map(v => clamp(v));
const encode = v => v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
export const hex = (rgb, gain = 1) => '#' + rgb.map(v =>
  Math.round(255 * gain * encode(v)).toString(16).padStart(2, '0')).join('');
export const clipped = (l, c, dir) => clip(linearRgb(l, c, dir));

export function preview(l, c, dir) {
  if (l === 0 || l === 1) return [l, l, l];
  let rgb = linearRgb(l, c, dir);
  if (inside(rgb)) return clip(rgb);
  let low = 0, high = c, chroma = c;
  let lowInside = true;
  let result = clip(rgb);
  while (high - low > 0.0001) {
    if (lowInside && inside(rgb)) low = chroma;
    else {
      result = clip(rgb);
      const [lightness, a, b] = toLab(result);
      const difference = Math.hypot(l - lightness, chroma * dir[0] - a, chroma * dir[1] - b);
      if (difference < 0.02) {
        if (chroma === c || 0.02 - difference < 0.0001) break;
        lowInside = false;
        low = chroma;
      } else high = chroma;
    }
    chroma = (low + high) / 2;
    rgb = linearRgb(l, chroma, dir);
  }
  return result;
}

// The native SDR picker adds a second fallback tile only when the quantized
// clipped and mapped previews differ by at least 5 CIEDE2000 units.
function labD50(rgb) {
  const linear = rgb.map(v => {
    const s = Math.round(255 * encode(v)) / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const [x, y, z] = multiply([
    [0.4360657, 0.3851515, 0.1430784], [0.2224932, 0.716887, 0.0606198],
    [0.0139239, 0.0970813, 0.7140994],
  ], ...linear);
  const f = t => t > (6 / 29) ** 3 ? Math.cbrt(t) : t * (29 / 6) ** 2 / 3 + 4 / 29;
  return [116 * f(y) - 16, 500 * (f(x / 0.964296) - f(y)), 200 * (f(y) - f(z / 0.825105))];
}

export function fallbackDistance(first, second) {
  const [l1, a1, b1] = labD50(first), [l2, a2, b2] = labD50(second);
  const ratio = c => Math.sqrt(c ** 7 / (c ** 7 + 25 ** 7));
  const correction = 1 + 0.5 * (1 - ratio((Math.hypot(a1, b1) + Math.hypot(a2, b2)) / 2));
  const rad = Math.PI / 180;
  const polar = (a, b) => [Math.hypot(a * correction, b), (Math.atan2(b, a * correction) / rad + 360) % 360];
  const [c1, h1] = polar(a1, b1), [c2, h2] = polar(a2, b2);
  const chroma = (c1 + c2) / 2, sum = h1 + h2;
  let delta = h2 - h1, hue = sum / 2;
  if (c1 * c2 === 0) { delta = 0; hue = sum; }
  else if (Math.abs(delta) > 180) {
    delta -= 360 * Math.sign(delta);
    hue += sum < 360 ? 180 : -180;
  }
  const cos = angle => Math.cos(rad * angle);
  const t = 1 - 0.17 * cos(hue - 30) + 0.24 * cos(2 * hue) + 0.32 * cos(3 * hue + 6) - 0.2 * cos(4 * hue - 63);
  const offset = ((l1 + l2) / 2 - 50) ** 2;
  const light = (l2 - l1) / (1 + 0.015 * offset / Math.sqrt(20 + offset));
  const c = (c2 - c1) / (1 + 0.045 * chroma);
  const h = 2 * Math.sqrt(c1 * c2) * Math.sin(rad * delta / 2) / (1 + 0.015 * chroma * t);
  const rotation = -2 * ratio(chroma) * Math.sin(rad * 60 * Math.exp(-(((hue - 275) / 25) ** 2)));
  return Math.sqrt(light ** 2 + c ** 2 + h ** 2 + rotation * c * h);
}

// Cache the working-gamut contour per hue. Dragging beyond it projects in
// screen space, as in the desktop picker, instead of clipping an RGB channel.
export function makeContour(direction, gamut, steps) {
  return Array.from({ length: steps + 1 }, (_, i) => boundary(1 - i / steps, direction, gamut));
}
export function project(x, y, contour, aspect, dir) {
  const l = clamp(1 - y), c = clamp(x) * MAX_CHROMA;
  if (inGamut(l, c, dir, 2)) return { l, c };
  const steps = contour.length - 1;
  let closest = 0, distance = Infinity;
  contour.forEach((edge, i) => {
    const dx = (Math.min(c, edge) / MAX_CHROMA - x) * aspect;
    const d = dx * dx + (i / steps - y) ** 2;
    if (d < distance) { distance = d; closest = i; }
  });
  let lo = Math.max(0, closest - 1) / steps, hi = Math.min(steps, closest + 1) / steps;
  const score = t => ((Math.min(c, boundary(1 - t, dir, 2)) / MAX_CHROMA - x) * aspect) ** 2 + (t - y) ** 2;
  for (let i = 0; i < 16; i++) {
    const a = (2 * lo + hi) / 3, b = (lo + 2 * hi) / 3;
    if (score(a) <= score(b)) hi = b;
    else lo = a;
  }
  const lightness = 1 - (lo + hi) / 2;
  return { l: lightness, c: Math.min(c, boundary(lightness, dir, 2)) };
}
