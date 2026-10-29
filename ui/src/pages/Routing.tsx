import { useEffect, useState } from "react";
import { api, listen } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { ChannelId, RoutingReport } from "@/lib/types";
import { DeviceSelect, Notice, Page, Pill, Row, Section, Segmented, Toggle } from "@/components/ui";

export function RoutingPage() {
  const { settings, update, devices, replaceSettings, toast } = useStore();
  const [report, setReport] = useState<RoutingReport | null>(null);
  const r = settings.routing;

  useEffect(() => {
    api.routingReport().then(setReport).catch(() => {});
    const off = listen<RoutingReport>("routing", setReport);
    return () => {
      off.then((f) => f());
    };
  }, []);

  const setChannel = async (exe: string, channel: ChannelId) => {
    try {
      replaceSettings(await api.setAppChannel(exe, channel));
    } catch (e) {
      toast({ title: "変更できません", description: String(e), variant: "error" });
    }
  };

  const sessions = report?.sessions ?? [];

  return (
    <Page title="振り分け" description="通話アプリの音を Chat に、それ以外を Game に自動で振り分けます。ChatMix やチャンネルごとのEQはこの振り分けを使います。">
      {report && !report.available && <Notice>{report.error ?? "この Windows ではアプリ別の振り分けが使えません。Windows の設定（サウンド → 音量ミキサー）で手動で出力先を選んでください。"}</Notice>}

      <Section title="自動の振り分け">
        <Row label="アプリの音を振り分ける" description="オフにすると、Clarivo が変えた出力先をすべて元に戻します。">
          <Toggle checked={r.enabled} onChange={(v) => update((s) => void (s.routing.enabled = v))} />
        </Row>
        <Row label="Windows の既定デバイスも切り替える" description="既定の再生デバイスを Game に、通信デバイスを Chat にします。終了時に元へ戻します。" disabled={!r.enabled}>
          <Toggle checked={r.setDefaultDevices} disabled={!r.enabled} onChange={(v) => update((s) => void (s.routing.setDefaultDevices = v))} />
        </Row>
        <Row label="Game の出力先" description="VB-CABLE A の「CABLE-A Input」">
          <DeviceSelect devices={devices.outputs} value={r.gameSink} autoLabel="CABLE-A Input（自動）" onChange={(v) => update((s) => void (s.routing.gameSink = v))} />
        </Row>
        <Row label="Chat の出力先" description="VB-CABLE B の「CABLE-B Input」">
          <DeviceSelect devices={devices.outputs} value={r.chatSink} autoLabel="CABLE-B Input（自動）" onChange={(v) => update((s) => void (s.routing.chatSink = v))} />
        </Row>
      </Section>

      <Section title="音を出しているアプリ" description="切り替えた後、アプリ側で再生し直すと反映されます。">
        {sessions.length === 0 ? (
          <div className="px-5 py-8 text-center text-[13px] text-text-muted">音を出しているアプリはありません。ゲームや Discord を起動すると、ここに表示されます。</div>
        ) : (
          sessions.map((s) => (
            <div key={`${s.pid}-${s.device}`} className="flex min-h-[60px] items-center justify-between gap-6 border-b border-border-subtle px-5 py-3 last:border-b-0">
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-[13px] font-medium text-text-primary">
                  <span className={`h-1.5 w-1.5 rounded-full ${s.active ? "bg-accent-blue" : "bg-text-subtle"}`} />
                  {s.name}
                </div>
                <div className="mt-0.5 truncate text-[12px] text-text-muted">
                  {s.exe} ・ 今の出力先：{s.device}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-3">
                {r.enabled && !s.routed && <Pill tone="warn">再生し直すと反映</Pill>}
                <Segmented<ChannelId>
                  value={s.channel}
                  onChange={(v) => setChannel(s.exe, v)}
                  options={[
                    { id: "game", label: "Game" },
                    { id: "chat", label: "Chat" },
                  ]}
                />
              </div>
            </div>
          ))
        )}
      </Section>

      <Section title="Chat に入れるアプリ" description="ここにあるアプリが Chat に、それ以外は Game に入ります。">
        <div className="flex flex-wrap gap-2 px-5 py-4">
          {r.rules
            .filter((x) => x.channel === "chat")
            .map((x) => (
              <span key={x.process} className="inline-flex h-7 items-center gap-1.5 rounded-full border border-border bg-surface-raised pl-3 pr-1 text-[12px] text-text-secondary">
                {x.process}
                <button
                  className="focus-ring flex h-5 w-5 items-center justify-center rounded-full text-text-muted hover:bg-surface-hover hover:text-text-primary"
                  aria-label={`${x.process} を Chat から外す`}
                  onClick={() => update((s) => void (s.routing.rules = s.routing.rules.filter((y) => y.process !== x.process)))}
                >
                  ×
                </button>
              </span>
            ))}
          <AddRule onAdd={(exe) => update((s) => {
            const name = exe.trim().toLowerCase();
            if (!name) return;
            const full = name.endsWith(".exe") ? name : `${name}.exe`;
            s.routing.rules = s.routing.rules.filter((y) => y.process !== full);
            s.routing.rules.push({ process: full, channel: "chat" });
          })} />
        </div>
      </Section>
    </Page>
  );
}

function AddRule({ onAdd }: { onAdd: (exe: string) => void }) {
  const [v, setV] = useState("");
  return (
    <input
      className="text-input h-7 w-[200px] text-[12px]"
      placeholder="実行ファイル名を追加（例：obs64.exe）"
      value={v}
      onChange={(e) => setV(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          onAdd(v);
          setV("");
        }
      }}
    />
  );
}
