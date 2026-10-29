// Rust 側のコマンド呼び出し。ブラウザで画面だけを確認するときはモックを返す。

import type {
  AutoEqEntry,
  Bootstrap,
  ChannelId,
  DeviceList,
  EqSettings,
  Meters,
  ProfileSummary,
  RoutingReport,
  Settings,
  StatusReply,
} from "./types";
import defaultSettings from "./default-settings.json";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mock(cmd, args) as Promise<T>;
}

export const api = {
  bootstrap: () => call<Bootstrap>("get_bootstrap"),
  updateSettings: (settings: Settings) => call<void>("update_settings", { settings }),
  devices: () => call<DeviceList>("get_devices"),
  meters: () => call<Meters>("get_meters"),
  status: () => call<StatusReply>("get_status"),
  recordTest: (seconds: number) => call<{ raw: string; processed: string }>("record_test", { seconds }),
  routingReport: () => call<RoutingReport>("routing_report"),
  setAppChannel: (exe: string, channel: ChannelId) => call<Settings>("set_app_channel", { exe, channel }),
  autoeqSearch: (query: string) => call<AutoEqEntry[]>("autoeq_search", { query }),
  autoeqLoad: (path: string) => call<EqSettings>("autoeq_load", { path }),
  parseEqText: (text: string) => call<EqSettings>("parse_eq_text", { text }),
  readTextFile: (path: string) => call<string>("read_text_file", { path }),
  downloadSpeakerModel: () => call<void>("download_speaker_model"),
  listSpeakerProfiles: () => call<ProfileSummary[]>("list_speaker_profiles"),
  enrollSpeaker: (name: string, seconds: number) => call<ProfileSummary>("enroll_speaker", { name, seconds }),
  deleteSpeakerProfile: (id: string) => call<void>("delete_speaker_profile", { id }),
  resetAudio: () => call<void>("reset_audio"),
  restartEngine: () => call<void>("restart_engine"),
  foregroundApp: () => call<string | null>("foreground_app"),
  openLogDir: () => call<void>("open_log_dir"),
  quit: () => call<void>("quit_app"),
};

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

export async function openUrl(url: string) {
  if (!inTauri) {
    window.open(url, "_blank");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function pickFile(filters: { name: string; extensions: string[] }[]): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false, filters });
  return typeof r === "string" ? r : null;
}

export async function windowAction(action: "minimize" | "maximize" | "close") {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  if (action === "minimize") await w.minimize();
  else if (action === "maximize") await w.toggleMaximize();
  else await w.close();
}

// ---------------------------------------------------------------------------
// ブラウザ確認用のモック
// ---------------------------------------------------------------------------

let mockSettings: Settings = structuredClone(defaultSettings) as unknown as Settings;
let t = 0;

const mockDevices: DeviceList = {
  inputs: [
    { id: "mic1", name: "マイク (USB Audio Device)", isDefault: true, isDefaultCommunications: true },
    { id: "cableA-out", name: "CABLE-A Output (VB-Audio Cable A)", isDefault: false, isDefaultCommunications: false },
    { id: "cableB-out", name: "CABLE-B Output (VB-Audio Cable B)", isDefault: false, isDefaultCommunications: false },
  ],
  outputs: [
    { id: "hp", name: "ヘッドホン (WH-1000XM5)", isDefault: false, isDefaultCommunications: false },
    { id: "cable", name: "CABLE Input (VB-Audio Virtual Cable)", isDefault: false, isDefaultCommunications: false },
    { id: "cableA", name: "CABLE-A Input (VB-Audio Cable A)", isDefault: true, isDefaultCommunications: false },
    { id: "cableB", name: "CABLE-B Input (VB-Audio Cable B)", isDefault: false, isDefaultCommunications: true },
  ],
};

function level(base: number) {
  const v = base + Math.sin(t / 7) * 8 + Math.sin(t / 3.1) * 4;
  return { peak: v + 4, rms: v };
}

async function mock(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "get_bootstrap":
      return {
        settings: mockSettings,
        devices: mockDevices,
        presets: [
          { id: "flat", name: "フラット", description: "EQをかけない", builtin: true, eq: { enabled: false, autoPreamp: true, preampDb: 0, bands: [] }, leveler: mockSettings.game.leveler },
          {
            id: "footsteps",
            name: "足音強調",
            description: "低音を抑えて足音の帯域を持ち上げ、小さい音も聞き取りやすくする",
            builtin: true,
            eq: {
              enabled: true,
              autoPreamp: true,
              preampDb: 0,
              bands: [
                { enabled: true, kind: "low_shelf", freq: 120, gainDb: -6, q: 0.707 },
                { enabled: true, kind: "peaking", freq: 3200, gainDb: 4, q: 1 },
                { enabled: true, kind: "peaking", freq: 6500, gainDb: -1.5, q: 2 },
              ],
            },
            leveler: { ...mockSettings.game.leveler, enabled: true },
          },
        ],
        speakerProfiles: [],
        speakerModelReady: false,
        version: "0.1.0",
        windows: false,
        credits: [],
      } satisfies Bootstrap;
    case "update_settings":
      mockSettings = args!.settings as Settings;
      return;
    case "get_devices":
      return mockDevices;
    case "get_meters":
      t += 1;
      return {
        micIn: level(-30),
        micOut: level(-24),
        game: level(-18),
        chat: level(-26),
        master: level(-14),
        stream: level(-60),
        gateOpen: Math.sin(t / 9) > -0.3,
        micReductionDb: 3,
        limiterReductionDb: 0,
        speakerScore: null,
        speakerState: "silent",
        micLoad: 0.08,
        outputLoad: 0.05,
      } satisfies Meters;
    case "get_status":
      return {
        status: {
          mic: { running: true, error: null, devices: ["マイク (USB Audio Device)", "CABLE Input"], latencyMs: 38 },
          output: { running: true, error: null, devices: ["ヘッドホン"], latencyMs: 45 },
          notices: [],
          cables: { mic: true, game: true, chat: true },
        },
        notices: [],
        activeProfile: null,
      } satisfies StatusReply;
    case "routing_report":
      return {
        available: true,
        error: null,
        sessions: [
          { pid: 1, exe: "discord.exe", name: "Discord", channel: "chat", device: "CABLE-B Input", routed: true, active: true },
          { pid: 2, exe: "valorant-win64-shipping.exe", name: "VALORANT", channel: "game", device: "CABLE-A Input", routed: true, active: true },
          { pid: 3, exe: "spotify.exe", name: "Spotify", channel: "game", device: "ヘッドホン", routed: false, active: false },
        ],
      } satisfies RoutingReport;
    case "list_speaker_profiles":
      return [];
    case "autoeq_search":
      return [{ name: "Sony WH-1000XM5", path: "oratory1990/over-ear/Sony%20WH-1000XM5", source: "oratory1990" }];
    default:
      return null;
  }
}
