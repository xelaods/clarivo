//! Clarivo 本体（Tauri）。音声エンジンの起動、設定の保存、トレイ、ホットキー、
//! オーバーレイ、ゲームごとのプロファイル切り替えを受け持つ。

mod autoeq;
mod commands;
mod router;
mod speaker;
mod state;

use clarivo_dsp::presets::builtin_presets;
use clarivo_engine::settings::Settings;
use clarivo_engine::Engine;
use parking_lot::Mutex;
use state::{AppState, HotkeyAction, Paths, ProfileRuntime};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// 画面に短いお知らせを出す。
pub(crate) fn notify(app: &AppHandle, msg: &str) {
    log::info!("{msg}");
    let _ = app.emit("notice", msg.to_string());
}

/// 設定を反映して保存する。画面以外（ホットキーなど）からの変更は画面にも知らせる。
pub(crate) fn commit_settings(app: &AppHandle, new: Settings, from_ui: bool) {
    let state = app.state::<AppState>();
    let old = {
        let mut s = state.settings.lock();
        let old = s.clone();
        *s = new.clone();
        old
    };
    state::save_settings(&state.paths, &new);
    {
        let mut speaker = state.speaker.lock();
        let engine = state.engine.lock();
        engine.apply(&new);
        if old.mic.speaker != new.mic.speaker {
            if let Some(problem) = speaker.sync(&engine, &new, &state.paths) {
                notify(app, &problem);
            }
        }
    }
    if old.hotkeys != new.hotkeys {
        register_hotkeys(app);
    }
    if old.app.autostart != new.app.autostart {
        let launcher = app.autolaunch();
        let res = if new.app.autostart {
            launcher.enable()
        } else {
            launcher.disable()
        };
        if let Err(e) = res {
            notify(app, &format!("自動起動を変更できません: {e}"));
        }
    }
    if old.routing.enabled != new.routing.enabled
        || old.routing.game_sink != new.routing.game_sink
        || old.routing.chat_sink != new.routing.chat_sink
    {
        state.router.tick_now();
    }
    if !from_ui {
        let _ = app.emit("settings", &new);
    }
}

// ---------------------------------------------------------------------------
// オーバーレイ
// ---------------------------------------------------------------------------

fn create_overlay(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html#overlay".into()))
        .title("Clarivo overlay")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .focusable(false)
        .shadow(false)
        .visible(false)
        .inner_size(380.0, 72.0);
    match builder.build() {
        Ok(w) => {
            let _ = w.set_ignore_cursor_events(true);
            if let Ok(Some(m)) = w.primary_monitor() {
                let scale = m.scale_factor();
                let size = m.size();
                let x = (size.width as f64 / scale - 380.0) / 2.0;
                let _ = w.set_position(tauri::LogicalPosition::new(x, 36.0));
            }
        }
        Err(e) => log::warn!("オーバーレイを作れません: {e}"),
    }
}

pub(crate) fn show_overlay(app: &AppHandle, text: &str) {
    let state = app.state::<AppState>();
    if !state.settings.lock().app.overlay {
        return;
    }
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    let gen = state.overlay_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit_to("overlay", "overlay-message", text.to_string());
    let _ = w.show();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1700));
        let state = app.state::<AppState>();
        if state.overlay_generation.load(Ordering::SeqCst) == gen {
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.hide();
            }
        }
    });
}

// ---------------------------------------------------------------------------
// ホットキー
// ---------------------------------------------------------------------------

fn register_hotkeys(app: &AppHandle) {
    let state = app.state::<AppState>();
    let hk = state.settings.lock().hotkeys.clone();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut map = HashMap::new();
    let pairs = [
        (hk.toggle_mic_mute, HotkeyAction::ToggleMicMute),
        (hk.next_preset, HotkeyAction::NextPreset),
        (hk.chatmix_game, HotkeyAction::ChatmixGame),
        (hk.chatmix_chat, HotkeyAction::ChatmixChat),
        (hk.toggle_surround, HotkeyAction::ToggleSurround),
    ];
    for (key, action) in pairs {
        let Some(key) = key.filter(|k| !k.trim().is_empty()) else {
            continue;
        };
        match key.parse::<Shortcut>() {
            Ok(sc) => {
                if let Err(e) = gs.register(sc) {
                    notify(app, &format!("ホットキー「{key}」を登録できません（他のアプリが使っている可能性があります）: {e}"));
                } else {
                    map.insert(sc.id(), action);
                }
            }
            Err(e) => notify(app, &format!("ホットキー「{key}」の書き方が正しくありません: {e}")),
        }
    }
    *state.hotkeys.lock() = map;
}

