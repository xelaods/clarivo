import { useEffect, useState } from "react";
import { AudioLines, Mic, SlidersHorizontal, Route, UserRound, Settings2, Minus, Square, X } from "lucide-react";
import { api, inTauri, windowAction } from "./lib/api";
import { StoreProvider, useStore } from "./lib/store";
import type { Bootstrap } from "./lib/types";
import { Sidebar } from "./components/easyui/sidebar";
import { VelocityToast } from "./components/easyui/VelocityToast";
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

function Shell() {
  const [page, setPage] = useState<PageId>("mixer");
  const { status, toasts, dismissToast } = useStore();

  const running = status?.status.mic.running || status?.status.output.running;
  const hasError = status?.status.mic.error || status?.status.output.error;

  return (
    <div className="flex h-full flex-col bg-background">
      <TitleBar running={!!running} hasError={!!hasError} profile={status?.activeProfile ?? null} />
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
          {page === "settings" && <SettingsPage />}
        </main>
      </div>
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

function TitleBar({ running, hasError, profile }: { running: boolean; hasError: boolean; profile: string | null }) {
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
      {inTauri && (
        <div className="no-drag flex h-full">
          <WinButton label="最小化" onClick={() => windowAction("minimize")}><Minus className="h-3.5 w-3.5" /></WinButton>
          <WinButton label="最大化" onClick={() => windowAction("maximize")}><Square className="h-3 w-3" /></WinButton>
          <WinButton label="閉じる" danger onClick={() => windowAction("close")}><X className="h-4 w-4" /></WinButton>
        </div>
      )}
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
