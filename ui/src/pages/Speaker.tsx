import { useEffect, useState } from "react";
import { Download, FileUp, Trash2, Check } from "lucide-react";
import { cn } from "@/lib/utils";
import { api, listen, pickFile } from "@/lib/api";
import { useMeters, useStore } from "@/lib/store";
import type { SpeakerMode } from "@/lib/types";
import { Stepper } from "@/components/easyui/stepper";
import { MorphingButton, type ButtonStatusState } from "@/components/easyui/MorphingButton";
import { Button, Notice, Page, Row, Section, Segmented, Slider } from "@/components/ui";

const SCRIPT =
  "今日はいい天気ですね。最近は夜遅くまでゲームをすることが多くて、つい寝不足になってしまいます。週末は友だちと集まって、新しいマップを練習する予定です。作戦を決めて、声をかけ合いながら進めるのが一番楽しいと思います。もし音が聞き取りにくいときは、遠慮なく教えてください。";

const RECORD_SECONDS = 30;

export function SpeakerPage() {
  const { settings, update, speakerProfiles, speakerModelReady } = useStore();
  const sp = settings.mic.speaker;
  const m = useMeters();

  const stateLabel = {
    self: "あなたの声",
    other: "ほかの人の声（下げています）",
    checking: "判定中",
    silent: "声はありません",
  }[m?.speakerState ?? "silent"];

  return (
    <Page title="声の登録" description="自分の声を登録すると、家族やテレビなど、ほかの人の声をマイクから外せます。">
      <Section title="使い方">
        <div className="flex items-center justify-between gap-6 px-5 py-4">
          <Segmented<SpeakerMode>
            value={sp.mode}
            onChange={(v) => update((s) => void (s.mic.speaker.mode = v))}
            options={[
              { id: "off", label: "使わない" },
              { id: "gate", label: "話者ゲート" },
              { id: "extract", label: "話者抽出（試験機能）" },
            ]}
          />
        </div>
        <p className="border-t border-border-subtle px-5 py-3 text-[12px] leading-relaxed text-text-muted">
          {sp.mode === "gate" &&
            "あなたが話していないとき、ほかの人の声を下げます。あなたと同時に話された声は残ります。判定に約 0.5 秒かかるため、ほかの人の話し始めは少し聞こえることがあります。"}
          {sp.mode === "extract" &&
            "声が重なっても、あなたの声だけを残すモデルを使います。モデル（ONNX）は別途用意が必要です。遅延が約 0.2 秒増えます。"}
          {sp.mode === "off" && "マイクの音はノイズ除去だけで出します。"}
        </p>
        {sp.mode !== "off" && (
          <>
            <Row label="今の判定" description={m?.speakerScore != null ? `一致度 ${(m.speakerScore * 100).toFixed(0)}%` : "話すと判定します"}>
              <span
                className={cn(
                  "inline-flex h-7 items-center rounded-full border px-3 text-[12px] font-medium",
                  m?.speakerState === "self" && "border-[#1e3a5f] bg-[#0f1d30] text-[#93c5fd]",
                  m?.speakerState === "other" && "border-[#4a3b12] bg-[#2a210a] text-[#fcd34d]",
                  (!m || m.speakerState === "silent" || m.speakerState === "checking") && "border-border text-text-muted",
                )}
              >
                {stateLabel}
              </span>
            </Row>
            <Row label="判定の厳しさ" description="高いほど、あなたの声と判定する条件が厳しくなります。自分の声が切れるときは下げてください。">
              <Slider value={Math.round(sp.threshold * 100)} min={20} max={80} format={(v) => `${v}%`} onChange={(v) => update((s) => void (s.mic.speaker.threshold = v / 100))} />
            </Row>
            <Row label="ほかの人の声を下げる量">
              <Slider value={sp.attenuationDb} min={-60} max={-6} step={1} format={(v) => `${v} dB`} onChange={(v) => update((s) => void (s.mic.speaker.attenuationDb = v))} />
            </Row>
          </>
        )}
        {sp.mode === "extract" && (
          <Row label="話者抽出モデル" description={sp.extractModel ?? "入力：混合音 [1, 8000]・声紋 [1, 256]、出力：[1, 8000]（16kHz）"}>
            <Button
              onClick={async () => {
                const p = await pickFile([{ name: "ONNX モデル", extensions: ["onnx"] }]);
                if (p) update((s) => void (s.mic.speaker.extractModel = p));
              }}
            >
              <FileUp className="h-3.5 w-3.5" /> モデルを選ぶ
            </Button>
          </Row>
        )}
      </Section>

      {!speakerModelReady ? <ModelDownload /> : <Enroll />}

      <Section title="登録した声" description="使う声を 1 つ選びます。">
        {speakerProfiles.length === 0 ? (
          <div className="px-5 py-8 text-center text-[13px] text-text-muted">まだ登録されていません。上の手順で 30 秒ほど読み上げて登録してください。</div>
        ) : (
          speakerProfiles.map((p) => <ProfileRow key={p.id} id={p.id} name={p.name} created={p.created} seconds={p.voicedSeconds} selected={sp.profile === p.id} />)
        )}
      </Section>
    </Page>
  );
}

