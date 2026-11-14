"use client";

import { useEffect, useMemo, useState } from "react";
import { useReducedMotion } from "framer-motion";
import { palette, swatch } from "./data";

// Port of CodyncKit/Design/CharacterAvatar.swift: the halftone character, dot for dot.

/** Rounded so server (Node) and browser math print the same SVG. */
const q = (v: number) => Math.round(v * 1000) / 1000;

export type Mood = "idle" | "working" | "needs";
type Pt = [number, number];

const ellipse = (x: number, y: number, w: number, h: number): Pt[] =>
  Array.from({ length: 48 }, (_, i) => {
    const a = (i / 48) * 2 * Math.PI;
    return [x + w / 2 + (Math.cos(a) * w) / 2, y + h / 2 + (Math.sin(a) * h) / 2];
  });

const roundedRect = (x: number, y: number, w: number, h: number, r: number): Pt[] => {
  const out: Pt[] = [];
  const corners: [number, number, number][] = [
    [x + w - r, y + r, -Math.PI / 2],
    [x + w - r, y + h - r, 0],
    [x + r, y + h - r, Math.PI / 2],
    [x + r, y + r, Math.PI],
  ];
  for (const [cx, cy, start] of corners)
    for (let i = 0; i <= 8; i++) {
      const a = start + (i / 8) * (Math.PI / 2);
      out.push([cx + Math.cos(a) * r, cy + Math.sin(a) * r]);
    }
  return out;
};

const quad = (p0: Pt, c: Pt, p1: Pt): Pt[] =>
  Array.from({ length: 16 }, (_, i) => {
    const t = (i + 1) / 16, u = 1 - t;
    return [u * u * p0[0] + 2 * u * t * c[0] + t * t * p1[0], u * u * p0[1] + 2 * u * t * c[1] + t * t * p1[1]];
  });

const cubic = (p0: Pt, c1: Pt, c2: Pt, p1: Pt): Pt[] =>
  Array.from({ length: 20 }, (_, i) => {
    const t = (i + 1) / 20, u = 1 - t;
    const k = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
    return [
      k[0] * p0[0] + k[1] * c1[0] + k[2] * c2[0] + k[3] * p1[0],
      k[0] * p0[1] + k[1] * c1[1] + k[2] * c2[1] + k[3] * p1[1],
    ];
  });

function inside(poly: Pt[], [x, y]: Pt) {
  let hit = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

function nearEdge(poly: Pt[], [x, y]: Pt, d: number) {
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [ax, ay] = poly[j], [bx, by] = poly[i];
    const t = Math.max(0, Math.min(1, ((x - ax) * (bx - ax) + (y - ay) * (by - ay)) / ((bx - ax) ** 2 + (by - ay) ** 2)));
    if (Math.hypot(x - ax - t * (bx - ax), y - ay - t * (by - ay)) <= d) return true;
  }
  return false;
}

