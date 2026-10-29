//! アプリ別の振り分け（フェーズ3）と、前面アプリの検出（フェーズ5）。
//!
//! Windows には「アプリごとの出力先」を設定する公開 API が無いため、EarTrumpet や
//! SoundVolumeView と同じく、非公開の `AudioPolicyConfig`（アプリ別の既定デバイス）と
//! `IPolicyConfig`（既定デバイスの切り替え）を使う。Windows の更新で動かなくなる
//! 可能性があるので、失敗しても音声エンジン自体は止めない。

use clarivo_engine::settings::ChannelId;
use serde::Serialize;

#[cfg(windows)]
mod win;

/// 音を出しているアプリ 1 つ分。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSession {
    pub pid: u32,
    /// 実行ファイル名（小文字）
    pub exe: String,
    /// 表示名
    pub name: String,
    /// 規則上のチャンネル
    pub channel: ChannelId,
    /// 今鳴っているデバイス名
    pub device: String,
    /// 振り分け先と一致しているか（アプリの再生をやり直すと一致する）
    pub routed: bool,
    pub active: bool,
}

/// 振り分けの結果。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingReport {
    pub sessions: Vec<AppSession>,
    /// 振り分けの仕組みが使えるか（非公開 API が見つかったか）
    pub available: bool,
    pub error: Option<String>,
}

#[cfg(windows)]
pub use win::{foreground_process, process_exe, Router};

#[cfg(not(windows))]
mod stub {
    use super::RoutingReport;
    use clarivo_engine::settings::RoutingSettings;

    /// Windows 以外では何もしない。
    #[derive(Default)]
    pub struct Router;

    impl Router {
        pub fn new() -> Self {
            Router
        }
        pub fn tick(&mut self, _s: &mut RoutingSettings) -> RoutingReport {
            RoutingReport::default()
        }
        pub fn restore(&mut self, _s: &mut RoutingSettings) {}
        pub fn reset_all(_s: &mut RoutingSettings) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub fn foreground_process() -> Option<(u32, String)> {
        None
    }

    pub fn process_exe(_pid: u32) -> Option<String> {
        None
    }
}

#[cfg(not(windows))]
pub use stub::{foreground_process, process_exe, Router};

/// ゲームごとのプロファイルを、前面アプリの実行ファイル名から選ぶ。
pub fn match_profile<'a>(
    profiles: &'a [clarivo_engine::settings::GameProfile],
    exe: &str,
) -> Option<&'a clarivo_engine::settings::GameProfile> {
    let exe = exe.to_ascii_lowercase();
    profiles
        .iter()
        .find(|p| p.processes.iter().any(|x| x.eq_ignore_ascii_case(&exe)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clarivo_engine::settings::{ChannelSettings, GameProfile};

    #[test]
    fn profile_matching_ignores_case() {
        let p = vec![GameProfile {
            id: "val".into(),
            name: "VALORANT".into(),
            processes: vec!["VALORANT-Win64-Shipping.exe".into()],
            game: ChannelSettings::game_default(),
            chatmix: 0.0,
        }];
        assert!(match_profile(&p, "valorant-win64-shipping.exe").is_some());
        assert!(match_profile(&p, "chrome.exe").is_none());
    }
}