function ProfileRow({ id, name, created, seconds, selected }: { id: string; name: string; created: number; seconds: number; selected: boolean }) {
  const { update, refreshSpeakers, toast } = useStore();
  const date = new Date(created * 1000).toLocaleDateString("ja-JP");
  return (
    <div className="flex min-h-[56px] items-center justify-between gap-6 border-b border-border-subtle px-5 py-3 last:border-b-0">
      <button className="focus-ring flex items-center gap-3 text-left" onClick={() => update((s) => void (s.mic.speaker.profile = id))}>
        <span className={cn("flex h-5 w-5 items-center justify-center rounded-full border", selected ? "border-text-primary bg-text-primary text-background" : "border-border-hover")}>
          {selected && <Check className="h-3 w-3" />}
        </span>
        <span>
          <span className="block text-[13px] font-medium text-text-primary">{name}</span>
          <span className="block text-[12px] text-text-muted">
            {date} に登録 ・ 声 {seconds.toFixed(0)} 秒
          </span>
        </span>
      </button>
      <Button
        variant="ghost"
        onClick={async () => {
          try {
            await api.deleteSpeakerProfile(id);
            await refreshSpeakers();
          } catch (e) {
            toast({ title: "削除できません", description: String(e), variant: "error" });
          }
        }}
      >
        <Trash2 className="h-3.5 w-3.5" /> 削除
      </Button>
    </div>
  );
}

function ModelDownload() {
  const { setSpeakerModelReady, toast } = useStore();
  const [progress, setProgress] = useState<{ received: number; total: number | null } | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const off = listen<{ received: number; total: number | null }>("model-progress", setProgress);
    return () => {
      off.then((f) => f());
    };
  }, []);

  const start = async () => {
    setBusy(true);
    try {
      await api.downloadSpeakerModel();
      setSpeakerModelReady(true);
      toast({ title: "話者モデルの準備ができました", variant: "success" });
    } catch (e) {
      toast({ title: "ダウンロードできません", description: String(e), variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const pct = progress?.total ? Math.round((progress.received / progress.total) * 100) : null;

  return (
    <Section title="準備" description="声の判定には話者認識モデル（WeSpeaker ResNet34、約 26MB）を使います。初回だけダウンロードが必要です。">
      <div className="flex items-center justify-between gap-6 px-5 py-4">
        <div className="text-[12px] text-text-muted">
          {busy ? (pct !== null ? `ダウンロード中 ${pct}%` : "ダウンロード中…") : "モデルは Hugging Face から取得します（CC BY 4.0）。"}
        </div>
        <Button variant="primary" onClick={start} disabled={busy}>
          <Download className="h-3.5 w-3.5" /> ダウンロード
        </Button>
      </div>
      {busy && pct !== null && (
        <div className="mx-5 mb-4 h-1 overflow-hidden rounded-full bg-surface-hover">
          <div className="h-full bg-accent-blue transition-[width]" style={{ width: `${pct}%` }} />
        </div>
      )}
    </Section>
  );
}

function Enroll() {
  const { refreshSpeakers, toast, status } = useStore();
  const [step, setStep] = useState(0);
  const [name, setName] = useState("自分");
  const [btn, setBtn] = useState<ButtonStatusState>("idle");
  const [remaining, setRemaining] = useState(RECORD_SECONDS);

  useEffect(() => {
    if (btn !== "loading") return;
    setRemaining(RECORD_SECONDS);
    const id = window.setInterval(() => setRemaining((r) => Math.max(0, r - 1)), 1000);
    return () => window.clearInterval(id);
  }, [btn]);

  const record = async () => {
    setBtn("loading");
    try {
      await api.enrollSpeaker(name, RECORD_SECONDS);
      await refreshSpeakers();
      setBtn("success");
      setStep(2);
    } catch (e) {
      setBtn("error");
      toast({ title: "登録できませんでした", description: String(e), variant: "error" });
      window.setTimeout(() => setBtn("idle"), 1500);
    }
  };

  const micRunning = status?.status.mic.running;

  return (
    <Section title="声を登録する">
      <div className="px-5 pt-5">
        <Stepper
          current={step}
          onStepChange={(i) => i < step && setStep(i)}
          steps={[
            { title: "準備", description: "静かな場所で、普段のマイクを使います" },
            { title: "読み上げ", description: `${RECORD_SECONDS} 秒` },
            { title: "完了" },
          ]}
        />
      </div>
      <div className="px-5 pb-5 pt-6">
        {step === 0 && (
          <div className="flex items-end justify-between gap-6">
            <label className="flex flex-col gap-1.5 text-[12px] text-text-muted">
              登録名
              <input className="text-input w-[220px]" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <Button variant="primary" onClick={() => setStep(1)} disabled={!micRunning}>
              次へ
            </Button>
          </div>
        )}
        {step === 0 && !micRunning && <div className="mt-4"><Notice>マイクが動いていません。「マイク」のページでデバイスを確認してください。</Notice></div>}
        {step === 1 && (
          <div>
            <p className="rounded-[12px] border border-border-subtle bg-surface-raised px-5 py-4 text-[15px] leading-[1.9] text-text-primary">{SCRIPT}</p>
            <div className="mt-4 flex items-center justify-between gap-6">
              <span className="text-[12px] text-text-muted">
                {btn === "loading" ? `普段の声の大きさで読み上げてください（残り ${remaining} 秒）` : "録音を始めたら、上の文をゆっくり読み上げてください。読み終わったら繰り返して構いません。"}
              </span>
              <MorphingButton status={btn} idleText="録音を始める" loadingText="録音中" successText="登録しました" errorText="もう一度" onClick={record} variant="primary" />
            </div>
          </div>
        )}
        {step === 2 && (
          <div className="flex items-center justify-between gap-6">
            <span className="text-[13px] text-text-secondary">登録しました。「使い方」で話者ゲートを選ぶと使えます。</span>
            <Button
              onClick={() => {
                setBtn("idle");
                setStep(0);
              }}
            >
              別の声を登録
            </Button>
          </div>
        )}
      </div>
    </Section>
  );
}
