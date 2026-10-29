import { Headphones, Gamepad2, MessagesSquare, Mic, VolumeX, Volume2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { openUrl } from "@/lib/api";
import { useMeters, useStore } from "@/lib/store";
import type { Level } from "@/lib/types";
import { Button, DeviceSelect, fmtDb, Notice, Page, Row, Section, Slider, Toggle, VFader, VMeter } from "@/components/ui";

export function MixerPage({ onNavigate }: { onNavigate: (page: string) => void }) {
  const { settings, update, presets, status, devices } = useStore();
  const m = useMeters();
  const cables = status?.status.cables;
  const missing = cables && (!cables.mic || !cables.game || !cables.chat);
  const presetName = (id: string) => presets.find((p) => p.id === id)?.name ?? "カスタム";

  const chatmixPct = Math.round(((settings.output.chatmix + 1) / 2) * 100);

  return (
    <Page title="ミキサー" description="ゲームと通話の音量、マイク、ヘッドホンへの出力をまとめて調整します。">
      {missing && (
        <Notice action={<Button onClick={() => openUrl("https://vb-audio.com/Cable/")}>VB-CABLE を入手</Button>}>
          仮想デバイスが足りません（{[!cables.mic && "VB-CABLE", !cables.game && "VB-CABLE A", !cables.chat && "VB-CABLE B"].filter(Boolean).join("、")}）。
          インストール後、Windows を再起動するとここに表示されなくなります。
        </Notice>
      )}

      <div className="rounded-[18px] border border-border bg-surface px-8 pb-6 pt-7">
        <div className="grid grid-cols-4 gap-6">
          <Strip
            icon={<Gamepad2 className="h-4 w-4" />}
            name="Game"
            sub={presetName(settings.game.preset)}
            level={m?.game}
            volume={settings.game.volumeDb}
            muted={settings.game.muted}
            onVolume={(v) => update((s) => void (s.game.volumeDb = v))}
            onMute={() => update((s) => void (s.game.muted = !s.game.muted))}
            onSub={() => onNavigate("eq")}
          />
          <Strip
            icon={<MessagesSquare className="h-4 w-4" />}
            name="Chat"
            sub={settings.chat.denoise ? "ノイズ除去オン" : presetName(settings.chat.preset)}
            level={m?.chat}
            volume={settings.chat.volumeDb}
            muted={settings.chat.muted}
            onVolume={(v) => update((s) => void (s.chat.volumeDb = v))}
            onMute={() => update((s) => void (s.chat.muted = !s.chat.muted))}
            onSub={() => onNavigate("eq")}
          />
          <Strip
            icon={<Mic className="h-4 w-4" />}
            name="マイク"
            sub={settings.mic.denoise === "off" ? "ノイズ除去オフ" : settings.mic.denoise === "high" ? "高品質ノイズ除去" : "軽量ノイズ除去"}
            level={m?.micOut}
            volume={settings.mic.gainDb}
            muted={settings.mic.muted}
            min={-24}
            max={24}
            onVolume={(v) => update((s) => void (s.mic.gainDb = v))}
            onMute={() => update((s) => void (s.mic.muted = !s.mic.muted))}
            onSub={() => onNavigate("mic")}
          />
          <Strip
            icon={<Headphones className="h-4 w-4" />}
            name="ヘッドホン"
            sub={settings.output.headphoneEqName ?? "補正なし"}
            level={m?.master}
            volume={settings.output.masterDb}
            muted={false}
            hideMute
            onVolume={(v) => update((s) => void (s.output.masterDb = v))}
            onMute={() => {}}
            onSub={() => onNavigate("eq")}
            emphasis
          />
        </div>

        <div className="mt-8 border-t border-border-subtle pt-6">
          <div className="mb-3 flex items-baseline justify-between">
            <span className="text-[13px] font-medium text-text-primary">ChatMix</span>
            <span className="tabular text-[12px] text-text-muted">
              ゲーム {Math.round(Math.min(1, 1 - settings.output.chatmix) * 100)}% ・ チャット {Math.round(Math.min(1, 1 + settings.output.chatmix) * 100)}%
            </span>
          </div>
          <div className="flex items-center gap-4">
            <span className="w-14 whitespace-nowrap text-[12px] text-text-secondary">ゲーム</span>
            <input
              type="range"
              className="slider flex-1"
              min={-1}
              max={1}
              step={0.05}
              value={settings.output.chatmix}
              style={{ ["--fill" as string]: `${chatmixPct}%` }}
              onChange={(e) => update((s) => void (s.output.chatmix = Number(e.target.value)))}
              onDoubleClick={() => update((s) => void (s.output.chatmix = 0))}
            />
            <span className="w-14 whitespace-nowrap text-right text-[12px] text-text-secondary">チャット</span>
          </div>
        </div>
      </div>

      <Section title="ゲームの聞こえ方">
        <Row label="バーチャルサラウンド" description="7.1ch のゲーム音を、ヘッドホンでも前後左右の方向が分かるように変換します。">
          <Toggle checked={settings.game.surround.enabled} onChange={(v) => update((s) => void (s.game.surround.enabled = v))} />
        </Row>
        <Row label="部屋の響き" description="多いほど音が頭の外から聞こえます。" disabled={!settings.game.surround.enabled}>
          <Slider
            value={Math.round(settings.game.surround.room * 100)}
            min={0}
            max={100}
            format={(v) => `${v}%`}
            disabled={!settings.game.surround.enabled}
            onChange={(v) => update((s) => void (s.game.surround.room = v / 100))}
          />
        </Row>
        <Row label="小さい音を持ち上げる" description="遠くの足音など、小さな音を聞き取りやすくします。">
          <Toggle checked={settings.game.leveler.enabled} onChange={(v) => update((s) => void (s.game.leveler.enabled = v))} />
        </Row>
      </Section>

      <Section title="配信用ミックス" description="自分が聞く音とは別に、OBS などへ送る音を作ります。">
        <Row label="配信用ミックスを使う">
          <Toggle checked={settings.stream.enabled} onChange={(v) => update((s) => void (s.stream.enabled = v))} />
        </Row>
        {settings.stream.enabled && (
          <>
            <Row label="出力先" description="OBS で取り込む仮想デバイスを選びます（例：VB-CABLE C）。">
              <DeviceSelect devices={devices.outputs} value={settings.stream.device} autoLabel="選んでください" onChange={(v) => update((s) => void (s.stream.device = v))} />
            </Row>
            <Row label="ゲーム">
              <Slider value={settings.stream.gameDb} min={-40} max={12} step={0.5} format={fmtDb} onChange={(v) => update((s) => void (s.stream.gameDb = v))} />
            </Row>
            <Row label="チャット">
              <Slider value={settings.stream.chatDb} min={-40} max={12} step={0.5} format={fmtDb} onChange={(v) => update((s) => void (s.stream.chatDb = v))} />
            </Row>
            <Row label="マイクも入れる" description="OBS で別にマイクを取り込んでいる場合はオフにします。">
              <Toggle checked={settings.stream.includeMic} onChange={(v) => update((s) => void (s.stream.includeMic = v))} />
              <Slider value={settings.stream.micDb} min={-40} max={12} step={0.5} format={fmtDb} width={180} disabled={!settings.stream.includeMic} onChange={(v) => update((s) => void (s.stream.micDb = v))} />
            </Row>
          </>
        )}
      </Section>
    </Page>
  );
}

function Strip({ icon, name, sub, level, volume, muted, onVolume, onMute, onSub, min = -60, max = 12, hideMute, emphasis }: {
  icon: React.ReactNode;
  name: string;
  sub: string;
  level?: Level | null;
  volume: number;
  muted: boolean;
  onVolume: (v: number) => void;
  onMute: () => void;
  onSub: () => void;
  min?: number;
  max?: number;
  hideMute?: boolean;
  emphasis?: boolean;
}) {
  return (
    <div className={cn("flex flex-col items-center", emphasis && "border-l border-border-subtle")}>
      <div className="flex items-center gap-2 text-[13px] font-medium text-text-primary">
        <span className="text-text-secondary">{icon}</span>
        {name}
      </div>
      <button onClick={onSub} className="focus-ring mt-1 max-w-[150px] truncate rounded-md px-1.5 text-[11px] text-text-muted hover:text-text-secondary">
        {sub}
      </button>
      <div className="mt-5 flex items-end gap-4">
        <VMeter level={muted ? null : level} height={200} />
        <VFader value={volume} onChange={onVolume} height={200} min={min} max={max} />
      </div>
      <div className="tabular mt-3 text-[12px] text-text-secondary">{fmtDb(volume)}</div>
      {!hideMute ? (
        <button
          onClick={onMute}
          aria-pressed={muted}
          className={cn(
            "focus-ring mt-3 flex h-8 w-8 items-center justify-center rounded-full border transition-colors",
            muted ? "border-[#4a2424] bg-[#2a1515] text-[#fca5a5]" : "border-border bg-surface-raised text-text-secondary hover:text-text-primary",
          )}
          aria-label={muted ? "ミュートを解除" : "ミュート"}
        >
          {muted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
        </button>
      ) : (
        <div className="mt-3 h-8" />
      )}
    </div>
  );
}
