// Rust 側（crates/engine/src/settings.rs など）と同じ形の型。

export type FilterKind = "peaking" | "low_shelf" | "high_shelf" | "low_pass" | "high_pass";
export type DenoiseMode = "off" | "light" | "high";
export type SpeakerMode = "off" | "gate" | "extract";
export type ChannelId = "game" | "chat";

export interface Band {
  enabled: boolean;
  kind: FilterKind;
  freq: number;
  gainDb: number;
  q: number;
}

export interface EqSettings {
  enabled: boolean;
  autoPreamp: boolean;
  preampDb: number;
  bands: Band[];
}

export interface GateSettings {
  enabled: boolean;
  thresholdDb: number;
  rangeDb: number;
  attackMs: number;
  holdMs: number;
  releaseMs: number;
}

export interface CompressorSettings {
  enabled: boolean;
  thresholdDb: number;
  ratio: number;
  kneeDb: number;
  attackMs: number;
  releaseMs: number;
  makeupDb: number;
}

export interface LevelerSettings {
  enabled: boolean;
  thresholdDb: number;
  ratio: number;
  maxBoostDb: number;
  floorDb: number;
  attackMs: number;
  releaseMs: number;
}

export interface SurroundSettings {
  enabled: boolean;
  room: number;
  virtualizeStereo: boolean;
}

export interface SpeakerSettings {
  mode: SpeakerMode;
  profile: string | null;
  threshold: number;
  attenuationDb: number;
  holdMs: number;
  extractModel: string | null;
}

export interface MicSettings {
  enabled: boolean;
  inputDevice: string | null;
  virtualOutput: string | null;
  denoise: DenoiseMode;
  strengthDb: number;
  highpass: boolean;
  gate: GateSettings;
  compressor: CompressorSettings;
  gainDb: number;
  limiterDb: number;
  muted: boolean;
  monitor: boolean;
  monitorDb: number;
  speaker: SpeakerSettings;
  echoCancel: boolean;
}

export interface OutputSettings {
  enabled: boolean;
  headphones: string | null;
  gameSource: string | null;
  chatSource: string | null;
  chatmix: number;
  masterDb: number;
  headphoneEq: EqSettings;
  headphoneEqName: string | null;
}

export interface ChannelSettings {
  volumeDb: number;
  muted: boolean;
  preset: string;
  eq: EqSettings;
  leveler: LevelerSettings;
  surround: SurroundSettings;
  denoise: boolean;
  denoiseStrengthDb: number;
}

export interface StreamSettings {
  enabled: boolean;
  device: string | null;
  gameDb: number;
  chatDb: number;
  micDb: number;
  includeMic: boolean;
}

export interface RoutingRule {
  process: string;
  channel: ChannelId;
}

export interface RoutingSettings {
  enabled: boolean;
  rules: RoutingRule[];
  setDefaultDevices: boolean;
  gameSink: string | null;
  chatSink: string | null;
  savedDefaultConsole: string | null;
  savedDefaultCommunications: string | null;
}

export interface GameProfile {
  id: string;
  name: string;
  processes: string[];
  game: ChannelSettings;
  chatmix: number;
}

export interface ProfileSettings {
  autoSwitch: boolean;
  items: GameProfile[];
  active: string | null;
}

export interface HotkeySettings {
  toggleMicMute: string | null;
  nextPreset: string | null;
  chatmixGame: string | null;
  chatmixChat: string | null;
  toggleSurround: string | null;
}

export interface AppSettings {
  overlay: boolean;
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  onboarded: boolean;
}

export interface ChannelPreset {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
  eq: EqSettings;
  leveler: LevelerSettings;
}

export interface Settings {
  version: number;
  mic: MicSettings;
  output: OutputSettings;
  game: ChannelSettings;
  chat: ChannelSettings;
  stream: StreamSettings;
  routing: RoutingSettings;
  profiles: ProfileSettings;
  hotkeys: HotkeySettings;
  app: AppSettings;
  customPresets: ChannelPreset[];
}

export interface DeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
  isDefaultCommunications: boolean;
}

export interface DeviceList {
  inputs: DeviceInfo[];
  outputs: DeviceInfo[];
}

export interface Level {
  peak: number;
  rms: number;
}

export interface Meters {
  micIn: Level;
  micOut: Level;
  game: Level;
  chat: Level;
  master: Level;
  stream: Level;
  gateOpen: boolean;
  micReductionDb: number;
  limiterReductionDb: number;
  speakerScore: number | null;
  speakerState: "silent" | "self" | "other" | "checking";
  micLoad: number;
  outputLoad: number;
}

export interface PipelineStatus {
  running: boolean;
  error: string | null;
  devices: string[];
  latencyMs: number;
}

export interface EngineStatus {
  mic: PipelineStatus;
  output: PipelineStatus;
  notices: string[];
  cables: { mic: boolean; game: boolean; chat: boolean };
}

export interface StatusReply {
  status: EngineStatus;
  notices: string[];
  activeProfile: string | null;
}

export interface AppSession {
  pid: number;
  exe: string;
  name: string;
  channel: ChannelId;
  device: string;
  routed: boolean;
  active: boolean;
}

export interface RoutingReport {
  sessions: AppSession[];
  available: boolean;
  error: string | null;
}

export interface ProfileSummary {
  id: string;
  name: string;
  created: number;
  voicedSeconds: number;
}

export interface Credit {
  name: string;
  license: string;
  url: string;
}

export interface Bootstrap {
  settings: Settings;
  devices: DeviceList;
  presets: ChannelPreset[];
  speakerProfiles: ProfileSummary[];
  speakerModelReady: boolean;
  version: string;
  windows: boolean;
  credits: Credit[];
}

export interface AutoEqEntry {
  name: string;
  path: string;
  source: string;
}
