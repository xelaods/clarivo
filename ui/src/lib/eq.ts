// EQカーブの計算（Rust の clarivo-dsp と同じ RBJ の式）。

import type { Band, EqSettings, FilterKind } from "./types";

const SR = 48000;

interface Coefs {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

export function design(kind: FilterKind, freq: number, gainDb: number, q: number): Coefs {
  const f = Math.min(Math.max(freq, 10), SR * 0.49);
  const qq = Math.min(Math.max(q, 0.05), 40);
  const w0 = (2 * Math.PI * f) / SR;
  const cos = Math.cos(w0);
  const sin = Math.sin(w0);
  const alpha = sin / (2 * qq);
  const A = Math.pow(10, gainDb / 40);
  let b0 = 1, b1 = 0, b2 = 0, a0 = 1, a1 = 0, a2 = 0;
  switch (kind) {
    case "peaking":
      b0 = 1 + alpha * A; b1 = -2 * cos; b2 = 1 - alpha * A;
      a0 = 1 + alpha / A; a1 = -2 * cos; a2 = 1 - alpha / A;
      break;
    case "low_shelf": {
      const sq = 2 * Math.sqrt(A) * alpha;
      b0 = A * (A + 1 - (A - 1) * cos + sq); b1 = 2 * A * (A - 1 - (A + 1) * cos); b2 = A * (A + 1 - (A - 1) * cos - sq);
      a0 = A + 1 + (A - 1) * cos + sq; a1 = -2 * (A - 1 + (A + 1) * cos); a2 = A + 1 + (A - 1) * cos - sq;
      break;
    }
    case "high_shelf": {
      const sq = 2 * Math.sqrt(A) * alpha;
      b0 = A * (A + 1 + (A - 1) * cos + sq); b1 = -2 * A * (A - 1 + (A + 1) * cos); b2 = A * (A + 1 + (A - 1) * cos - sq);
      a0 = A + 1 - (A - 1) * cos + sq; a1 = 2 * (A - 1 - (A + 1) * cos); a2 = A + 1 - (A - 1) * cos - sq;
      break;
    }
    case "low_pass":
      b0 = (1 - cos) / 2; b1 = 1 - cos; b2 = (1 - cos) / 2; a0 = 1 + alpha; a1 = -2 * cos; a2 = 1 - alpha;
      break;
    case "high_pass":
      b0 = (1 + cos) / 2; b1 = -(1 + cos); b2 = (1 + cos) / 2; a0 = 1 + alpha; a1 = -2 * cos; a2 = 1 - alpha;
      break;
  }
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

export function magnitudeDb(c: Coefs, freq: number): number {
  const w = (2 * Math.PI * freq) / SR;
  const c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * c1 + c.b2 * c2;
  const ni = -(c.b1 * s1 + c.b2 * s2);
  const dr = 1 + c.a1 * c1 + c.a2 * c2;
  const di = -(c.a1 * s1 + c.a2 * s2);
  return 10 * Math.log10(Math.max((nr * nr + ni * ni) / Math.max(dr * dr + di * di, 1e-30), 1e-30));
}

export function bandResponse(b: Band, freq: number): number {
  if (!b.enabled) return 0;
  const gainless = (b.kind === "peaking" || b.kind === "low_shelf" || b.kind === "high_shelf") && Math.abs(b.gainDb) < 1e-3;
  if (gainless) return 0;
  return magnitudeDb(design(b.kind, b.freq, b.gainDb, b.q), freq);
}

export function eqResponse(eq: EqSettings, freq: number): number {
  return eq.bands.reduce((sum, b) => sum + bandResponse(b, freq), 0);
}

export function maxBoost(eq: EqSettings): number {
  let m = 0;
  for (let i = 0; i <= 240; i++) {
    const f = 20 * Math.pow(1000, i / 240);
    m = Math.max(m, eqResponse(eq, f));
  }
  return m;
}

export const KIND_LABELS: Record<FilterKind, string> = {
  peaking: "ピーク",
  low_shelf: "ローシェルフ",
  high_shelf: "ハイシェルフ",
  low_pass: "ローパス",
  high_pass: "ハイパス",
};

export const hasGain = (k: FilterKind) => k === "peaking" || k === "low_shelf" || k === "high_shelf";

export function formatFreq(f: number): string {
  return f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 1 : 2).replace(/\.?0+$/, "")}k` : `${Math.round(f)}`;
}
