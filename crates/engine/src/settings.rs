//! アプリ全体の設定。JSON で `%APPDATA%\Clarivo\settings.json` に保存する。
//!
//! 画面（TypeScript）とそのままやり取りするため、名前は camelCase にする。
//! 古い設定ファイルに無い項目は既定値で補う（`#[serde(default)]`）。

use clarivo_denoise::DenoiseMode;
use clarivo_dsp::dynamics::{CompressorSettings, GateSettings, LevelerSettings};
use clarivo_dsp::peq::EqSettings;
use clarivo_dsp::presets::ChannelPreset;
use clarivo_dsp::surround::SurroundSettings;
use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub mic: MicSettings,
    pub output: OutputSettings,
    pub game: ChannelSettings,
    pub chat: ChannelSettings,
    pub stream: StreamSettings,
    pub routing: RoutingSettings,
    pub profiles: ProfileSettings,
    pub hotkeys: HotkeySettings,
    pub app: AppSettings,
    /// ユーザーが保存したEQプリセット
    pub custom_presets: Vec<ChannelPreset>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            mic: MicSettings::default(),
            output: OutputSettings::default(),
            game: ChannelSettings::game_default(),
            chat: ChannelSettings::chat_default(),
            stream: StreamSettings::default(),
            routing: RoutingSettings::default(),
            profiles: ProfileSettings::default(),
            hotkeys: HotkeySettings::default(),
            app: AppSettings::default(),
            custom_presets: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// マイク（フェーズ1・4）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MicSettings {
    pub enabled: bool,
    /// 実マイクのデバイスID（None = Windows の既定の通信用マイク）
    pub input_device: Option<String>,
    /// 仮想マイクの出力先（VB-CABLE の「CABLE Input」）。None なら自動で探す
    pub virtual_output: Option<String>,
    pub denoise: DenoiseMode,
    /// ノイズ除去の強さ（抑制量の上限, 0〜100dB）
    pub strength_db: f32,
    pub highpass: bool,
    pub gate: GateSettings,
    pub compressor: CompressorSettings,
    /// 入力ゲイン（dB）
    pub gain_db: f32,
    /// リミッターの上限（dBFS）
    pub limiter_db: f32,
    pub muted: bool,
    pub monitor: bool,
    pub monitor_db: f32,
    pub speaker: SpeakerSettings,
    /// スピーカー使用時のエコーキャンセル
    pub echo_cancel: bool,
}

impl Default for MicSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_device: None,
            virtual_output: None,
            denoise: DenoiseMode::Light,
            strength_db: 60.0,
            highpass: true,
            gate: GateSettings::default(),
            compressor: CompressorSettings::default(),
            gain_db: 0.0,
            limiter_db: -1.0,
            muted: false,
            monitor: false,
            monitor_db: -6.0,
            speaker: SpeakerSettings::default(),
            echo_cancel: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerMode {
    #[default]
    Off,
    /// 段階A：登録した声のときだけマイクを開く
    Gate,
    /// 段階B：話者抽出モデルで自分の声だけを残す（試験機能）
    Extract,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeakerSettings {
    pub mode: SpeakerMode,
    /// 使う声紋プロファイルのID
    pub profile: Option<String>,
    /// 本人と判定する類似度のしきい値（0〜1）
    pub threshold: f32,
    /// 他人の声と判定したときの減衰量（dB, 負）
    pub attenuation_db: f32,
    /// 判定が変わってから閉じるまでの時間（ms）
    pub hold_ms: f32,
    /// 話者抽出モデル（ONNX）のパス
    pub extract_model: Option<String>,
}

impl Default for SpeakerSettings {
    fn default() -> Self {
        Self {
            mode: SpeakerMode::Off,
            profile: None,
            threshold: 0.45,
            attenuation_db: -40.0,
            hold_ms: 600.0,
            extract_model: None,
        }
    }
}

// ---------------------------------------------------------------------------
// 出力（フェーズ2・3）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
    pub enabled: bool,
    /// 実際に聞くヘッドホン（None = 仮想ケーブル以外の既定デバイス）
    pub headphones: Option<String>,
    /// Game チャンネルの取り込み元（VB-CABLE A の「CABLE-A Output」）
    pub game_source: Option<String>,
    /// Chat チャンネルの取り込み元（VB-CABLE B の「CABLE-B Output」）
    pub chat_source: Option<String>,
    /// -1（ゲームのみ）〜 +1（チャットのみ）
    pub chatmix: f32,
    pub master_db: f32,
    /// ヘッドホン補正（AutoEq）
    pub headphone_eq: EqSettings,
    pub headphone_eq_name: Option<String>,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            headphones: None,
            game_source: None,
            chat_source: None,
            chatmix: 0.0,
            master_db: 0.0,
            headphone_eq: EqSettings::default(),
            headphone_eq_name: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelSettings {
    pub volume_db: f32,
    pub muted: bool,
    /// 選択中のプリセットID（EQを手で変えたら "custom"）
    pub preset: String,
    pub eq: EqSettings,
    pub leveler: LevelerSettings,
    /// Game のみで使う
    pub surround: SurroundSettings,
    /// Chat のみで使う：受信した声のノイズ除去
    pub denoise: bool,
    pub denoise_strength_db: f32,
}

impl ChannelSettings {
    pub fn game_default() -> Self {
        Self {
            volume_db: 0.0,
            muted: false,
            preset: "flat".into(),
            eq: EqSettings::default(),
            leveler: LevelerSettings::default(),
            surround: SurroundSettings::default(),
            denoise: false,
            denoise_strength_db: 30.0,
        }
    }

    pub fn chat_default() -> Self {
        Self {
            preset: "flat".into(),
            denoise: true,
            ..Self::game_default()
        }
    }
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self::game_default()
    }
}

