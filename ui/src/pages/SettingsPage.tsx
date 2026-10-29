import { useState } from "react";
import { Plus, Trash2, RotateCcw } from "lucide-react";
import { api, openUrl } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { HotkeySettings } from "@/lib/types";
import { DragToConfirm } from "@/components/easyui/DragToConfirm";
import { Button, DeviceSelect, Page, Row, Section, Toggle } from "@/components/ui";

export function SettingsPage() {
  const { settings, update, devices, refreshDevices, boot, toast } = useStore();

  return (
    <Page
      title="設定"
      actions={
        <Button onClick={() => refreshDevices().then(() => toast({ title: "デバイスの一覧を更新しました", variant: "success" }))}>
          <RotateCcw className="h-3.5 w-3.5" /> デバイスを再読み込み
        </Button>
      }
    >
      <Section title="出力デバイス">
        <Row label="ヘッドホン" description="実際に音を聞くデバイス。仮想デバイスは選ばないでください。">
          <DeviceSelect devices={devices.outputs.filter((d) => !/CABLE|VB-Audio/i.test(d.name))} value={settings.output.headphones} autoLabel="既定のデバイス" onChange={(v) => update((s) => void (s.output.headphones = v))} />
        </Row>
        <Row label="Game の取り込み元" description="VB-CABLE A の「CABLE-A Output」">
          <DeviceSelect devices={devices.inputs} value={settings.output.gameSource} autoLabel="CABLE-A Output（自動）" onChange={(v) => update((s) => void (s.output.gameSource = v))} />
        </Row>
        <Row label="Chat の取り込み元" description="VB-CABLE B の「CABLE-B Output」">
          <DeviceSelect devices={devices.inputs} value={settings.output.chatSource} autoLabel="CABLE-B Output（自動）" onChange={(v) => update((s) => void (s.output.chatSource = v))} />
        </Row>
      </Section>

      <Section title="アプリ">
        <Row label="Windows の起動時に開始">
          <Toggle checked={settings.app.autostart} onChange={(v) => update((s) => void (s.app.autostart = v))} />
        </Row>
        <Row label="起動時はタスクトレイに入れる">
          <Toggle checked={settings.app.startMinimized} onChange={(v) => update((s) => void (s.app.startMinimized = v))} />
        </Row>
        <Row label="閉じるボタンでタスクトレイに入れる" description="オフにすると、閉じたときに Clarivo を終了します。">
          <Toggle checked={settings.app.closeToTray} onChange={(v) => update((s) => void (s.app.closeToTray = v))} />
        </Row>
        <Row label="オーバーレイ" description="ホットキーで切り替えたとき、画面の上に一瞬表示します。">
          <Toggle checked={settings.app.overlay} onChange={(v) => update((s) => void (s.app.overlay = v))} />
        </Row>
      </Section>

      <Section title="ホットキー" description="入力欄を選んでキーを押すと設定できます。Backspace で解除します。">
        <HotkeyRow label="マイクのミュート" field="toggleMicMute" />
        <HotkeyRow label="Game のプリセットを切り替え" field="nextPreset" />
        <HotkeyRow label="ChatMix をゲーム側へ" field="chatmixGame" />
        <HotkeyRow label="ChatMix をチャット側へ" field="chatmixChat" />
        <HotkeyRow label="サラウンドの切り替え" field="toggleSurround" />
      </Section>

      <Profiles />

      <Section title="困ったとき">
        <Row label="音声エンジンを再起動" description="音が出なくなった、デバイスを付け替えた、などのときに試してください。">
          <Button onClick={() => api.restartEngine().then(() => toast({ title: "再起動しました", variant: "success" }))}>再起動</Button>
        </Row>
        <Row label="音の出力先を元に戻す" description="Clarivo が変えたアプリ別の出力先と既定デバイスを、すべて元に戻します（振り分けもオフになります）。">
          <div className="w-[240px]">
            <DragToConfirm
              label="スライドして戻す"
              confirmedLabel="戻しました"
              size="sm"
              autoResetDelay={2000}
              onConfirm={() => api.resetAudio().catch((e) => toast({ title: "戻せませんでした", description: String(e), variant: "error" }))}
            />
          </div>
        </Row>
      </Section>

      <Section title="Clarivo について" description={`バージョン ${boot.version}`}>
        <Row label="VB-CABLE" description="仮想マイク・Game・Chat に使う仮想デバイス（VB-Audio 製、寄付制）。">
          <Button onClick={() => openUrl("https://vb-audio.com/Cable/")}>入手する</Button>
        </Row>
        <div className="px-5 py-4">
          <div className="mb-2 text-[13px] font-medium text-text-primary">使用しているオープンソース</div>
          <ul className="grid grid-cols-2 gap-x-6 gap-y-1.5">
            {boot.credits.map((c) => (
              <li key={c.name}>
                <button className="focus-ring text-left text-[12px] text-text-secondary hover:text-text-primary" onClick={() => openUrl(c.url)}>
                  {c.name} <span className="text-text-muted">（{c.license}）</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      </Section>
    </Page>
  );
}

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  " ": "Space",
};

function toAccelerator(e: React.KeyboardEvent): string | null {
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  if (mods.length === 0) return null;
  let key = KEY_NAMES[e.key] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  if (/^F\d{1,2}$/.test(e.key)) key = e.key;
  return [...mods, key].join("+");
}

function HotkeyRow({ label, field }: { label: string; field: keyof HotkeySettings }) {
  const { settings, update } = useStore();
  const [listening, setListening] = useState(false);
  const value = settings.hotkeys[field];
  return (
    <Row label={label}>
      <input
        readOnly
        className="text-input tabular w-[200px] cursor-pointer text-center"
        value={listening ? "キーを押してください" : value ?? "なし"}
        onFocus={() => setListening(true)}
        onBlur={() => setListening(false)}
        onKeyDown={(e) => {
          e.preventDefault();
          if (e.key === "Backspace" || e.key === "Delete") {
            update((s) => void (s.hotkeys[field] = null));
            (e.target as HTMLInputElement).blur();
            return;
          }
          const acc = toAccelerator(e);
          if (acc) {
            update((s) => void (s.hotkeys[field] = acc));
            (e.target as HTMLInputElement).blur();
          }
        }}
      />
    </Row>
  );
}

function Profiles() {
  const { settings, update, presets } = useStore();
  const [exe, setExe] = useState("");
  const [name, setName] = useState("");
  const p = settings.profiles;

  const add = () => {
    const e = exe.trim().toLowerCase();
    if (!e) return;
    const full = e.endsWith(".exe") ? e : `${e}.exe`;
    update((s) => {
      s.profiles.items.push({
        id: `g-${Date.now().toString(36)}`,
        name: name.trim() || full.replace(/\.exe$/, ""),
        processes: [full],
        game: structuredClone(s.game),
        chatmix: s.output.chatmix,
      });
    });
    setExe("");
    setName("");
  };

  return (
    <Section title="ゲームごとのプロファイル" description="指定したゲームが前面に来たら、Game のEQ・サラウンド・ChatMix を切り替えます。ゲームを終了すると元に戻ります。">
      <Row label="自動で切り替える">
        <Toggle checked={p.autoSwitch} onChange={(v) => update((s) => void (s.profiles.autoSwitch = v))} />
      </Row>
      {p.items.map((item, i) => (
        <div key={item.id} className="flex min-h-[56px] items-center justify-between gap-6 border-b border-border-subtle px-5 py-3">
          <div>
            <div className="text-[13px] font-medium text-text-primary">{item.name}</div>
            <div className="mt-0.5 text-[12px] text-text-muted">
              {item.processes.join("、")} ・ {presets.find((x) => x.id === item.game.preset)?.name ?? "カスタム"}
              {item.game.surround.enabled ? " ・ サラウンド" : ""}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              onClick={() =>
                update((s) => {
                  s.profiles.items[i].game = structuredClone(s.game);
                  s.profiles.items[i].chatmix = s.output.chatmix;
                })
              }
            >
              今の設定で上書き
            </Button>
            <Button variant="ghost" onClick={() => update((s) => void s.profiles.items.splice(i, 1))}>
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      ))}
      <div className="flex items-end gap-3 px-5 py-4">
        <label className="flex flex-col gap-1.5 text-[12px] text-text-muted">
          表示名
          <input className="text-input w-[180px]" placeholder="VALORANT" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="flex flex-1 flex-col gap-1.5 text-[12px] text-text-muted">
          実行ファイル名（振り分けのページで確認できます）
          <input className="text-input w-full" placeholder="valorant-win64-shipping.exe" value={exe} onChange={(e) => setExe(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
        </label>
        <Button onClick={add} disabled={!exe.trim()}>
          <Plus className="h-3.5 w-3.5" /> 今の Game 設定で追加
        </Button>
      </div>
    </Section>
  );
}