fn run_hotkey(app: &AppHandle, action: HotkeyAction) {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().clone();
    let message = match action {
        HotkeyAction::ToggleMicMute => {
            s.mic.muted = !s.mic.muted;
            if s.mic.muted { "マイク：ミュート".to_string() } else { "マイク：オン".to_string() }
        }
        HotkeyAction::NextPreset => {
            let mut all = builtin_presets();
            all.extend(s.custom_presets.iter().cloned());
            let idx = all.iter().position(|p| p.id == s.game.preset).map(|i| (i + 1) % all.len()).unwrap_or(0);
            let p = &all[idx];
            s.game.preset = p.id.clone();
            s.game.eq = p.eq.clone();
            s.game.leveler = p.leveler.clone();
            format!("Game：{}", p.name)
        }
        HotkeyAction::ChatmixGame | HotkeyAction::ChatmixChat => {
            let step = if action == HotkeyAction::ChatmixGame { -0.2 } else { 0.2 };
            s.output.chatmix = ((s.output.chatmix + step) * 10.0).round().clamp(-10.0, 10.0) / 10.0;
            chatmix_label(s.output.chatmix)
        }
        HotkeyAction::ToggleSurround => {
            s.game.surround.enabled = !s.game.surround.enabled;
            if s.game.surround.enabled { "サラウンド：オン".to_string() } else { "サラウンド：オフ".to_string() }
        }
    };
    commit_settings(app, s, false);
    show_overlay(app, &message);
}

fn chatmix_label(v: f32) -> String {
    let (g, c) = clarivo_dsp::mixer::chatmix_gains(v);
    format!("ChatMix　ゲーム {:.0}% ・ チャット {:.0}%", g * 100.0, c * 100.0)
}

// ---------------------------------------------------------------------------
// ゲームごとのプロファイル
// ---------------------------------------------------------------------------

fn spawn_profile_loop(app: AppHandle) {
    std::thread::Builder::new()
        .name("clarivo-profiles".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let state = app.state::<AppState>();
            let s = state.settings.lock().clone();
            let mut rt = state.profile.lock();
            // プロファイルのゲームが終了したら元に戻す
            let ended = rt
                .active_pid
                .map(|pid| clarivo_routing::process_exe(pid).is_none())
                .unwrap_or(false);
            if (ended || !s.profiles.auto_switch) && s.profiles.active.is_some() {
                let mut new = s.clone();
                if let Some((game, chatmix)) = rt.base.take() {
                    new.game = game;
                    new.output.chatmix = chatmix;
                }
                new.profiles.active = None;
                rt.active_pid = None;
                drop(rt);
                commit_settings(&app, new, false);
                show_overlay(&app, "通常の設定に戻しました");
                continue;
            }
            if !s.profiles.auto_switch {
                continue;
            }
            let Some((pid, exe)) = clarivo_routing::foreground_process() else {
                continue;
            };
            let Some(p) = clarivo_routing::match_profile(&s.profiles.items, &exe) else {
                continue;
            };
            if s.profiles.active.as_deref() == Some(p.id.as_str()) {
                rt.active_pid = Some(pid);
                continue;
            }
            if rt.base.is_none() {
                rt.base = Some((s.game.clone(), s.output.chatmix));
            }
            rt.active_pid = Some(pid);
            let mut new = s.clone();
            new.game = p.game.clone();
            new.output.chatmix = p.chatmix;
            new.profiles.active = Some(p.id.clone());
            let name = p.name.clone();
            drop(rt);
            commit_settings(&app, new, false);
            show_overlay(&app, &format!("プロファイル：{name}"));
        })
        .expect("プロファイル監視スレッドを開始できません");
}

