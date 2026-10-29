// パラメトリックEQの編集画面。点をドラッグで周波数とゲイン、ホイールで Q を変える。

import React, { useMemo, useRef, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { bandResponse, eqResponse, formatFreq, hasGain, KIND_LABELS, maxBoost } from "@/lib/eq";
import type { Band, EqSettings, FilterKind } from "@/lib/types";
import { SpringSelect } from "@/components/easyui/SpringSelect";
import { Button, Toggle } from "./ui";

const W = 840;
const H = 240;
const PAD_L = 36;
const PAD_R = 12;
const PAD_T = 12;
const PAD_B = 22;
const FMIN = 20;
const FMAX = 20000;
const DB = 15;
const MAX_BANDS = 10;

const fx = (f: number) => PAD_L + (Math.log10(f / FMIN) / Math.log10(FMAX / FMIN)) * (W - PAD_L - PAD_R);
const xf = (x: number) => FMIN * Math.pow(FMAX / FMIN, (x - PAD_L) / (W - PAD_L - PAD_R));
const gy = (db: number) => PAD_T + ((DB - db) / (2 * DB)) * (H - PAD_T - PAD_B);
const yg = (y: number) => DB - ((y - PAD_T) / (H - PAD_T - PAD_B)) * 2 * DB;

const GRID_F = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
const GRID_DB = [-12, -6, 0, 6, 12];

export function EqEditor({ eq, onChange, readOnly }: { eq: EqSettings; onChange: (eq: EqSettings) => void; readOnly?: boolean }) {
  const [selected, setSelected] = useState<number | null>(eq.bands.length ? 0 : null);
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<number | null>(null);

  const path = useMemo(() => {
    const pts: string[] = [];
    for (let i = 0; i <= 200; i++) {
      const f = FMIN * Math.pow(FMAX / FMIN, i / 200);
      const db = Math.max(-DB, Math.min(DB, eqResponse(eq, f)));
      pts.push(`${i === 0 ? "M" : "L"}${fx(f).toFixed(1)},${gy(db).toFixed(1)}`);
    }
    return pts.join(" ");
  }, [eq]);

  const selectedCurve = useMemo(() => {
    if (selected === null || !eq.bands[selected]) return null;
    const b = eq.bands[selected];
    const pts: string[] = [];
    for (let i = 0; i <= 120; i++) {
      const f = FMIN * Math.pow(FMAX / FMIN, i / 120);
      const db = Math.max(-DB, Math.min(DB, bandResponse(b, f)));
      pts.push(`${i === 0 ? "M" : "L"}${fx(f).toFixed(1)},${gy(db).toFixed(1)}`);
    }
    return pts.join(" ");
  }, [eq, selected]);

  const setBand = (i: number, patch: Partial<Band>) => {
    const bands = eq.bands.map((b, j) => (j === i ? { ...b, ...patch } : b));
    onChange({ ...eq, bands });
  };

  const toSvg = (e: React.PointerEvent | React.WheelEvent) => {
    const r = svgRef.current!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * W, y: ((e.clientY - r.top) / r.height) * H };
  };

  const onMove = (e: React.PointerEvent) => {
    if (drag.current === null || readOnly) return;
    const { x, y } = toSvg(e);
    const i = drag.current;
    const b = eq.bands[i];
    const freq = Math.round(Math.min(FMAX, Math.max(FMIN, xf(x))));
    const gain = Math.round(Math.min(DB, Math.max(-DB, yg(y))) * 10) / 10;
    setBand(i, hasGain(b.kind) ? { freq, gainDb: gain } : { freq });
  };

  const addBand = () => {
    if (eq.bands.length >= MAX_BANDS) return;
    const used = eq.bands.map((b) => b.freq);
    const candidates = [1000, 250, 4000, 100, 8000, 500, 2000, 60, 12000, 160];
    const freq = candidates.find((c) => !used.includes(c)) ?? 1000;
    onChange({ ...eq, enabled: true, bands: [...eq.bands, { enabled: true, kind: "peaking", freq, gainDb: 0, q: 1 }] });
    setSelected(eq.bands.length);
  };

  const removeBand = (i: number) => {
    onChange({ ...eq, bands: eq.bands.filter((_, j) => j !== i) });
    setSelected(null);
  };

  const preamp = (eq.autoPreamp ? -maxBoost(eq) : 0) + eq.preampDb;
  const sel = selected !== null ? eq.bands[selected] : null;

  return (
    <div className={cn(!eq.enabled && "opacity-60")}>
      <svg
        ref={svgRef}
        viewBox={`0 0 ${W} ${H}`}
        className="block w-full touch-none select-none"
        onPointerMove={onMove}
        onPointerUp={() => (drag.current = null)}
        onPointerLeave={() => (drag.current = null)}
      >
        {GRID_DB.map((d) => (
          <g key={d}>
            <line x1={PAD_L} x2={W - PAD_R} y1={gy(d)} y2={gy(d)} stroke={d === 0 ? "var(--border-hover)" : "var(--border-subtle)"} />
            <text x={PAD_L - 8} y={gy(d) + 4} textAnchor="end" fontSize="10" fill="var(--text-muted)">
              {d > 0 ? `+${d}` : d}
            </text>
          </g>
        ))}
        {GRID_F.map((f) => (
          <g key={f}>
            <line x1={fx(f)} x2={fx(f)} y1={PAD_T} y2={H - PAD_B} stroke="var(--border-subtle)" />
            <text x={fx(f)} y={H - 6} textAnchor="middle" fontSize="10" fill="var(--text-muted)">
              {formatFreq(f)}
            </text>
          </g>
        ))}
        <path d={`${path} L${W - PAD_R},${gy(0)} L${PAD_L},${gy(0)} Z`} fill="var(--indicator-active)" opacity={0.08} />
        {selectedCurve && <path d={selectedCurve} fill="none" stroke="var(--text-muted)" strokeDasharray="3 3" strokeWidth={1} />}
        <path d={path} fill="none" stroke="var(--indicator-active)" strokeWidth={2} strokeLinejoin="round" />
        {eq.bands.map((b, i) => {
          const y = hasGain(b.kind) ? gy(Math.max(-DB, Math.min(DB, b.gainDb))) : gy(0);
          const active = i === selected;
          return (
            <g
              key={i}
              className={readOnly ? "" : "cursor-grab active:cursor-grabbing"}
              onPointerDown={(e) => {
                if (readOnly) return;
                (e.currentTarget.ownerSVGElement as SVGSVGElement).setPointerCapture(e.pointerId);
                drag.current = i;
                setSelected(i);
              }}
              onWheel={(e) => {
                if (readOnly) return;
                const q = Math.min(10, Math.max(0.1, b.q * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
                setBand(i, { q: Math.round(q * 100) / 100 });
              }}
            >
              <circle cx={fx(b.freq)} cy={y} r={14} fill="transparent" />
              <circle
                cx={fx(b.freq)}
                cy={y}
                r={active ? 7 : 5.5}
                fill={b.enabled ? (active ? "var(--text-primary)" : "var(--bg)") : "var(--surface-hover)"}
                stroke={b.enabled ? "var(--text-primary)" : "var(--text-muted)"}
                strokeWidth={1.5}
              />
              <text x={fx(b.freq)} y={y - 12} textAnchor="middle" fontSize="10" fill="var(--text-secondary)">
                {i + 1}
              </text>
            </g>
          );
        })}
      </svg>

      <div className="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-border-subtle pt-3">
        <div className="flex items-center gap-2 text-[12px] text-text-muted">
          <span>プリアンプ</span>
          <span className="tabular text-text-secondary">{preamp > 0 ? "+" : ""}{preamp.toFixed(1)} dB</span>
          {eq.autoPreamp && <span>音割れしないよう自動で調整</span>}
        </div>
        {!readOnly && (
          <Button onClick={addBand} disabled={eq.bands.length >= MAX_BANDS}>
            <Plus className="h-3.5 w-3.5" /> バンドを追加
          </Button>
        )}
      </div>

      {sel && selected !== null && !readOnly && (
        <div className="mt-3 grid grid-cols-[auto_180px_1fr_1fr_1fr_auto] items-end gap-3 rounded-[12px] border border-border-subtle bg-surface-raised p-3">
          <div className="pb-1.5">
            <Toggle checked={sel.enabled} onChange={(v) => setBand(selected, { enabled: v })} />
          </div>
          <label className="flex flex-col gap-1 text-[11px] text-text-muted">
            種類
            <SpringSelect
              options={(Object.keys(KIND_LABELS) as FilterKind[]).map((k) => ({ value: k, label: KIND_LABELS[k] }))}
              value={sel.kind}
              onChange={(v) => setBand(selected, { kind: v as FilterKind })}
            />
          </label>
          <NumberField label="周波数（Hz）" value={sel.freq} min={20} max={20000} step={1} onChange={(v) => setBand(selected, { freq: v })} />
          <NumberField label="ゲイン（dB）" value={sel.gainDb} min={-15} max={15} step={0.1} disabled={!hasGain(sel.kind)} onChange={(v) => setBand(selected, { gainDb: v })} />
          <NumberField label="Q（幅）" value={sel.q} min={0.1} max={10} step={0.01} onChange={(v) => setBand(selected, { q: v })} />
          <button
            className="focus-ring mb-0.5 flex h-8 w-8 items-center justify-center rounded-[8px] text-text-muted hover:bg-surface-hover hover:text-text-primary"
            onClick={() => removeBand(selected)}
            aria-label="このバンドを削除"
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      )}
    </div>
  );
}

function NumberField({ label, value, min, max, step, onChange, disabled }: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  disabled?: boolean;
}) {
  const [text, setText] = useState(String(value));
  React.useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = Number(text);
    if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, v)));
    else setText(String(value));
  };
  return (
    <label className={cn("flex flex-col gap-1 text-[11px] text-text-muted", disabled && "opacity-40")}>
      {label}
      <input
        className="text-input tabular w-full"
        value={text}
        disabled={disabled}
        inputMode="decimal"
        step={step}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
    </label>
  );
}
