import { MAX_CHROMA, hex, hueDirection, inGamut, preview } from './picker-color.mjs';

export const plotWidth = 290;
export const plotHeight = 181;

// Use a scanline gradient per lightness. The three outlines follow the RGB
// cube faces, independently of the sRGB mapping used to display the pixels.
export function drawPlane(canvas, direction, contours) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const scale = canvas.width / plotWidth;
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  ctx.clearRect(0, 0, plotWidth, plotHeight);
  const xAt = chroma => chroma * plotWidth / MAX_CHROMA;
  const outer = contours[2];
  const rows = outer.length - 1;
  for (let y = 0; y < rows; y++) {
    const l = 1 - (y + 0.5) / rows;
    const edge = (outer[y] + outer[y + 1]) / 2;
    const end = xAt(edge);
    if (end <= 0) continue;
    const gradient = ctx.createLinearGradient(0, 0, end, 0);
    for (let stop = 0; stop <= 14; stop++) {
      gradient.addColorStop(stop / 14, hex(preview(l, edge * stop / 14, direction)));
    }
    ctx.fillStyle = gradient;
    ctx.fillRect(0, y * plotHeight / rows, end, plotHeight / rows);
  }
  ctx.lineWidth = 1 / scale;
  for (let gamut = 2; gamut >= 0; gamut--) {
    ctx.strokeStyle = gamut === 0 ? '#1f1d22' : '#161518';
    ctx.beginPath();
    contours[gamut].forEach((c, i) => {
      const x = xAt(c), y = plotHeight * i / rows;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }
}

export function drawHue(canvas, lightness, chroma) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  let previousMembership = -1;
  for (let x = 0; x < canvas.width; x++) {
    const direction = hueDirection(360 * (x + 0.5) / canvas.width);
    const membership = [0, 1, 2].reduce((mask, gamut) =>
      mask | (Number(inGamut(lightness, chroma, direction, gamut)) << gamut), 0);
    const crossing = x > 0 && membership !== previousMembership;
    ctx.fillStyle = crossing ? '#000' : hex(preview(lightness, chroma, direction), membership & 4 ? 1 : 0.5);
    ctx.fillRect(x, 0, 1, canvas.height);
    previousMembership = membership;
  }
}
