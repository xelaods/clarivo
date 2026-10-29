// 初回起動時のセットアップ。VB-CABLE の有無を確かめ、マイクとヘッドホンを選び、
// Discord などでの設定方法を案内する。設定ページからいつでも開き直せる。

import { useState } from "react";
import { Check, ExternalLink, RefreshCw, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { openUrl } from "@/lib/api";
import { useStore } from "@/lib/store";
import { SettleModal } from "@/components/easyui/SettleModal";
import { Stepper, StepPanels } from "@/components/easyui/stepper";
import { Button, DeviceSelect } from "@/components/ui";

const VB_CABLE_URL = "https://vb-audio.com/Cable/";

export function SetupGuide({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { status, devices, settings, update, refreshDevices } = useStore();
  const [step, setStep] = useState(0);
  const [checking, setChecking] = useState(false);
  const cables = status?.status.cables;

  const finish = () => {
    update((s) => void (s.app.onboarded = true));
    setStep(0);
    onClose();
  };

  const recheck = async () => {
    setChecking(true);
    try {
      await refreshDevices();
    } finally {
      // 状態（VB-CABLE の有無）は 2 秒おきに更新されるので少し待つ
      window.setTimeout(() => setChecking(false), 2200);
    }
  };

  const steps = [
    { title: "仮想デバイス" },
    { title: "マイク" },
    { title: "出力" },
  ];

  return (
    <SettleModal
      open={open}
      onClose={finish}
      size="lg"
      title="Clarivo へようこそ"
      description="3 つの手順で使えるようになります。あとから設定ページで変更できます。"
      closeOnBackdrop={false}
      className="max-w-[620px]"
      footer={
        <div className="flex items-center justify-between">
          <Button variant="ghost" onClick={finish}>
            スキップ
          </Button>
          <div className="flex gap-2">
            {step > 0 && <Button onClick={() => setStep(step - 1)}>戻る</Button>}
            {step < steps.length - 1 ? (
              <Button variant="primary" onClick={() => setStep(step + 1)}>
                次へ
              </Button>
            ) : (
              <Button variant="primary" onClick={finish}>
                はじめる
              </Button>
            )}
          </div>
        </div>
      }
    >
      <Stepper current={step} steps={steps} onStepChange={(i) => setStep(i)} className="mb-6" />
      <StepPanels current={step}>
        <div className="flex flex-col gap-4">
          <p className="text-[13px] leading-relaxed text-text-secondary">
            Clarivo は VB-Audio 社の仮想デバイス「VB-CABLE」を使って、処理した声をほかのアプリへ渡したり、ゲームと通話の音を分けたりします。
          </p>
          <div className="rounded-[12px] border border-border-subtle">
            <CableRow ok={cables?.mic} name="VB-CABLE" use="仮想マイク（処理した声を Discord などへ）" />
            <CableRow ok={cables?.game} name="VB-CABLE A" use="Game チャンネル" />
            <CableRow ok={cables?.chat} name="VB-CABLE B" use="Chat チャンネル（通話の声）" />
          </div>
          <p className="text-[12px] leading-relaxed text-text-muted">
            VB-CABLE は無料、A と B は寄付で入手する追加パック（A+B）です。インストールしたら Windows を再起動してください。
            足りないものがあっても、ある分の機能は使えます。
          </p>
          <div className="flex gap-2">
            <Button onClick={() => openUrl(VB_CABLE_URL)}>
              <ExternalLink className="h-3.5 w-3.5" /> VB-CABLE のページを開く
            </Button>
            <Button variant="ghost" onClick={recheck} disabled={checking}>
              <RefreshCw className={cn("h-3.5 w-3.5", checking && "animate-spin")} /> もう一度確認
            </Button>
          </div>
        </div>

        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between gap-4">
            <span className="text-[13px] font-medium text-text-primary">使うマイク</span>
            <DeviceSelect
              devices={devices.inputs.filter((d) => !/cable|vb-audio|voicemeeter/i.test(d.name))}
              value={settings.mic.inputDevice}
              autoLabel="既定の通信用マイク"
              onChange={(v) => update((s) => void (s.mic.inputDevice = v))}
            />
          </div>
          <div className="rounded-[12px] border border-border-subtle bg-surface-raised px-4 py-3 text-[13px] leading-relaxed text-text-secondary">
            Discord・Zoom・ゲーム内ボイスチャットのマイク（入力デバイス）に
            <span className="mx-1 rounded bg-surface-hover px-1.5 py-0.5 font-medium text-text-primary">CABLE Output (VB-Audio Virtual Cable)</span>
            を選んでください。Clarivo でノイズを消した声が届きます。
          </div>
          <p className="text-[12px] leading-relaxed text-text-muted">
            Discord 側のノイズ抑制（Krisp）やエコー除去は、二重にかからないようオフにするのがおすすめです。
          </p>
        </div>

        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between gap-4">
            <span className="text-[13px] font-medium text-text-primary">実際に聞くヘッドホン</span>
            <DeviceSelect
              devices={devices.outputs.filter((d) => !/cable|vb-audio|voicemeeter/i.test(d.name))}
              value={settings.output.headphones}
              autoLabel="既定のデバイス"
              onChange={(v) => update((s) => void (s.output.headphones = v))}
            />
          </div>
          <div className="rounded-[12px] border border-border-subtle bg-surface-raised px-4 py-3 text-[13px] leading-relaxed text-text-secondary">
            Clarivo は Windows の既定の再生デバイスを Game（VB-CABLE A）に、既定の通信デバイスを Chat（VB-CABLE B）に切り替え、
            通話アプリを自動で Chat に振り分けます。Discord の出力デバイスは「Default」のままで大丈夫です。
          </div>
          <p className="text-[12px] leading-relaxed text-text-muted">
            Clarivo を終了すると、既定デバイスとアプリ別の出力先は元に戻ります。
          </p>
        </div>
      </StepPanels>
    </SettleModal>
  );
}

function CableRow({ ok, name, use }: { ok: boolean | undefined; name: string; use: string }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border-subtle px-4 py-2.5 last:border-b-0">
      <div>
        <div className="text-[13px] font-medium text-text-primary">{name}</div>
        <div className="text-[12px] text-text-muted">{use}</div>
      </div>
      <span
        className={cn(
          "inline-flex h-6 items-center gap-1 rounded-full border px-2.5 text-[11px] font-medium",
          ok ? "border-[#1e3a5f] bg-[#0f1d30] text-[#93c5fd]" : "border-[#4a3b12] bg-[#2a210a] text-[#fcd34d]",
        )}
      >
        {ok ? <Check className="h-3 w-3" /> : <X className="h-3 w-3" />}
        {ok ? "見つかりました" : "見つかりません"}
      </span>
    </div>
  );
}