// ---------------------------------------------------------------------------
// 配信用ミックス（フェーズ5）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamSettings {
    pub enabled: bool,
    /// 配信用ミックスの出力先（OBS が取り込む仮想デバイス）
    pub device: Option<String>,
    pub game_db: f32,
    pub chat_db: f32,
    pub mic_db: f32,
    pub include_mic: bool,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            device: None,
            game_db: 0.0,
            chat_db: 0.0,
            mic_db: 0.0,
            include_mic: false,
        }
    }
}

// ---------------------------------------------------------------------------
// アプリ別振り分け（フェーズ3）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChannelId {
    Game,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    /// 実行ファイル名（小文字、例: "discord.exe"）
    pub process: String,
    pub channel: ChannelId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoutingSettings {
    pub enabled: bool,
    pub rules: Vec<RoutingRule>,
    /// Windows の既定の再生デバイスを Game に、通信デバイスを Chat に切り替える
    pub set_default_devices: bool,
    /// Game 用の再生先（VB-CABLE A の「CABLE-A Input」）
    pub game_sink: Option<String>,
    /// Chat 用の再生先（VB-CABLE B の「CABLE-B Input」）
    pub chat_sink: Option<String>,
    /// 振り分け前の既定デバイス（終了時に戻す）
    pub saved_default_console: Option<String>,
    pub saved_default_communications: Option<String>,
}

pub const DEFAULT_CHAT_APPS: &[&str] = &[
    "discord.exe",
    "discordptb.exe",
    "discordcanary.exe",
    "vesktop.exe",
    "teams.exe",
    "ms-teams.exe",
    "zoom.exe",
    "slack.exe",
    "skype.exe",
    "line.exe",
    "teamspeak.exe",
    "ts3client_win64.exe",
    "mumble.exe",
    "guilded.exe",
];

impl Default for RoutingSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: DEFAULT_CHAT_APPS
                .iter()
                .map(|p| RoutingRule {
                    process: (*p).into(),
                    channel: ChannelId::Chat,
                })
                .collect(),
            set_default_devices: true,
            game_sink: None,
            chat_sink: None,
            saved_default_console: None,
            saved_default_communications: None,
        }
    }
}

impl RoutingSettings {
    /// 実行ファイル名からチャンネルを決める（規則に無ければ Game）。
    pub fn channel_for(&self, exe: &str) -> ChannelId {
        let exe = exe.to_ascii_lowercase();
        self.rules
            .iter()
            .find(|r| r.process.eq_ignore_ascii_case(&exe))
            .map(|r| r.channel)
            .unwrap_or(ChannelId::Game)
    }

    pub fn set_rule(&mut self, exe: &str, channel: ChannelId) {
        let exe = exe.to_ascii_lowercase();
        if let Some(r) = self.rules.iter_mut().find(|r| r.process == exe) {
            r.channel = channel;
        } else {
            self.rules.push(RoutingRule {
                process: exe,
                channel,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// ゲームごとのプロファイル（フェーズ5）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    /// このプロファイルに切り替える実行ファイル名
    pub processes: Vec<String>,
    pub game: ChannelSettings,
    pub chatmix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileSettings {
    pub auto_switch: bool,
    pub items: Vec<GameProfile>,
    /// 今適用しているプロファイル（None = 通常設定）
    pub active: Option<String>,
}

// ---------------------------------------------------------------------------
// ホットキー・アプリ（フェーズ5）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
    pub toggle_mic_mute: Option<String>,
    pub next_preset: Option<String>,
    pub chatmix_game: Option<String>,
    pub chatmix_chat: Option<String>,
    pub toggle_surround: Option<String>,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_mic_mute: Some("Ctrl+Alt+M".into()),
            next_preset: Some("Ctrl+Alt+P".into()),
            chatmix_game: Some("Ctrl+Alt+Left".into()),
            chatmix_chat: Some("Ctrl+Alt+Right".into()),
            toggle_surround: Some("Ctrl+Alt+S".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub overlay: bool,
    pub autostart: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    /// 初回セットアップを済ませたか
    pub onboarded: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            overlay: true,
            autostart: false,
            start_minimized: false,
            close_to_tray: true,
            onboarded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_missing_fields() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        // 古い設定ファイル（項目が足りない）でも読める
        let partial: Settings = serde_json::from_str(r#"{"mic":{"strengthDb":30}}"#).unwrap();
        assert_eq!(partial.mic.strength_db, 30.0);
        assert!(partial.routing.enabled);
    }

    #[test]
    fn routing_rules() {
        let mut r = RoutingSettings::default();
        assert_eq!(r.channel_for("Discord.exe"), ChannelId::Chat);
        assert_eq!(r.channel_for("valorant.exe"), ChannelId::Game);
        r.set_rule("Spotify.exe", ChannelId::Chat);
        assert_eq!(r.channel_for("spotify.exe"), ChannelId::Chat);
        r.set_rule("discord.exe", ChannelId::Game);
        assert_eq!(r.channel_for("discord.exe"), ChannelId::Game);
    }
}
