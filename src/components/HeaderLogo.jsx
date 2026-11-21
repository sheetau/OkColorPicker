import { useEffect, useId, useRef } from "react";
import { localePath } from "../site/config.mjs";

const restingOutline = "8,0 14.93,4.01 14.93,12.01 8,16 1.07,12.01 1.07,4.01";
const idleDelay = 3000;
const turnDuration = 1520;

// Sample the reference silhouette at tenths of the turn, then interpolate
// the angles so the SVG stays smooth on displays faster than the 25 fps clip.
const turnAngles = [0, 4, 20, 53, 112, 185, 262, 311, 342, 355, 360];
function angleAt(elapsed) {
  const sample = 10 * elapsed / turnDuration;
  const i = Math.min(9, Math.floor(sample));
  const t = sample - i;
  const a = turnAngles[i], b = turnAngles[i + 1];
  const startSlope = i === 0 ? 0 : (b - turnAngles[i - 1]) / 2;
  const endSlope = i === 9 ? 0 : (turnAngles[i + 2] - a) / 2;
  const degrees = a + (b - a) * t * t * (3 - 2 * t)
    + t * (1 - t) * ((1 - t) * startSlope - t * endSlope);
  return degrees * Math.PI / 180;
}

function silhouette(angle) {
  const cos = Math.cos(angle), sin = Math.sin(angle);
  const points = [];
  // The axis slopes down to the right by 60°. Its perpendicular points
  // up and right, the direction in which the front of the cube rolls.
  const axisX = 0.5, axisY = Math.sqrt(0.75);
  for (const x of [-1, 1]) {
    for (const y of [-1, 1]) {
      for (const z of [-1, 1]) {
        const right = Math.sqrt(12) * (x - z);
        const down = 2 * (x + z - 2 * y);
        const depth = Math.sqrt(8) * (x + y + z);
        const along = right * axisX + down * axisY;
        const across = (right * axisY - down * axisX) * cos + depth * sin;
        points.push([8 + along * axisX + across * axisY, 8 + along * axisY - across * axisX]);
      }
    }
  }
  points.sort(([x, y], [otherX, otherY]) => x - otherX || y - otherY);
  function hullHalf(vertices) {
    const hull = [];
    for (const p of vertices) {
      while (hull.length >= 2) {
        const [a, b] = hull.slice(-2);
        if ((b[0] - a[0]) * (p[1] - a[1]) > (b[1] - a[1]) * (p[0] - a[0])) break;
        hull.pop();
      }
      hull.push(p);
    }
    hull.pop();
    return hull;
  }
  return [...hullHalf(points), ...hullHalf(points.reverse())].map(p => p.join(",")).join(" ");
}

export default function HeaderLogo({ locale }) {
  const link = useRef(null);
  const hole = useRef(null);
  const maskId = useId();
  useEffect(() => {
    const element = link.current;
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    let visible = false, frame = 0, timer = 0;
    const allowed = () => visible && !document.hidden && !reduced.matches;
    function schedule() {
      clearTimeout(timer);
      if (allowed()) timer = window.setTimeout(turn, idleDelay);
    }
    function turn() {
      if (!allowed() || frame) return;
      clearTimeout(timer);
      const start = performance.now();
      function draw(now) {
        const elapsed = Math.max(0, now - start);
        if (elapsed < turnDuration) {
          hole.current.setAttribute("points", silhouette(angleAt(elapsed)));
          frame = requestAnimationFrame(draw);
        } else {
          refresh();
        }
      }
      frame = requestAnimationFrame(draw);
    }
    function refresh() {
      cancelAnimationFrame(frame);
      frame = 0;
      hole.current.setAttribute("points", restingOutline);
      schedule();
    }
    const observer = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      refresh();
    });
    observer.observe(element);
    element.addEventListener("pointerenter", turn);
    element.addEventListener("focus", turn);
    document.addEventListener("visibilitychange", refresh);
    reduced.addEventListener("change", refresh);
    return () => {
      clearTimeout(timer);
      cancelAnimationFrame(frame);
      observer.disconnect();
      element.removeEventListener("pointerenter", turn);
      element.removeEventListener("focus", turn);
      document.removeEventListener("visibilitychange", refresh);
      reduced.removeEventListener("change", refresh);
    };
  }, []);

  return <a ref={link} className="brand" href={localePath(locale)} aria-label="OkColorPicker">
    <svg width="24" height="24" viewBox="0 0 16 16" aria-hidden="true">
      <defs>
        <mask id={maskId} maskUnits="userSpaceOnUse" x="0" y="0" width="16" height="16">
          <rect width="16" height="16" fill="white" />
          <polygon ref={hole} points={restingOutline} fill="black" />
        </mask>
      </defs>
      <path d="M2.14 0h11.72L16 2.14v11.72L13.86 16H2.14L0 13.86V2.14Z" fill="currentColor" mask={`url(#${maskId})`} />
    </svg>
    <span>OkColorPicker</span>
  </a>;
}
