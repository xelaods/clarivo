import { useEffect, useState } from "react";
import { Search, FileUp, Save, X } from "lucide-react";
import { api, pickFile } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { AutoEqEntry, ChannelSettings, EqSettings } from "@/lib/types";
import { EqEditor } from "@/components/EqEditor";
import { Combobox } from "@/components/easyui/combobox";
import { SettleModal } from "@/components/easyui/SettleModal";
import { Button, fmtDb, Page, Row, Section, Segmented, Slider, Toggle } from "@/components/ui";

type Tab = "game" | "chat" | "headphones";

export function EqualizerPage() {
  const [tab, setTab] = useState<Tab>("game");
  return (
    <Page
      title="イコライザー"
      description="Game と Chat はそれぞれ別のEQをかけられます。ヘッドホン補正は最後に全体へかかります。"
      actions={
        <Segmented<Tab>
          value={tab}
          onChange={setTab}
          options={[
            { id: "game", label: "Game" },
            { id: "chat", label: "Chat" },
            { id: "headphones", label: "ヘッドホン補正" },
          ]}
        />
      }
    >
      {tab === "headphones" ? <HeadphoneEq /> : <ChannelEq channel={tab} key={tab} />}
    </Page>
  );
}

function ChannelEq({ channel }: { channel: "game" | "chat" }) {
  const { settings, update, presets, toast } = useStore();
  const ch: ChannelSettings = settings[channel];
  const [saveOpen, setSaveOpen] = useState(false);
  const [name, setName] = useState("");

  const setEq = (eq: EqSettings) =>
    update((s) => {
      s[channel].eq = eq;
      s[channel].preset = "custom";
    });

  const applyPreset = (id: string | null) => {
    const p = presets.find((x) => x.id === id);
    if (!p) return;
    update((s) => {
      s[channel].preset = p.id;
      s[channel].eq = structuredClone(p.eq);
      s[channel].leveler = structuredClone(p.leveler);
    });
  };

  const savePreset = () => {
    const n = name.trim();
    if (!n) return;
    const id = `user-${Date.now().toString(36)}`;
    update((s) => {
      s.customPresets.push({ id, name: n, description: "自分で作ったプリセット", builtin: false, eq: structuredClone(ch.eq), leveler: structuredClone(ch.leveler) });
      s[channel].preset = id;
    });
    setSaveOpen(false);
    setName("");
    toast({ title: `「${n}」を保存しました`, variant: "success" });
  };

  const current = presets.find((p) => p.id === ch.preset);

  return (
    <>
      <Section
        title={channel === "game" ? "Game のEQ" : "Chat のEQ"}
        aside={
          <div className="flex items-center gap-2">
            <div className="w-[240px]">
              <Combobox
                className="w-full"
                options={presets.map((p) => ({ value: p.id, label: p.name, description: p.description }))}
                value={current ? current.id : null}
                placeholder="カスタム"
                searchPlaceholder="プリセットを検索"
                emptyText="見つかりません"
                onValueChange={applyPreset}
                align="end"
              />
            </div>
            <Button onClick={() => setSaveOpen(true)}>
              <Save className="h-3.5 w-3.5" /> 保存
            </Button>
          </div>
        }
      >
        <div className="flex items-center justify-between border-b border-border-subtle px-5 py-3">
          <span className="text-[13px] text-text-secondary">{current ? current.description : "手動で調整中"}</span>
          <Toggle checked={ch.eq.enabled} onChange={(v) => update((s) => void (s[channel].eq.enabled = v))} />
        </div>
        <div className="px-5 py-4">
          <EqEditor eq={ch.eq} onChange={setEq} />
        </div>
      </Section>

      <Section title="小さい音を持ち上げる" description="しきい値より小さい音だけを持ち上げます。無音（ノイズ）は持ち上げません。">
        <Row label="使う">
          <Toggle checked={ch.leveler.enabled} onChange={(v) => update((s) => void (s[channel].leveler.enabled = v))} />
        </Row>
        <Row label="しきい値" disabled={!ch.leveler.enabled}>
          <Slider value={ch.leveler.thresholdDb} min={-50} max={-10} step={1} format={(v) => `${v} dB`} disabled={!ch.leveler.enabled} onChange={(v) => update((s) => void (s[channel].leveler.thresholdDb = v))} />
        </Row>
        <Row label="最大の持ち上げ量" disabled={!ch.leveler.enabled}>
          <Slider value={ch.leveler.maxBoostDb} min={0} max={20} step={0.5} format={fmtDb} disabled={!ch.leveler.enabled} onChange={(v) => update((s) => void (s[channel].leveler.maxBoostDb = v))} />
        </Row>
      </Section>

      {channel === "chat" && (
        <Section title="受信した声のノイズ除去" description="通話相手のマイクの雑音を、こちら側で消します。">
          <Row label="使う">
            <Toggle checked={ch.denoise} onChange={(v) => update((s) => void (s.chat.denoise = v))} />
          </Row>
          <Row label="消す強さ" disabled={!ch.denoise}>
            <Slider value={ch.denoiseStrengthDb} min={0} max={100} step={1} format={(v) => `${v} dB`} disabled={!ch.denoise} onChange={(v) => update((s) => void (s.chat.denoiseStrengthDb = v))} />
          </Row>
        </Section>
      )}

      <SettleModal
        open={saveOpen}
        onClose={() => setSaveOpen(false)}
        title="プリセットとして保存"
        description="今のEQと「小さい音を持ち上げる」の設定を保存します。"
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setSaveOpen(false)}>やめる</Button>
            <Button variant="primary" onClick={savePreset} disabled={!name.trim()}>保存する</Button>
          </div>
        }
      >
        <input className="text-input w-full" autoFocus placeholder="例：VALORANT 用" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && savePreset()} />
      </SettleModal>
    </>
  );
}

