// 画面共通の部品。EasyUI に無いもの（スライダー、メーター、設定行）をここで作る。

import React from "react";
import { cn } from "@/lib/utils";
import { AnimatedTabs } from "@/components/easyui/AnimatedTabs";
import { SpringSelect } from "@/components/easyui/SpringSelect";
import { StretchSwitch } from "@/components/easyui/StretchSwitch";
import type { DeviceInfo, Level } from "@/lib/types";

export function Page({ title, description, actions, children }: {
  title: string;
  description?: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-[920px] px-10 pb-16 pt-8">
        <header className="mb-8 flex items-end justify-between gap-6">
          <div>
            <h1 className="text-[26px] font-semibold tracking-[-0.03em] text-text-primary">{title}</h1>
            {description && <p className="mt-1.5 max-w-[60ch] text-[13px] leading-relaxed text-text-secondary">{description}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </header>
        <div className="flex flex-col gap-10">{children}</div>
      </div>
    </div>
  );
}

export function Section({ title, description, aside, children, className }: {
  title: string;
  description?: string;
  aside?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      <div className="mb-3 flex items-end justify-between gap-4">
        <div>
          <h2 className="text-[15px] font-semibold tracking-[-0.015em] text-text-primary">{title}</h2>
          {description && <p className="mt-0.5 text-[12px] leading-relaxed text-text-muted">{description}</p>}
        </div>
        {aside}
      </div>
      <div className="rounded-[14px] border border-border bg-surface">{children}</div>
    </section>
  );
}

/** 設定 1 行：左に説明、右に操作部品。 */
export function Row({ label, description, children, className, disabled }: {
  label: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <div className={cn("flex min-h-[56px] items-center justify-between gap-6 border-b border-border-subtle px-5 py-3 last:border-b-0", disabled && "opacity-40", className)}>
      <div className="min-w-0">
        <div className="text-[13px] font-medium text-text-primary">{label}</div>
        {description && <div className="mt-0.5 text-[12px] leading-relaxed text-text-muted">{description}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-3">{children}</div>
    </div>
  );
}

export function Toggle({ checked, onChange, disabled }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return <StretchSwitch checked={checked} onChange={onChange} disabled={disabled} />;
}

/** 数値スライダー（右に値を表示）。 */
export function Slider({ value, min, max, step = 1, onChange, format, width = 220, disabled }: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
  width?: number;
  disabled?: boolean;
}) {
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <div className="flex items-center gap-3" style={{ width }}>
      <input
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        style={{ ["--fill" as string]: `${fill}%` }}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="tabular w-[64px] shrink-0 text-right text-[12px] text-text-secondary">{format ? format(value) : value}</span>
    </div>
  );
}

