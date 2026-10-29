// 画面全体で共有する状態（設定・デバイス・状態表示・お知らせ）。

import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { api, listen } from "./api";
import type { Bootstrap, ChannelPreset, DeviceList, ProfileSummary, Settings, StatusReply } from "./types";

export interface Toast {
  id: number;
  title: string;
  description?: string;
  variant?: "default" | "success" | "warning" | "error" | "info";
}

interface Store {
  boot: Bootstrap;
  settings: Settings;
  devices: DeviceList;
  status: StatusReply | null;
  presets: ChannelPreset[];
  speakerProfiles: ProfileSummary[];
  speakerModelReady: boolean;
  /** 設定を書き換える（関数の中で draft を直接変更してよい） */
  update: (fn: (draft: Settings) => void) => void;
  replaceSettings: (s: Settings) => void;
  refreshDevices: () => Promise<void>;
  refreshSpeakers: () => Promise<void>;
  setSpeakerModelReady: (v: boolean) => void;
  toasts: Toast[];
  toast: (t: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("StoreProvider の外で使われています");
  return s;
}

export function StoreProvider({ boot, children }: { boot: Bootstrap; children: React.ReactNode }) {
  const [settings, setSettings] = useState<Settings>(boot.settings);
  const [devices, setDevices] = useState<DeviceList>(boot.devices);
  const [status, setStatus] = useState<StatusReply | null>(null);
  const [speakerProfiles, setSpeakerProfiles] = useState<ProfileSummary[]>(boot.speakerProfiles);
  const [speakerModelReady, setSpeakerModelReady] = useState(boot.speakerModelReady);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const pending = useRef<Settings | null>(null);
  const timer = useRef<number | null>(null);
  const toastId = useRef(1);

  const toast = useCallback((t: Omit<Toast, "id">) => {
    const id = toastId.current++;
    setToasts((ts) => [...ts.slice(-2), { ...t, id }]);
  }, []);
  const dismissToast = useCallback((id: number) => setToasts((ts) => ts.filter((t) => t.id !== id)), []);

  // つまみを動かしている間は送りすぎないよう、少しまとめてから送る
  const flush = useCallback(() => {
    timer.current = null;
    const s = pending.current;
    pending.current = null;
    if (s) {
      api.updateSettings(s).catch((e) => toast({ title: "設定を反映できません", description: String(e), variant: "error" }));
    }
  }, [toast]);

  const update = useCallback(
    (fn: (draft: Settings) => void) => {
      setSettings((prev) => {
        const next = structuredClone(prev);
        fn(next);
        pending.current = next;
        if (timer.current === null) timer.current = window.setTimeout(flush, 40);
        return next;
      });
    },
    [flush],
  );

  const replaceSettings = useCallback((s: Settings) => setSettings(s), []);

  const refreshDevices = useCallback(async () => {
    setDevices(await api.devices());
  }, []);

  const refreshSpeakers = useCallback(async () => {
    setSpeakerProfiles(await api.listSpeakerProfiles());
  }, []);

  // 状態とお知らせを 1 秒ごとに取りに行く
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const st = await api.status();
        if (!alive) return;
        setStatus(st);
        for (const n of st.notices) toast({ title: n, variant: "info" });
      } catch {
        /* 次の周期で再試行 */
      }
    };
    tick();
    const id = window.setInterval(tick, 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [toast]);

  // ホットキーやプロファイル切り替えなど、Rust 側で変わった設定を受け取る
  useEffect(() => {
    const offs: Promise<() => void>[] = [
      listen<Settings>("settings", (s) => setSettings(s)),
      listen<string>("notice", (msg) => toast({ title: msg, variant: "info" })),
    ];
    return () => {
      offs.forEach((p) => p.then((off) => off()));
    };
  }, [toast]);

  const presets = [...boot.presets, ...settings.customPresets];

  return (
    <Ctx.Provider
      value={{
        boot,
        settings,
        devices,
        status,
        presets,
        speakerProfiles,
        speakerModelReady,
        update,
        replaceSettings,
        refreshDevices,
        refreshSpeakers,
        setSpeakerModelReady,
        toasts,
        toast,
        dismissToast,
      }}
    >
      {children}
    </Ctx.Provider>
  );
}

/** メーターを毎秒 30 回取りに行く。 */
export function useMeters() {
  const [m, setM] = useState<Awaited<ReturnType<typeof api.meters>> | null>(null);
  useEffect(() => {
    let alive = true;
    let busy = false;
    const id = window.setInterval(async () => {
      if (busy || document.hidden) return;
      busy = true;
      try {
        const v = await api.meters();
        if (alive) setM(v);
      } finally {
        busy = false;
      }
    }, 33);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);
  return m;
}
