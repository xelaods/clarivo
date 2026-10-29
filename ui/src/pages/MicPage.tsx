import { useState } from "react";
import { api } from "@/lib/api";
import { useMeters, useStore } from "@/lib/store";
import type { DenoiseMode } from "@/lib/types";
import { TorqueDial } from "@/components/easyui/TorqueDial";
import { MorphingButton, type ButtonStatusState } from "@/components/easyui/MorphingButton";
import { DeviceSelect, fmtDb, HMeter, Page, Pill, Row, Section, Segmented, Slider, Toggle } from "@/components/ui";

export function MicPage() {
  const { settings, update, devices, status, toast } = useStore();
  const m = useMeters();
  const mic = settings.mic;
  const st = status?.status.mic;

  return (
    <Page
      title="マイク"
      description="AIでノイズを消した声を仮想マイク（CABLE Output）に出します。Discord などではマイクに「CABLE Output」を選んでください。"
      actions={
        st?.running ? (
          <Pill tone="ok">遅延 約 {Math.round(st.latencyMs)} ms</Pill>
        ) : (
          <Pill tone="warn">{st?.error ?? "停止中"}</Pill>
        )
      }
    >
      <div className="grid grid-cols-[1fr_auto] items-center gap-8 rounded-[18px] border border-border bg-surface px-8 py-7">
        <div className="flex flex-col gap-5">
          <div>
            <div className="mb-2 flex justify-between text-[12px] text-text-muted">
              <span>入力（処理前）</span>
              <span className="tabular">{m ? `${Math.max(-60, m.micIn.peak).toFixed(0)} dB` : ""}</span>
            </div>
            <HMeter level={m?.micIn} className="h-2" />
          </div>
          <div>
            <div className="mb-2 flex justify-between text-[12px] text-text-muted">
              <span>仮想マイクへ（処理後）</span>
              <span className="tabular">{mic.muted ? "ミュート中" : m?.gateOpen ? "声を通しています" : "ゲートで閉じています"}</span>
            </div>
            <HMeter level={mic.muted ? null : m?.micOut} className="h-2" />
          </div>
          <Segmented<DenoiseMode>
            value={mic.denoise}
            onChange={(v) => update((s) => void (s.mic.denoise = v))}
            options={[
              { id: "off", label: "オフ" },
              { id: "light", label: "軽量（RNNoise）" },
              { id: "high", label: "高品質（DeepFilterNet3）" },
            ]}
          />
          <p className="text-[12px] leading-relaxed text-text-muted">
            {mic.denoise === "high"
              ? "キーボードや生活音もしっかり消します。遅延が約 10ms 増えます。"
              : mic.denoise === "light"
                ? "処理が軽く、ゲーム中の常用に向いています。"
                : "ノイズ除去をしません。"}
          </p>
        </div>
        <div className="flex flex-col items-center gap-2">
          <TorqueDial
            value={mic.strengthDb}
            min={0}
            max={100}
            step={1}
            size={132}
            unit=" dB"
            label="ノイズ除去の強さ"
            disabled={mic.denoise === "off"}
            onChange={(v) => update((s) => void (s.mic.strengthDb = v))}
          />
        </div>
      </div>

      <Section title="デバイス">
        <Row label="マイク">
          <DeviceSelect devices={devices.inputs.filter((d) => !/CABLE/i.test(d.name))} value={mic.inputDevice} autoLabel="既定の通信用マイク" onChange={(v) => update((s) => void (s.mic.inputDevice = v))} />
        </Row>
        <Row label="仮想マイクの出力先" description="通常は「CABLE Input」のままで大丈夫です。">
          <DeviceSelect devices={devices.outputs} value={mic.virtualOutput} autoLabel="CABLE Input（自動）" onChange={(v) => update((s) => void (s.mic.virtualOutput = v))} />
        </Row>
        <Row label="マイクを使う" description="オフにするとマイクの処理を止めます。">
          <Toggle checked={mic.enabled} onChange={(v) => update((s) => void (s.mic.enabled = v))} />
        </Row>
      </Section>

      <Section title="声の整え方">
        <Row label="ローカット" description="80Hz 以下の空調音や机の振動を落とします。">
          <Toggle checked={mic.highpass} onChange={(v) => update((s) => void (s.mic.highpass = v))} />
        </Row>
        <Row label="ノイズゲート" description="しきい値より小さい音を切ります。話し終わりの小さな雑音を消します。">
          <Toggle checked={mic.gate.enabled} onChange={(v) => update((s) => void (s.mic.gate.enabled = v))} />
          <Slider value={mic.gate.thresholdDb} min={-80} max={-20} step={1} format={(v) => `${v} dB`} width={200} disabled={!mic.gate.enabled} onChange={(v) => update((s) => void (s.mic.gate.thresholdDb = v))} />
        </Row>
        <Row label="コンプレッサー" description="声の大きさのばらつきをならします。">
          <Toggle checked={mic.compressor.enabled} onChange={(v) => update((s) => void (s.mic.compressor.enabled = v))} />
          <Slider
            value={mic.compressor.ratio}
            min={1.5}
            max={8}
            step={0.5}
            format={(v) => `${v}:1`}
            width={200}
            disabled={!mic.compressor.enabled}
            onChange={(v) => update((s) => void (s.mic.compressor.ratio = v))}
          />
        </Row>
        <Row label="入力の大きさ">
          <Slider value={mic.gainDb} min={-24} max={24} step={0.5} format={fmtDb} onChange={(v) => update((s) => void (s.mic.gainDb = v))} />
        </Row>
        <Row label="エコーキャンセル" description="スピーカーで聞いているとき、相手の声がマイクに戻るのを防ぎます。ヘッドホンなら不要です。">
          <Toggle checked={mic.echoCancel} onChange={(v) => update((s) => void (s.mic.echoCancel = v))} />
        </Row>
      </Section>

      <Section title="確認">
        <Row label="マイクモニター" description="処理後の自分の声をヘッドホンで聞きます。">
          <Toggle checked={mic.monitor} onChange={(v) => update((s) => void (s.mic.monitor = v))} />
          <Slider value={mic.monitorDb} min={-40} max={6} step={0.5} format={fmtDb} width={200} disabled={!mic.monitor} onChange={(v) => update((s) => void (s.mic.monitorDb = v))} />
        </Row>
        <RecordTest onError={(e) => toast({ title: "録音できませんでした", description: e, variant: "error" })} />
      </Section>
    </Page>
  );
}

