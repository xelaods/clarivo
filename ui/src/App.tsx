import { useEffect, useMemo, useState } from "react";
import { AudioLines, Mic, MicOff, SlidersHorizontal, Route, UserRound, Settings2, Minus, Square, X, Headphones, Sparkles, Gamepad2, MessagesSquare, Search } from "lucide-react";
import { api, inTauri, windowAction } from "./lib/api";
import { StoreProvider, useStore } from "./lib/store";
import type { Bootstrap } from "./lib/types";
import { Sidebar } from "./components/easyui/sidebar";
import { VelocityToast } from "./components/easyui/VelocityToast";
import { CommandMenu, type CommandItem } from "./components/easyui/CommandMenu";
import { SetupGuide } from "./components/SetupGuide";
import { MixerPage } from "./pages/Mixer";
import { MicPage } from "./pages/MicPage";
import { EqualizerPage } from "./pages/Equalizer";
import { RoutingPage } from "./pages/Routing";
import { SpeakerPage } from "./pages/Speaker";
import { SettingsPage } from "./pages/SettingsPage";

type PageId = "mixer" | "mic" | "eq" | "routing" | "speaker" | "settings";

export function App() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.bootstrap().then(setBoot).catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return <div className="flex h-full items-center justify-center p-10 text-[13px] text-text-secondary">起動に失敗しました：{error}</div>;
  }
  if (!boot) {
    return <div className="h-full bg-background" />;
  }
  return (
    <StoreProvider boot={boot}>
      <Shell />
    </StoreProvider>
  );
}

const PAGES: { id: PageId; label: string }[] = [
  { id: "mixer", label: "ミキサー" },
  { id: "mic", label: "マイク" },
  { id: "eq", label: "イコライザー" },
  { id: "routing", label: "振り分け" },
  { id: "speaker", label: "声の登録" },
  { id: "settings", label: "設定" },
];