// ---------------------------------------------------------------------------
// トレイ
// ---------------------------------------------------------------------------

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Clarivo を開く", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "マイクのミュートを切り替え", true, None::<&str>)?;
    let surround = MenuItem::with_id(app, "surround", "サラウンドを切り替え", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &mute, &surround, &sep, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Clarivo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => show_main(app),
            "mute" => run_hotkey(app, HotkeyAction::ToggleMicMute),
            "surround" => run_hotkey(app, HotkeyAction::ToggleSurround),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 起動と終了
// ---------------------------------------------------------------------------

fn shutdown(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.router.shutdown_blocking();
    state.speaker.lock().shutdown();
    state.engine.lock().stop();
    let s = state.settings.lock().clone();
    state::save_settings(&state.paths, &s);
}

/// `clarivo.exe --reset-audio`（アンインストール時）。画面を出さずに後片付けだけする。
pub fn reset_audio_cli() {
    let base = std::env::var("APPDATA").map(std::path::PathBuf::from).unwrap_or_default();
    let paths = Paths::new(base.join("com.xelaods.clarivo"), std::env::temp_dir());
    let mut s = state::load_settings(&paths);
    let _ = clarivo_routing::Router::reset_all(&mut s.routing);
    s.routing.enabled = false;
    state::save_settings(&paths, &s);
}

pub fn run() {
    let minimized_arg = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(
            // %LOCALAPPDATA%\com.xelaods.clarivo\logs\clarivo.log に残す（設定ページから開ける）
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("clarivo".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let action = app
                        .state::<AppState>()
                        .hotkeys
                        .lock()
                        .get(&shortcut.id())
                        .copied();
                    if let Some(a) = action {
                        run_hotkey(app, a);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            log::info!("Clarivo {} を起動します", env!("CARGO_PKG_VERSION"));
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            let paths = Paths::new(config_dir, cache_dir);
            let mut settings = state::load_settings(&paths);
            // 前回プロファイル適用中に終了していたら通常設定のまま始める
            settings.profiles.active = None;

            let mut engine = Engine::new();
            engine.start(&settings);
            let mut speaker = speaker::SpeakerManager::default();
            speaker.reset_tap(&engine);

            let (router_handle, router_rx) = router::channel();
            app.manage(AppState {
                engine: Mutex::new(engine),
                settings: Mutex::new(settings.clone()),
                paths: paths.clone(),
                router: router_handle,
                routing_report: Mutex::new(Default::default()),
                speaker: Mutex::new(speaker),
                hotkeys: Mutex::new(HashMap::new()),
                profile: Mutex::new(ProfileRuntime::default()),
                autoeq_index: Mutex::new(None),
                overlay_generation: AtomicU64::new(0),
            });

            {
                let state = handle.state::<AppState>();
                let mut speaker = state.speaker.lock();
                let engine = state.engine.lock();
                if let Some(p) = speaker.sync(&engine, &settings, &paths) {
                    log::info!("{p}");
                }
            }
            router::spawn(handle.clone(), router_rx);
            register_hotkeys(&handle);
            if let Err(e) = build_tray(&handle) {
                log::warn!("トレイを作れません: {e}");
            }
            create_overlay(&handle);
            spawn_profile_loop(handle.clone());

            if !(minimized_arg || settings.app.start_minimized) {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = app
                    .try_state::<AppState>()
                    .map(|s| s.settings.lock().app.close_to_tray)
                    .unwrap_or(true);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::update_settings,
            commands::get_devices,
            commands::get_meters,
            commands::get_status,
            commands::record_test,
            commands::routing_report,
            commands::set_app_channel,
            commands::autoeq_search,
            commands::autoeq_load,
            commands::parse_eq_text,
            commands::read_text_file,
            commands::download_speaker_model,
            commands::list_speaker_profiles,
            commands::enroll_speaker,
            commands::delete_speaker_profile,
            commands::reset_audio,
            commands::restart_engine,
            commands::save_settings_now,
            commands::foreground_app,
            commands::open_log_dir,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("Clarivo を起動できません")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                shutdown(app);
            }
        });
}

// 未使用警告を避ける（chatmix_label はホットキーとテストで使う）
#[cfg(test)]
mod tests {
    #[test]
    fn chatmix_label_text() {
        assert!(super::chatmix_label(0.0).contains("100%"));
    }
}