/** CharacterShape: the eight silhouettes, as a point-in-shape test. */
function contains(kind: string, s: number): (p: Pt) => boolean {
  const w = s, h = s;
  switch (kind) {
    case "pebble": {
      const p = ellipse(0, h * 0.1, w, h * 0.8);
      return (pt) => inside(p, pt);
    }
    case "squircle": {
      const p = roundedRect(w * 0.04, h * 0.04, w * 0.92, h * 0.92, w * 0.3);
      return (pt) => inside(p, pt);
    }
    case "tablet": {
      const p = roundedRect(w * 0.14, 0, w * 0.72, h, w * 0.22);
      return (pt) => inside(p, pt);
    }
    case "wedge": {
      const a: Pt = [w * 0.5, h * 0.04], b: Pt = [w * 0.98, h * 0.86], c: Pt = [w * 0.02, h * 0.86];
      const p = [a, ...quad(a, [w * 0.9, h * 0.4], b), ...quad(b, [w * 0.5, h * 1.04], c), ...quad(c, [w * 0.1, h * 0.4], a)];
      return (pt) => inside(p, pt);
    }
    case "hex": {
      const p = Array.from({ length: 6 }, (_, i): Pt => {
        const a = (i * Math.PI) / 3 - Math.PI / 2;
        return [w / 2 + Math.cos(a) * w * 0.49, h / 2 + Math.sin(a) * h * 0.49];
      });
      // The polygon unioned with its round-joined stroke (width 0.08w).
      return (pt) => inside(p, pt) || nearEdge(p, pt, w * 0.04);
    }
    case "cloud": {
      const parts = [
        ellipse(0, h * 0.3, w * 0.55, h * 0.55),
        ellipse(w * 0.45, h * 0.3, w * 0.55, h * 0.55),
        ellipse(w * 0.18, h * 0.08, w * 0.64, h * 0.64),
        roundedRect(w * 0.1, h * 0.5, w * 0.8, h * 0.35, w * 0.15),
      ];
      return (pt) => parts.some((p) => inside(p, pt));
    }
    case "teardrop": {
      const top: Pt = [w * 0.5, 0], right: Pt = [w * 0.94, h * 0.62], left: Pt = [w * 0.06, h * 0.62];
      const arc = Array.from({ length: 24 }, (_, i): Pt => {
        const a = ((i + 1) / 24) * Math.PI;
        return [w * 0.5 + Math.cos(a) * w * 0.44, h * 0.62 + Math.sin(a) * w * 0.44];
      });
      const p = [top, ...cubic(top, [w * 0.62, h * 0.2], [w * 0.94, h * 0.38], right), ...arc, ...cubic(left, [w * 0.06, h * 0.38], [w * 0.38, h * 0.2], top)];
      return (pt) => inside(p, pt);
    }
    default: {
      const p = Array.from({ length: 64 }, (_, i): Pt => {
        const a = (i / 64) * 2 * Math.PI;
        const rr = 0.46 + 0.035 * Math.sin(a * 3 + 0.6);
        return [w / 2 + Math.cos(a) * w * rr, h / 2 + Math.sin(a) * h * rr];
      });
      return (pt) => inside(p, pt);
    }
  }
}

type Dot = { row: number; col: number; x: number; y: number };

function dotGrid(shape: string, size: number) {
  // 9 × 9 large dots from 18 px up; tiny ones 7 × 7, as in the apps.
  const cells = size < 18 ? 7 : 9;
  const step = size / cells;
  const test = contains(shape, size);
  const dots: Dot[] = [];
  for (let row = 0; row < cells; row++)
    for (let col = 0; col < cells; col++) {
      const x = (col + 0.5) * step, y = (row + 0.5) * step;
      if (test([x, y])) dots.push({ row, col, x, y });
    }
  return {
    step,
    dots,
    eyeColumns: cells === 7 ? [2, 4] : [3, 6],
    eyeRows: cells === 7 ? [2, 3] : [3, 4],
  };
}

type Pose = { yaw: number; glance: number; blinking: boolean; ripple: number | null };

/** DottedBody.inks: shades the dots as a lit ball, the eyes stay hollow. */
function inks(g: ReturnType<typeof dotGrid>, size: number, pose: Pose) {
  const eyeRows = pose.blinking ? g.eyeRows.slice(-1) : g.eyeRows;
  const eyeCols = g.eyeColumns.map((c) => c + pose.glance);
  const half = size / 2;
  const lx = Math.sin(pose.yaw) * 0.8, ly = 0.55, lz = Math.cos(pose.yaw) * 0.5 + 0.6;
  const ll = Math.hypot(lx, ly, lz);
  return g.dots
    .filter((d) => !(eyeCols.includes(d.col) && eyeRows.includes(d.row)))
    .map((d) => {
      const u = (d.x - half) / half, v = (half - d.y) / half;
      const z = Math.sqrt(Math.max(0.2, 1 - u * u - v * v));
      const nl = Math.hypot(u, v, z);
      let shade = 0.3 + 0.7 * Math.max(0, (u * lx + v * ly + z * lz) / (nl * ll));
      if (pose.ripple !== null) shade *= 0.6 + 0.4 * (0.5 + 0.5 * Math.sin(Math.hypot(u, v) * 9 - pose.ripple));
      return {
        d,
        r: g.step * 0.4 * (0.7 + 0.3 * shade),
        ink: 0.4 + 0.4 * shade,
        tint: shade > 0.6 ? (shade - 0.6) / 0.4 : 0,
      };
    });
}