function HeadphoneEq() {
  const { settings, update, toast } = useStore();
  const eq = settings.output.headphoneEq;
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AutoEqEntry[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setResults([]);
      return;
    }
    const id = window.setTimeout(async () => {
      try {
        setBusy(true);
        setResults(await api.autoeqSearch(q));
      } catch (e) {
        toast({ title: "検索できません", description: String(e), variant: "error" });
      } finally {
        setBusy(false);
      }
    }, 300);
    return () => window.clearTimeout(id);
  }, [query, toast]);

  const apply = async (entry: AutoEqEntry) => {
    try {
      setBusy(true);
      const loaded = await api.autoeqLoad(entry.path);
      update((s) => {
        s.output.headphoneEq = loaded;
        s.output.headphoneEqName = entry.name;
      });
      setQuery("");
      toast({ title: `${entry.name} の補正を適用しました`, variant: "success" });
    } catch (e) {
      toast({ title: "補正データを取得できません", description: String(e), variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const importFile = async () => {
    const path = await pickFile([{ name: "Equalizer APO / AutoEq", extensions: ["txt"] }]);
    if (!path) return;
    try {
      const text = await api.readTextFile(path);
      const loaded = await api.parseEqText(text);
      const fileName = path.split(/[\\/]/).pop()?.replace(/\.txt$/i, "") ?? "読み込んだ補正";
      update((s) => {
        s.output.headphoneEq = loaded;
        s.output.headphoneEqName = fileName;
      });
      toast({ title: "補正を読み込みました", variant: "success" });
    } catch (e) {
      toast({ title: "読み込めません", description: String(e), variant: "error" });
    }
  };

  const clear = () =>
    update((s) => {
      s.output.headphoneEq = { enabled: false, autoPreamp: true, preampDb: 0, bands: [] };
      s.output.headphoneEqName = null;
    });

  return (
    <>
      <Section title="機種を探す" description="AutoEq の測定データから、ヘッドホンのクセを打ち消す補正を取り込みます。">
        <div className="px-5 py-4">
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-muted" />
            <input className="text-input w-full pl-9" placeholder="機種名（例：WH-1000XM5、HD 650）" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          {results.length > 0 && (
            <ul className="mt-2 max-h-[260px] overflow-y-auto rounded-[10px] border border-border-subtle">
              {results.map((r) => (
                <li key={r.path}>
                  <button className="focus-ring flex w-full items-center justify-between px-3 py-2 text-left text-[13px] hover:bg-surface-hover disabled:opacity-50" onClick={() => apply(r)} disabled={busy}>
                    <span className="text-text-primary">{r.name}</span>
                    <span className="text-[11px] text-text-muted">{r.source}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {query.trim().length >= 2 && !busy && results.length === 0 && <p className="mt-2 text-[12px] text-text-muted">見つかりません。別の書き方で探すか、ファイルから読み込んでください。</p>}
          <div className="mt-3 flex gap-2">
            <Button onClick={importFile}>
              <FileUp className="h-3.5 w-3.5" /> ファイルから読み込む
            </Button>
          </div>
        </div>
      </Section>

      <Section
        title={settings.output.headphoneEqName ? `補正：${settings.output.headphoneEqName}` : "補正なし"}
        aside={
          settings.output.headphoneEqName ? (
            <Button variant="ghost" onClick={clear}>
              <X className="h-3.5 w-3.5" /> 外す
            </Button>
          ) : undefined
        }
      >
        <div className="flex items-center justify-between border-b border-border-subtle px-5 py-3">
          <span className="text-[13px] text-text-secondary">補正を使う</span>
          <Toggle checked={eq.enabled} onChange={(v) => update((s) => void (s.output.headphoneEq.enabled = v))} />
        </div>
        <div className="px-5 py-4">
          <EqEditor eq={eq} onChange={(e) => update((s) => void (s.output.headphoneEq = e))} />
        </div>
      </Section>
    </>
  );
}
