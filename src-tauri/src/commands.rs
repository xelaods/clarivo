//! 画面（React）から呼ばれる処理。

use crate::state::{save_settings, AppState};
use crate::{autoeq, commit_settings, notify};
use base64::Engine as _;
use clarivo_dsp::peq::EqSettings;
use clarivo_dsp::presets::{builtin_presets, ChannelPreset};
use clarivo_dsp::wav::encode_wav_i16;
use clarivo_engine::devices::DeviceList;
use clarivo_engine::settings::{ChannelId, Settings};
use clarivo_engine::state::{EngineStatus, MeterSnapshot};
use clarivo_routing::RoutingReport;
use clarivo_speaker::{EmbeddingModel, ProfileStore, ProfileSummary, MODEL_CREDIT, MODEL_URL, WINDOW_SAMPLES};
use futures_util::StreamExt;
use serde::Serialize;
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: Settings,
    pub devices: DeviceList,
    pub presets: Vec<ChannelPreset>,
    pub speaker_profiles: Vec<ProfileSummary>,
    pub speaker_model_ready: bool,
    pub version: String,
    pub windows: bool,
    pub credits: Vec<Credit>,
}

#[derive(Serialize, Clone)]
pub struct Credit {
    pub name: &'static str,
    pub license: &'static str,
    pub url: &'static str,
}

fn credits() -> Vec<Credit> {
    vec![
        Credit { name: "DeepFilterNet", license: "MIT / Apache-2.0", url: "https://github.com/Rikorose/DeepFilterNet" },
        Credit { name: "RNNoise（nnnoiseless）", license: "BSD-3-Clause", url: "https://github.com/jneem/nnnoiseless" },
        Credit { name: "tract", license: "MIT / Apache-2.0", url: "https://github.com/sonos/tract" },
        Credit { name: MODEL_CREDIT, license: "CC BY 4.0", url: "https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM" },
        Credit { name: "sonora（WebRTC 音声処理）", license: "BSD-3-Clause", url: "https://github.com/dignifiedquire/sonora" },
        Credit { name: "wasapi-rs", license: "MIT", url: "https://github.com/HEnquist/wasapi-rs" },
        Credit { name: "AutoEq", license: "MIT", url: "https://github.com/jaakkopasanen/AutoEq" },
        Credit { name: "EasyUI", license: "MIT", url: "https://github.com/Surajmaurya1/easyui" },
        Credit { name: "Tauri", license: "MIT / Apache-2.0", url: "https://tauri.app" },
        Credit { name: "VB-CABLE（別途インストール）", license: "Donationware", url: "https://vb-audio.com/Cable/" },
    ]
}

#[tauri::command]
pub fn get_bootstrap(state: State<AppState>) -> CmdResult<Bootstrap> {
    let settings = state.settings.lock().clone();
    let store = ProfileStore::new(&state.paths.speakers_dir);
    Ok(Bootstrap {
        devices: clarivo_engine::list_devices().unwrap_or_default(),
        presets: builtin_presets(),
        speaker_profiles: store.list().iter().map(ProfileSummary::from).collect(),
        speaker_model_ready: state.paths.speaker_model().exists(),
        version: env!("CARGO_PKG_VERSION").into(),
        windows: cfg!(windows),
        credits: credits(),
        settings,
    })
}

#[tauri::command]
pub fn update_settings(app: AppHandle, settings: Settings) -> CmdResult<()> {
    commit_settings(&app, settings, true);
    Ok(())
}

#[tauri::command]
pub fn get_devices() -> CmdResult<DeviceList> {
    clarivo_engine::list_devices().map_err(err)
}

