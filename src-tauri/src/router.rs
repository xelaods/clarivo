//! アプリ別の振り分けを専用スレッドで動かす。
//!
//! Windows の COM オブジェクトは作ったスレッドで使うのが安全なので、
//! Router はこのスレッドの中だけで扱い、外からはメッセージで頼む。

use crate::state::AppState;
use clarivo_engine::settings::RoutingSettings;
use clarivo_routing::{RoutingReport, Router};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub enum RouterCmd {
    /// 振り分けを解除して既定デバイスを戻す
    Restore(Sender<()>),
    /// すぐに 1 回振り分ける
    TickNow,
    Shutdown(Sender<()>),
}

#[derive(Clone)]
pub struct RouterHandle {
    tx: Sender<RouterCmd>,
}

impl RouterHandle {
    pub fn restore_blocking(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(RouterCmd::Restore(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }

    pub fn tick_now(&self) {
        let _ = self.tx.send(RouterCmd::TickNow);
    }

    pub fn shutdown_blocking(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(RouterCmd::Shutdown(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }
}

pub fn channel() -> (RouterHandle, Receiver<RouterCmd>) {
    let (tx, rx) = mpsc::channel();
    (RouterHandle { tx }, rx)
}

/// 振り分けスレッドを開始する。
pub fn spawn(app: AppHandle, rx: Receiver<RouterCmd>) {
    std::thread::Builder::new()
        .name("clarivo-router".into())
        .spawn(move || {
            let mut router = Router::new();
            let mut was_enabled = false;
            loop {
                let cmd = rx.recv_timeout(Duration::from_secs(2));
                let state = app.state::<AppState>();
                match cmd {
                    Ok(RouterCmd::Restore(reply)) => {
                        with_routing(&state, |r| router.restore(r));
                        was_enabled = false;
                        let _ = reply.send(());
                        continue;
                    }
                    Ok(RouterCmd::Shutdown(reply)) => {
                        with_routing(&state, |r| router.restore(r));
                        let _ = reply.send(());
                        break;
                    }
                    Ok(RouterCmd::TickNow) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                let enabled = state.settings.lock().routing.enabled;
                if was_enabled && !enabled {
                    with_routing(&state, |r| router.restore(r));
                }
                was_enabled = enabled;
                let mut report = RoutingReport::default();
                with_routing(&state, |r| report = router.tick(r));
                *state.routing_report.lock() = report.clone();
                let _ = app.emit("routing", report);
            }
        })
        .expect("振り分けスレッドを開始できません");
}

/// 振り分けの設定を一時的に取り出して処理し、保存が必要な項目だけを書き戻す。
fn with_routing(state: &AppState, f: impl FnOnce(&mut RoutingSettings)) {
    let mut routing = state.settings.lock().routing.clone();
    let before = (
        routing.saved_default_console.clone(),
        routing.saved_default_communications.clone(),
    );
    f(&mut routing);
    let after = (
        routing.saved_default_console.clone(),
        routing.saved_default_communications.clone(),
    );
    if before != after {
        let mut s = state.settings.lock();
        s.routing.saved_default_console = after.0;
        s.routing.saved_default_communications = after.1;
        crate::state::save_settings(&state.paths, &s);
    }
}
