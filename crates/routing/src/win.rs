//! Windows の実装。

#![allow(non_snake_case)]

use super::{AppSession, RoutingReport};
use clarivo_engine::devices::{self, Cable, DeviceInfo};
use clarivo_engine::settings::{ChannelId, RoutingSettings};
use std::collections::HashMap;
use std::ffi::c_void;
use wasapi::{DeviceEnumerator, Direction, SessionState};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{eCommunications, eConsole, eMultimedia, ERole};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::WinRT::RoGetActivationFactory;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows_core::{interface, IUnknown, Interface, GUID, HRESULT, HSTRING, PCWSTR, PWSTR};

// ---------------------------------------------------------------------------
// 非公開 COM インターフェース
// ---------------------------------------------------------------------------

/// 既定デバイスの切り替え（Windows 7 以降）。
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, a: *mut c_void, b: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, a: *mut i64, b: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, a: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

macro_rules! audio_policy_factory {
    ($name:ident, $iid:literal) => {
        /// アプリ別の既定デバイス（Windows.Media.Internal.AudioPolicyConfig）。
        /// 先頭 3 つは IInspectable、続く 19 個は使わないので仮の宣言。
        #[interface($iid)]
        unsafe trait $name: IUnknown {
            fn GetIids(&self) -> HRESULT;
            fn GetRuntimeClassName(&self) -> HRESULT;
            fn GetTrustLevel(&self) -> HRESULT;
            fn _r01(&self) -> HRESULT;
            fn _r02(&self) -> HRESULT;
            fn _r03(&self) -> HRESULT;
            fn _r04(&self) -> HRESULT;
            fn _r05(&self) -> HRESULT;
            fn _r06(&self) -> HRESULT;
            fn _r07(&self) -> HRESULT;
            fn _r08(&self) -> HRESULT;
            fn _r09(&self) -> HRESULT;
            fn _r10(&self) -> HRESULT;
            fn _r11(&self) -> HRESULT;
            fn _r12(&self) -> HRESULT;
            fn _r13(&self) -> HRESULT;
            fn _r14(&self) -> HRESULT;
            fn _r15(&self) -> HRESULT;
            fn _r16(&self) -> HRESULT;
            fn _r17(&self) -> HRESULT;
            fn _r18(&self) -> HRESULT;
            fn _r19(&self) -> HRESULT;
            fn SetPersistedDefaultAudioEndpoint(
                &self,
                process_id: u32,
                flow: i32,
                role: i32,
                device_id: *mut c_void,
            ) -> HRESULT;
            fn GetPersistedDefaultAudioEndpoint(
                &self,
                process_id: u32,
                flow: i32,
                role: i32,
                device_id: *mut *mut c_void,
            ) -> HRESULT;
            fn ClearAllPersistedApplicationDefaultEndpoints(&self) -> HRESULT;
        }
    };
}

// Windows 10 21H2 以降
audio_policy_factory!(IAudioPolicyConfigFactory, "ab3d4648-e242-459f-b02f-541c70306324");
// それより前の Windows 10
audio_policy_factory!(IAudioPolicyConfigFactoryLegacy, "2a59116d-6c4f-45e0-a74f-707e3fef9258");

enum PolicyFactory {
    Modern(IAudioPolicyConfigFactory),
    Legacy(IAudioPolicyConfigFactoryLegacy),
}

impl PolicyFactory {
    fn create() -> windows_core::Result<Self> {
        let class = HSTRING::from("Windows.Media.Internal.AudioPolicyConfig");
        unsafe {
            match RoGetActivationFactory::<IAudioPolicyConfigFactory>(&class) {
                Ok(f) => Ok(Self::Modern(f)),
                Err(_) => RoGetActivationFactory::<IAudioPolicyConfigFactoryLegacy>(&class)
                    .map(Self::Legacy),
            }
        }
    }

    /// `endpoint_id` が None のときは設定を消す（既定に戻す）。
    fn set(&self, pid: u32, role: ERole, endpoint_id: Option<&str>) -> windows_core::Result<()> {
        let h = endpoint_id.map(|id| HSTRING::from(persisted_id(id)));
        let raw: *mut c_void = match &h {
            Some(h) => unsafe { std::mem::transmute_copy(h) },
            None => std::ptr::null_mut(),
        };
        let flow = 0; // eRender
        unsafe {
            match self {
                Self::Modern(f) => f.SetPersistedDefaultAudioEndpoint(pid, flow, role.0, raw).ok(),
                Self::Legacy(f) => f.SetPersistedDefaultAudioEndpoint(pid, flow, role.0, raw).ok(),
            }
        }
    }