#[tauri::command]
pub fn get_meters(state: State<AppState>) -> MeterSnapshot {
    state.engine.lock().shared().meters.snapshot()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReply {
    pub status: EngineStatus,
    pub notices: Vec<String>,
    pub active_profile: Option<String>,
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> StatusReply {
    let shared = state.engine.lock().shared();
    let notices = shared.take_notices();
    let status = shared.status.lock().clone();
    StatusReply {
        status,
        notices,
        active_profile: state.settings.lock().profiles.active.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingReply {
    /// data URL（audio/wav）
    pub raw: String,
    pub processed: String,
}

fn wav_data_url(samples: &[f32]) -> String {
    let wav = encode_wav_i16(samples, 1, 48_000);
    format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(wav)
    )
}

#[tauri::command]
pub async fn record_test(app: AppHandle, seconds: f32) -> CmdResult<RecordingReply> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let pending = state.engine.lock().start_record(seconds).map_err(err)?;
        let rec = pending.wait().map_err(err)?;
        Ok(RecordingReply {
            raw: wav_data_url(&rec.raw),
            processed: wav_data_url(&rec.processed),
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn routing_report(state: State<AppState>) -> RoutingReport {
    state.routing_report.lock().clone()
}

#[tauri::command]
pub fn set_app_channel(app: AppHandle, exe: String, channel: ChannelId) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().clone();
    s.routing.set_rule(&exe, channel);
    commit_settings(&app, s.clone(), true);
    state.router.tick_now();
    Ok(s)
}

#[tauri::command]
pub async fn autoeq_search(app: AppHandle, query: String) -> CmdResult<Vec<autoeq::AutoEqEntry>> {
    let state = app.state::<AppState>();
    let cached = state.autoeq_index.lock().clone();
    let index = match cached {
        Some(i) => i,
        None => {
            let file = state.paths.cache_dir.join("autoeq-index.md");
            let idx = autoeq::fetch_index(&file).await?;
            *state.autoeq_index.lock() = Some(idx.clone());
            idx
        }
    };
    Ok(autoeq::search(&index, &query))
}

#[tauri::command]
pub async fn autoeq_load(path: String) -> CmdResult<EqSettings> {
    autoeq::load(&path).await
}

#[tauri::command]
pub fn parse_eq_text(text: String) -> CmdResult<EqSettings> {
    clarivo_dsp::autoeq::parse_equalizer_apo(&text).map_err(err)
}

#[tauri::command]
pub fn read_text_file(path: String) -> CmdResult<String> {
    let meta = std::fs::metadata(&path).map_err(err)?;
    if meta.len() > 1_000_000 {
        return Err("ファイルが大きすぎます".into());
    }
    std::fs::read_to_string(&path).map_err(err)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub received: u64,
    pub total: Option<u64>,
}

#[tauri::command]
pub async fn download_speaker_model(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let dest = state.paths.speaker_model();
    if dest.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&state.paths.models_dir).map_err(err)?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("Clarivo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(err)?;
    let resp = client
        .get(MODEL_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("話者モデルをダウンロードできません: {e}"))?;
    let total = resp.content_length();
    let tmp = dest.with_extension("onnx.part");
    let mut file = std::fs::File::create(&tmp).map_err(err)?;
    let mut received = 0u64;
    let mut stream = resp.bytes_stream();
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("ダウンロードが途切れました: {e}"))?;
        file.write_all(&chunk).map_err(err)?;
        received += chunk.len() as u64;
        if received - last_emit > 256 * 1024 {
            last_emit = received;
            let _ = app.emit("model-progress", Progress { received, total });
        }
    }
    drop(file);
    if received < 1_000_000 {
        let _ = std::fs::remove_file(&tmp);
        return Err("ダウンロードしたファイルが小さすぎます".into());
    }
    std::fs::rename(&tmp, &dest).map_err(err)?;
    let _ = app.emit("model-progress", Progress { received, total: Some(received) });
    Ok(())
}

#[tauri::command]
pub fn list_speaker_profiles(state: State<AppState>) -> Vec<ProfileSummary> {
    ProfileStore::new(&state.paths.speakers_dir)
        .list()
        .iter()
        .map(ProfileSummary::from)
        .collect()
}

#[tauri::command]
pub async fn enroll_speaker(app: AppHandle, name: String, seconds: f32) -> CmdResult<ProfileSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let model_path = state.paths.speaker_model();
        if !model_path.exists() {
            return Err("先に話者モデルをダウンロードしてください".to_string());
        }
        let pending = state.engine.lock().start_record(seconds).map_err(err)?;
        let rec = pending.wait().map_err(err)?;
        let mut model = EmbeddingModel::load(&model_path, WINDOW_SAMPLES).map_err(err)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let id = format!("p{now:x}");
        let name = if name.trim().is_empty() { "自分".to_string() } else { name.trim().to_string() };
        let profile = clarivo_speaker::profile::enroll(&mut model, &rec.raw, id, name, now).map_err(err)?;
        ProfileStore::new(&state.paths.speakers_dir)
            .save(&profile)
            .map_err(err)?;
        // 初めての登録なら、そのまま使う声に選ぶ
        let mut s = state.settings.lock().clone();
        if s.mic.speaker.profile.is_none() {
            s.mic.speaker.profile = Some(profile.id.clone());
            commit_settings(&app, s, false);
        }
        Ok(ProfileSummary::from(&profile))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn delete_speaker_profile(app: AppHandle, id: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    ProfileStore::new(&state.paths.speakers_dir)
        .delete(&id)
        .map_err(err)?;
    let mut s = state.settings.lock().clone();
    if s.mic.speaker.profile.as_deref() == Some(id.as_str()) {
        s.mic.speaker.profile = None;
        s.mic.speaker.mode = clarivo_engine::settings::SpeakerMode::Off;
        commit_settings(&app, s, false);
    }
    Ok(())
}

#[tauri::command]
pub fn reset_audio(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    state.router.restore_blocking();
    let mut s = state.settings.lock().clone();
    s.routing.enabled = false;
    let mut routing = s.routing.clone();
    clarivo_routing::Router::reset_all(&mut routing).map_err(err)?;
    s.routing = routing;
    commit_settings(&app, s, false);
    notify(&app, "アプリ別の出力先と既定デバイスを元に戻しました");
    Ok(())
}

#[tauri::command]
pub fn restart_engine(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().clone();
    let mut speaker = state.speaker.lock();
    let mut engine = state.engine.lock();
    engine.start(&settings);
    speaker.reset_tap(&engine);
    if let Some(p) = speaker.sync(&engine, &settings, &state.paths) {
        notify(&app, &p);
    }
    Ok(())
}

#[tauri::command]
pub fn save_settings_now(state: State<AppState>) {
    let s = state.settings.lock().clone();
    save_settings(&state.paths, &s);
}

#[tauri::command]
pub fn foreground_app() -> Option<String> {
    clarivo_routing::foreground_process().map(|(_, exe)| exe)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