function RecordTest({ onError }: { onError: (e: string) => void }) {
  const [state, setState] = useState<ButtonStatusState>("idle");
  const [result, setResult] = useState<{ raw: string; processed: string } | null>(null);
  const run = async () => {
    setState("loading");
    setResult(null);
    try {
      const r = await api.recordTest(10);
      setResult(r);
      setState("success");
      window.setTimeout(() => setState("idle"), 1500);
    } catch (e) {
      setState("error");
      onError(String(e));
      window.setTimeout(() => setState("idle"), 1500);
    }
  };
  return (
    <div className="border-t border-border-subtle px-5 py-4">
      <div className="flex items-center justify-between gap-6">
        <div>
          <div className="text-[13px] font-medium text-text-primary">録音テスト</div>
          <div className="mt-0.5 text-[12px] text-text-muted">10 秒録音して、処理前と処理後を聞き比べます。</div>
        </div>
        <MorphingButton status={state} idleText="録音する" loadingText="録音中（10 秒）" successText="録音しました" errorText="失敗しました" onClick={run} variant="secondary" />
      </div>
      {result && (
        <div className="mt-4 grid grid-cols-2 gap-4">
          <figure>
            <figcaption className="mb-1.5 text-[12px] text-text-muted">処理前</figcaption>
            <audio controls src={result.raw} className="h-9 w-full" />
          </figure>
          <figure>
            <figcaption className="mb-1.5 text-[12px] text-text-muted">処理後</figcaption>
            <audio controls src={result.processed} className="h-9 w-full" />
          </figure>
        </div>
      )}
    </div>
  );
}