    fn clear_all(&self) -> windows_core::Result<()> {
        unsafe {
            match self {
                Self::Modern(f) => f.ClearAllPersistedApplicationDefaultEndpoints().ok(),
                Self::Legacy(f) => f.ClearAllPersistedApplicationDefaultEndpoints().ok(),
            }
        }
    }
}

/// MMDevice の ID を、AudioPolicyConfig が受け付ける形式にする。
fn persisted_id(id: &str) -> String {
    format!(r"\\?\SWD#MMDEVAPI#{id}#{{e6327cad-dcec-4949-ae8a-991e976a79d2}}")
}

fn set_default_endpoint(id: &str, roles: &[ERole]) -> windows_core::Result<()> {
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        for role in roles {
            pc.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), *role).ok()?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// プロセス名
// ---------------------------------------------------------------------------

pub(crate) fn exe_name(pid: u32) -> Option<String> {
    unsafe {
        let h: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/'])
            .next()
            .map(|s| s.to_ascii_lowercase())
    }
}

/// 前面にあるウィンドウのプロセス（pid, 実行ファイル名）。
pub fn foreground_process() -> Option<(u32, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        exe_name(pid).map(|e| (pid, e))
    }
}

/// プロセスが動いていれば実行ファイル名を返す。
pub fn process_exe(pid: u32) -> Option<String> {
    exe_name(pid)
}

// ---------------------------------------------------------------------------
// 振り分け
// ---------------------------------------------------------------------------

fn com_init() {
    let _ = wasapi::initialize_mta().ok();
}

pub struct Router {
    factory: Option<PolicyFactory>,
    factory_error: Option<String>,
    /// pid → 振り分けた先のデバイス ID
    applied: HashMap<u32, String>,
    defaults_applied: bool,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    pub fn new() -> Self {
        com_init();
        let (factory, factory_error) = match PolicyFactory::create() {
            Ok(f) => (Some(f), None),
            Err(e) => (
                None,
                Some(format!("アプリ別の振り分けに対応していない Windows です（{e}）")),
            ),
        };
        Self {
            factory,
            factory_error,
            applied: HashMap::new(),
            defaults_applied: false,
        }
    }

    fn sinks(list: &[DeviceInfo], s: &RoutingSettings) -> (Option<DeviceInfo>, Option<DeviceInfo>) {
        let game = devices::pick_cable(list, s.game_sink.as_deref(), Cable::A).cloned();
        let chat = devices::pick_cable(list, s.chat_sink.as_deref(), Cable::B).cloned();
        (game, chat)
    }

    /// 1 回分の振り分けを行い、音を出しているアプリの一覧を返す（2 秒おきに呼ぶ想定）。
    pub fn tick(&mut self, s: &mut RoutingSettings) -> RoutingReport {
        com_init();
        let mut report = RoutingReport {
            available: self.factory.is_some(),
            error: self.factory_error.clone(),
            ..Default::default()
        };
        let list = match clarivo_engine::list_devices() {
            Ok(l) => l.outputs,
            Err(e) => {
                report.error = Some(format!("{e:#}"));
                return report;
            }
        };
        let (game, chat) = Self::sinks(&list, s);
        if s.enabled {
            self.apply_defaults(s, &list, game.as_ref(), chat.as_ref());
        }
        let own_pid = std::process::id();
        let Ok(en) = DeviceEnumerator::new() else {
            return report;
        };
        for dev in &list {
            // 仮想マイク用の線には Clarivo 以外は出さないので見ない
            if devices::cable_of(&dev.name) == Some(Cable::Main) {
                continue;
            }
            let Ok(device) = en.get_device(&dev.id) else { continue };
            let Ok(mgr) = device.get_iaudiosessionmanager() else { continue };
            let Ok(sessions) = mgr.get_audiosessionenumerator() else { continue };
            let count = sessions.get_count().unwrap_or(0);
            for i in 0..count {
                let Ok(ctl) = sessions.get_session(i) else { continue };
                let Ok(pid) = ctl.get_process_id() else { continue };
                if pid == 0 || pid == own_pid {
                    continue;
                }
                let state = ctl.get_state().ok();
                if matches!(state, Some(SessionState::Expired)) {
                    continue;
                }
                let Some(exe) = exe_name(pid) else { continue };
                let channel = s.channel_for(&exe);
                let target = match channel {
                    ChannelId::Game => game.as_ref(),
                    ChannelId::Chat => chat.as_ref(),
                };
                let routed = target.map(|t| t.id == dev.id).unwrap_or(false);
                if s.enabled && !routed {
                    if let (Some(t), Some(f)) = (target, self.factory.as_ref()) {
                        if self.applied.get(&pid) != Some(&t.id) {
                            let roles: &[ERole] = match channel {
                                ChannelId::Game => &[eConsole, eMultimedia],
                                ChannelId::Chat => &[eConsole, eMultimedia, eCommunications],
                            };
                            let mut ok = true;
                            for r in roles {
                                if let Err(e) = f.set(pid, *r, Some(&t.id)) {
                                    log::warn!("振り分けに失敗 {exe}: {e}");
                                    ok = false;
                                }
                            }
                            if ok {
                                self.applied.insert(pid, t.id.clone());
                            }
                        }
                    }
                }
                let name = ctl
                    .get_display_name()
                    .ok()
                    .filter(|n| !n.is_empty() && !n.starts_with('@'))
                    .unwrap_or_else(|| exe.trim_end_matches(".exe").to_string());
                report.sessions.push(AppSession {
                    pid,
                    exe,
                    name,
                    channel,
                    device: dev.name.clone(),
                    routed,
                    active: matches!(state, Some(SessionState::Active)),
                });
            }
        }
        report.sessions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        report.sessions.dedup_by(|a, b| a.pid == b.pid && a.device == b.device);
        report
    }

