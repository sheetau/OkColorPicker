import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_CHROMA, boundary, hex, hueDirection, inGamut, initialColor,
  makeContour, preview, project,
} from "../src/components/picker-color.mjs";

test("the native initial OKLCH readout and sRGB swatch describe the same color", () => {
  const { l, c, h } = initialColor;
  assert.equal(hex(preview(l, c, hueDirection(h))), "#fd0f88");
  assert.equal(hex(preview(l, c, hueDirection(0))), hex(preview(l, c, hueDirection(360))));
});

test("the pink slice distinguishes the sRGB, Display P3 and Rec.2020 boundaries", () => {
  const direction = hueDirection(0);
  const l = 0.65;
  const [srgb, p3, rec2020] = [0, 1, 2].map(g => boundary(l, direction, g));
  assert.ok(0 < srgb && srgb < p3 && p3 < rec2020 && rec2020 < MAX_CHROMA);
  assert.equal(inGamut(l, (srgb + p3) / 2, direction, 0), false);
  assert.equal(inGamut(l, (srgb + p3) / 2, direction, 1), true);
});

test("dragging past the edge projects onto the Rec.2020 contour", () => {
  const direction = hueDirection(30);
  const contour = makeContour(direction, 2, 181);
  const { l, c } = project(1.2, 0.5, contour, 290 / 181, direction);
  assert.ok(inGamut(l, c, direction, 2));
  assert.ok(Math.abs(c - boundary(l, direction, 2)) < 0.00001);
});