export const fmtDb = (v: number) => `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;

/** 選択肢の切り替え（EasyUI の AnimatedTabs を内容なしで使う）。 */
export function Segmented<T extends string>({ value, options, onChange, className }: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <AnimatedTabs
      tabs={options.map((o) => ({ id: o.id, label: o.label }))}
      activeTab={value}
      onChange={(id) => onChange(id as T)}
      renderContent={false}
      className={cn("gap-0", className)}
    />
  );
}

export function DeviceSelect({ devices, value, onChange, autoLabel = "自動で選ぶ", width = 300 }: {
  devices: DeviceInfo[];
  value: string | null;
  onChange: (v: string | null) => void;
  autoLabel?: string;
  width?: number;
}) {
  const options = [
    { value: "__auto__", label: autoLabel },
    ...devices.map((d) => ({ value: d.id, label: d.name })),
  ];
  return (
    <div style={{ width }}>
      <SpringSelect
        options={options}
        value={value ?? "__auto__"}
        onChange={(v) => onChange(v === "__auto__" ? null : v)}
      />
    </div>
  );
}

export function Button({ children, onClick, variant = "secondary", disabled, className, type = "button" }: {
  children: React.ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  disabled?: boolean;
  className?: string;
  type?: "button" | "submit";
}) {
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "focus-ring inline-flex h-8 items-center justify-center gap-1.5 rounded-[10px] px-3.5 text-[13px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        variant === "primary" && "bg-text-primary text-background hover:opacity-90",
        variant === "secondary" && "border border-border bg-surface-raised text-text-primary hover:border-border-hover hover:bg-surface-hover",
        variant === "ghost" && "text-text-secondary hover:bg-surface-hover hover:text-text-primary",
        variant === "danger" && "border border-[#4a2424] bg-[#2a1515] text-[#fca5a5] hover:bg-[#341a1a]",
        className,
      )}
    >
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// メーター
// ---------------------------------------------------------------------------

const FLOOR = -60;
const toPct = (db: number) => Math.min(100, Math.max(0, ((db - FLOOR) / -FLOOR) * 100));

function meterColor(db: number) {
  if (db > -1) return "var(--meter-clip)";
  if (db > -9) return "var(--meter-warn)";
  return "var(--meter-ok)";
}

/** 横向きのレベルメーター。RMS を帯、ピークを細線で表す。 */
export function HMeter({ level, className }: { level?: Level | null; className?: string }) {
  const rms = level ? toPct(level.rms) : 0;
  const peak = level ? toPct(level.peak) : 0;
  return (
    <div className={cn("relative h-1.5 w-full overflow-hidden rounded-full bg-surface-hover", className)}>
      <div className="absolute inset-y-0 left-0 rounded-full transition-[width] duration-75" style={{ width: `${rms}%`, background: meterColor(level?.rms ?? -120) }} />
      <div className="absolute inset-y-0 w-[2px] bg-text-primary/80" style={{ left: `calc(${peak}% - 1px)`, opacity: peak > 0 ? 1 : 0 }} />
    </div>
  );
}

/** 縦向きのメーター（ミキサーのチャンネル用）。 */
export function VMeter({ level, height = 220 }: { level?: Level | null; height?: number }) {
  const rms = level ? toPct(level.rms) : 0;
  const peak = level ? toPct(level.peak) : 0;
  return (
    <div className="relative w-[6px] overflow-hidden rounded-full bg-surface-hover" style={{ height }}>
      <div className="absolute inset-x-0 bottom-0 rounded-full transition-[height] duration-75" style={{ height: `${rms}%`, background: meterColor(level?.rms ?? -120) }} />
      <div className="absolute inset-x-0 h-[2px] bg-text-primary/80" style={{ bottom: `calc(${peak}% - 1px)`, opacity: peak > 0 ? 1 : 0 }} />
    </div>
  );
}

/** 縦フェーダー（-60〜+12 dB）。ダブルクリックで 0 dB に戻る。 */
export function VFader({ value, onChange, height = 220, min = -60, max = 12, disabled }: {
  value: number;
  onChange: (v: number) => void;
  height?: number;
  min?: number;
  max?: number;
  disabled?: boolean;
}) {
  const ref = React.useRef<HTMLDivElement>(null);
  const pct = ((value - min) / (max - min)) * 100;
  const zeroPct = ((0 - min) / (max - min)) * 100;
  const setFromY = (clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    const p = 1 - (clientY - r.top) / r.height;
    const v = Math.round((min + Math.min(1, Math.max(0, p)) * (max - min)) * 2) / 2;
    onChange(v);
  };
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      className={cn("focus-ring group relative w-8 cursor-ns-resize touch-none", disabled && "pointer-events-none opacity-40")}
      style={{ height }}
      onPointerDown={(e) => {
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        setFromY(e.clientY);
      }}
      onPointerMove={(e) => {
        if (e.buttons === 1) setFromY(e.clientY);
      }}
      onDoubleClick={() => onChange(0)}
      onKeyDown={(e) => {
        if (e.key === "ArrowUp") onChange(Math.min(max, value + 1));
        if (e.key === "ArrowDown") onChange(Math.max(min, value - 1));
      }}
      onWheel={(e) => onChange(Math.min(max, Math.max(min, value + (e.deltaY < 0 ? 1 : -1))))}
    >
      <div className="absolute left-1/2 top-0 h-full w-[4px] -translate-x-1/2 rounded-full bg-surface-hover" />
      <div className="absolute left-1/2 bottom-0 w-[4px] -translate-x-1/2 rounded-full bg-text-secondary" style={{ height: `${pct}%` }} />
      <div className="absolute left-1 right-1 h-px bg-border-hover" style={{ bottom: `${zeroPct}%` }} />
      <div
        className="absolute left-1/2 h-[10px] w-[26px] -translate-x-1/2 translate-y-1/2 rounded-[4px] border border-border-hover bg-text-primary shadow-[0_2px_6px_rgba(0,0,0,0.4)] transition-transform group-active:scale-105"
        style={{ bottom: `${pct}%` }}
      />
    </div>
  );
}

export function Pill({ children, tone = "neutral" }: { children: React.ReactNode; tone?: "neutral" | "ok" | "warn" | "off" }) {
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-medium",
        tone === "neutral" && "border-border bg-surface-raised text-text-secondary",
        tone === "ok" && "border-[#1e3a5f] bg-[#0f1d30] text-[#93c5fd]",
        tone === "warn" && "border-[#4a3b12] bg-[#2a210a] text-[#fcd34d]",
        tone === "off" && "border-border bg-transparent text-text-muted",
      )}
    >
      {children}
    </span>
  );
}

export function Notice({ children, action }: { children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-[12px] border border-[#4a3b12] bg-[#1d1708] px-4 py-3 text-[13px] text-[#fde68a]">
      <div className="leading-relaxed">{children}</div>
      {action}
    </div>
  );
}