    /// Windows の既定デバイスを Game / Chat に切り替える（元の設定は保存しておく）。
    fn apply_defaults(
        &mut self,
        s: &mut RoutingSettings,
        list: &[DeviceInfo],
        game: Option<&DeviceInfo>,
        chat: Option<&DeviceInfo>,
    ) {
        if !s.set_default_devices || self.defaults_applied {
            return;
        }
        let current_console = list.iter().find(|d| d.is_default);
        let current_comm = list.iter().find(|d| d.is_default_communications);
        if s.saved_default_console.is_none() {
            if let Some(d) = current_console.filter(|d| !devices::is_virtual(&d.name)) {
                s.saved_default_console = Some(d.id.clone());
            }
        }
        if s.saved_default_communications.is_none() {
            if let Some(d) = current_comm.filter(|d| !devices::is_virtual(&d.name)) {
                s.saved_default_communications = Some(d.id.clone());
            }
        }
        if let Some(g) = game {
            if let Err(e) = set_default_endpoint(&g.id, &[eConsole, eMultimedia]) {
                log::warn!("既定の再生デバイスを変更できません: {e}");
            }
        }
        if let Some(c) = chat {
            if let Err(e) = set_default_endpoint(&c.id, &[eCommunications]) {
                log::warn!("既定の通信デバイスを変更できません: {e}");
            }
        }
        self.defaults_applied = true;
    }

    /// 振り分けを解除し、既定デバイスを元に戻す（アプリ終了時・振り分けを切ったとき）。
    pub fn restore(&mut self, s: &mut RoutingSettings) {
        com_init();
        if let Some(f) = self.factory.as_ref() {
            for pid in self.applied.keys() {
                for r in [eConsole, eMultimedia, eCommunications] {
                    let _ = f.set(*pid, r, None);
                }
            }
        }
        self.applied.clear();
        restore_defaults(s);
        self.defaults_applied = false;
    }

    /// すべてのアプリの振り分けを消し、既定デバイスを戻す（--reset-audio、アンインストール時）。
    pub fn reset_all(s: &mut RoutingSettings) -> anyhow::Result<()> {
        com_init();
        if let Ok(f) = PolicyFactory::create() {
            f.clear_all()?;
        }
        restore_defaults(s);
        Ok(())
    }
}

fn restore_defaults(s: &mut RoutingSettings) {
    let list = clarivo_engine::list_devices().map(|l| l.outputs).unwrap_or_default();
    let fallback = devices::pick_headphones(&list, None).map(|d| d.id.clone());
    let console = s
        .saved_default_console
        .take()
        .filter(|id| list.iter().any(|d| &d.id == id))
        .or_else(|| fallback.clone());
    let comm = s
        .saved_default_communications
        .take()
        .filter(|id| list.iter().any(|d| &d.id == id))
        .or(fallback);
    // 今の既定が仮想ケーブルのときだけ戻す（ユーザーが自分で変えていたら触らない）
    let current_virtual = |pred: fn(&DeviceInfo) -> bool| {
        list.iter()
            .find(|d| pred(d))
            .map(|d| devices::is_virtual(&d.name))
            .unwrap_or(false)
    };
    if let Some(id) = console {
        if current_virtual(|d| d.is_default) {
            let _ = set_default_endpoint(&id, &[eConsole, eMultimedia]);
        }
    }
    if let Some(id) = comm {
        if current_virtual(|d| d.is_default_communications) {
            let _ = set_default_endpoint(&id, &[eCommunications]);
        }
    }
}

// Interface トレイトを使っていることをコンパイラに伝える（IID 参照用）
#[allow(dead_code)]
fn _iids() -> [GUID; 2] {
    [IPolicyConfig::IID, IAudioPolicyConfigFactory::IID]
}

#[allow(dead_code)]
fn _direction() -> Direction {
    Direction::Render
}