/** Seconds since mount, ticking every frame while `run` (TimelineView(.animation)). */
export function useClock(run: boolean) {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (!run) return;
    let id = 0;
    const start = performance.now();
    const tick = (now: number) => {
      setT((now - start) / 1000);
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, [run]);
  return run ? t : 0;
}

/** `ink`: the grey dots' color (the app's text color by default; light on a dark page). */
export function CharacterAvatar({ shape, color, size, mood = "idle", ink: inkColor = palette.text }: { shape: string; color: string; size: number; mood?: Mood; ink?: string }) {
  const reduce = useReducedMotion();
  const g = useMemo(() => dotGrid(shape, size), [shape, size]);
  const still = mood === "idle" || !!reduce;
  const t = useClock(!still);
  const pose: Pose = {
    yaw: mood === "working" ? t * 1.4 : -0.7,
    glance: mood === "working" ? Math.round(Math.sin((t * 2 * Math.PI) / 3.2) * 1.4) : 0,
    // t > 0: the first (server-rendered) frame never blinks, whatever Reduce Motion says.
    blinking: !still && t > 0 && (t / 4.7) % 1 < 0.035,
    ripple: mood === "needs" ? t * 5 : null,
  };
  const tint = swatch(color);
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden className="block shrink-0">
      {inks(g, size, pose).map(({ d, r, ink, tint: k }) => (
        <g key={d.row * 100 + d.col}>
          <circle cx={q(d.x)} cy={q(d.y)} r={q(r)} fill={inkColor} fillOpacity={q(ink)} />
          {k > 0 && <circle cx={q(d.x)} cy={q(d.y)} r={q(r)} fill={tint} fillOpacity={q(k)} />}
        </g>
      ))}
    </svg>
  );
}

export type Face = { shape: string; color: string; mood?: Mood };

/** GroupAvatar for two members: tucked diagonally like iMessage. */
export function GroupAvatar({ members, size }: { members: Face[]; size: number }) {
  const side = size * 0.66;
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }} aria-hidden>
      <div className="absolute top-0 right-0">
        <CharacterAvatar {...members[1]} size={side} />
      </div>
      <div className="absolute bottom-0 left-0">
        <CharacterAvatar {...members[0]} size={side} />
      </div>
    </div>
  );
}

/** AvatarWithStatus: amber "!" when it needs you, an ink dot when unread. */
export function AvatarWithStatus({ face, members, unread, size }: { face: Face; members?: Face[]; unread: number; size: number }) {
  const ring = { boxShadow: `0 0 0 2px ${palette.background}` };
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      {members ? <GroupAvatar members={members} size={size} /> : <CharacterAvatar {...face} size={size} />}
      {face.mood === "needs" ? (
        <span
          className="absolute right-0 bottom-0 flex items-center justify-center rounded-full font-black text-white"
          style={{ width: size * 0.36, height: size * 0.36, fontSize: size * 0.26, background: palette.warning, ...ring }}
        >
          !
        </span>
      ) : unread > 0 ? (
        <span className="absolute right-0 bottom-0 rounded-full" style={{ width: size * 0.28, height: size * 0.28, background: palette.accentFill, ...ring }} />
      ) : null}
    </div>
  );
}