function Shell() {
  const [page, setPage] = useState<PageId>("mixer");
  const { status, toasts, dismissToast, settings } = useStore();
  const [commandOpen, setCommandOpen] = useState(false);
  const [setupOpen, setSetupOpen] = useState(!settings.app.onboarded);

  // Ctrl+K でコマンドメニュー
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCommandOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const commands = useCommands((p) => setPage(p));

  const running = status?.status.mic.running || status?.status.output.running;
  const hasError = status?.status.mic.error || status?.status.output.error;

  return (
    <div className="flex h-full flex-col bg-background">
      <TitleBar running={!!running} hasError={!!hasError} profile={status?.activeProfile ?? null} onSearch={() => setCommandOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <Sidebar
          activeId={page}
          onActiveChange={(id) => setPage(id as PageId)}
          className="border-border bg-background dark:border-border dark:bg-background"
          sections={[
            {
              items: [
                { id: "mixer", label: "ミキサー", icon: <AudioLines className="h-4 w-4" /> },
                { id: "mic", label: "マイク", icon: <Mic className="h-4 w-4" /> },
                { id: "eq", label: "イコライザー", icon: <SlidersHorizontal className="h-4 w-4" /> },
                { id: "routing", label: "振り分け", icon: <Route className="h-4 w-4" /> },
                { id: "speaker", label: "声の登録", icon: <UserRound className="h-4 w-4" /> },
              ],
            },
            { items: [{ id: "settings", label: "設定", icon: <Settings2 className="h-4 w-4" /> }] },
          ]}
        />
        <main className="min-w-0 flex-1">
          {page === "mixer" && <MixerPage onNavigate={(p) => setPage(p as PageId)} />}
          {page === "mic" && <MicPage />}
          {page === "eq" && <EqualizerPage />}
          {page === "routing" && <RoutingPage />}
          {page === "speaker" && <SpeakerPage />}
          {page === "settings" && <SettingsPage onOpenSetup={() => setSetupOpen(true)} />}
        </main>
      </div>
      <CommandMenu isOpen={commandOpen} onClose={() => setCommandOpen(false)} items={commands} />
      <SetupGuide open={setupOpen} onClose={() => setSetupOpen(false)} />
      {toasts.map((t, i) => (
        <VelocityToast
          key={t.id}
          open
          title={t.title}
          description={t.description}
          variant={t.variant}
          position="bottom-right"
          duration={5000}
          onDismiss={() => dismissToast(t.id)}
          className={i > 0 ? "hidden" : undefined}
        />
      ))}
    </div>
  );
}

/** コマンドメニューの項目：ページ移動、プリセット、よく使う切り替え。 */
function useCommands(navigate: (p: PageId) => void): CommandItem[] {
  const { settings, update, presets } = useStore();
  return useMemo(() => {
    const items: CommandItem[] = [];
    const on = (v: boolean) => (v ? "オン" : "オフ");
    items.push(
      {
        id: "mute",
        category: "操作",
        title: settings.mic.muted ? "マイクのミュートを解除" : "マイクをミュート",
        icon: settings.mic.muted ? <Mic className="h-4 w-4" /> : <MicOff className="h-4 w-4" />,
        hint: settings.hotkeys.toggleMicMute ?? undefined,
        keywords: "mute ミュート",
        onSelect: () => update((s) => void (s.mic.muted = !s.mic.muted)),
      },
      {
        id: "surround",
        category: "操作",
        title: settings.game.surround.enabled ? "サラウンドをオフ" : "サラウンドをオン",
        icon: <Headphones className="h-4 w-4" />,
        hint: `今：${on(settings.game.surround.enabled)}`,
        keywords: "surround 7.1",
        onSelect: () => update((s) => void (s.game.surround.enabled = !s.game.surround.enabled)),
      },
      ...(["off", "light", "high"] as const).map((m) => ({
        id: `denoise-${m}`,
        category: "操作",
        title: `ノイズ除去：${m === "off" ? "オフ" : m === "light" ? "軽量（RNNoise）" : "高品質（DeepFilterNet3）"}`,
        icon: <Sparkles className="h-4 w-4" />,
        hint: settings.mic.denoise === m ? "使用中" : undefined,
        keywords: "noise ノイズ ノイキャン",
        onSelect: () => update((s) => void (s.mic.denoise = m)),
      })),
      {
        id: "chatmix-center",
        category: "操作",
        title: "ChatMix を中央に戻す",
        icon: <AudioLines className="h-4 w-4" />,
        keywords: "chatmix バランス",
        onSelect: () => update((s) => void (s.output.chatmix = 0)),
      },
    );
    for (const p of presets) {
      items.push({
        id: `game-${p.id}`,
        category: "Game のプリセット",
        title: p.name,
        icon: <Gamepad2 className="h-4 w-4" />,
        hint: settings.game.preset === p.id ? "使用中" : undefined,
        keywords: `game eq ${p.description}`,
        onSelect: () =>
          update((s) => {
            s.game.preset = p.id;
            s.game.eq = structuredClone(p.eq);
            s.game.leveler = structuredClone(p.leveler);
          }),
      });
    }
    for (const p of presets) {
      items.push({
        id: `chat-${p.id}`,
        category: "Chat のプリセット",
        title: p.name,
        icon: <MessagesSquare className="h-4 w-4" />,
        hint: settings.chat.preset === p.id ? "使用中" : undefined,
        keywords: `chat eq ${p.description}`,
        onSelect: () =>
          update((s) => {
            s.chat.preset = p.id;
            s.chat.eq = structuredClone(p.eq);
            s.chat.leveler = structuredClone(p.leveler);
          }),
      });
    }
    for (const pg of PAGES) {
      items.push({ id: `page-${pg.id}`, category: "ページ", title: `${pg.label}を開く`, onSelect: () => navigate(pg.id) });
    }
    return items;
  }, [settings, presets, update, navigate]);
}

function TitleBar({ running, hasError, profile, onSearch }: { running: boolean; hasError: boolean; profile: string | null; onSearch: () => void }) {
  const { settings } = useStore();
  const profileName = profile ? settings.profiles.items.find((p) => p.id === profile)?.name : null;
  return (
    <div className="drag flex h-10 shrink-0 items-center justify-between border-b border-border pl-4" data-tauri-drag-region>
      <div className="flex items-center gap-3 text-[13px]" data-tauri-drag-region>
        <span className="font-semibold tracking-[-0.02em] text-text-primary">Clarivo</span>
        <span className="flex items-center gap-1.5 text-[12px] text-text-muted" data-tauri-drag-region>
          <span className={`h-1.5 w-1.5 rounded-full ${hasError ? "bg-[var(--meter-warn)]" : running ? "bg-accent-blue" : "bg-text-subtle"}`} />
          {hasError ? "要確認" : running ? "動作中" : "停止中"}
        </span>
        {profileName && <span className="text-[12px] text-text-secondary">プロファイル：{profileName}</span>}
      </div>
      <div className="no-drag flex h-full items-center">
        <button
          onClick={onSearch}
          className="focus-ring mr-2 flex h-7 items-center gap-2 rounded-[8px] border border-border bg-surface px-2.5 text-[12px] text-text-muted transition-colors hover:border-border-hover hover:text-text-secondary"
        >
          <Search className="h-3.5 w-3.5" />
          操作を検索
          <kbd className="rounded border border-border px-1 font-sans text-[10px]">Ctrl+K</kbd>
        </button>
      {inTauri && (
        <div className="flex h-full">
          <WinButton label="最小化" onClick={() => windowAction("minimize")}><Minus className="h-3.5 w-3.5" /></WinButton>
          <WinButton label="最大化" onClick={() => windowAction("maximize")}><Square className="h-3 w-3" /></WinButton>
          <WinButton label="閉じる" danger onClick={() => windowAction("close")}><X className="h-4 w-4" /></WinButton>
        </div>
      )}
      </div>
    </div>
  );
}

function WinButton({ children, onClick, label, danger }: { children: React.ReactNode; onClick: () => void; label: string; danger?: boolean }) {
  return (
    <button
      aria-label={label}
      onClick={onClick}
      className={`flex h-full w-12 items-center justify-center text-text-secondary transition-colors ${danger ? "hover:bg-[#c42b1c] hover:text-white" : "hover:bg-surface-hover hover:text-text-primary"}`}
    >
      {children}
    </button>
  );
}
