//! Windows（WASAPI 共有モード）の音声入出力。
//!
//! すべてのストリームを 48kHz・32bit 浮動小数で開き、サンプルレートやチャンネル数の
//! 変換は Windows の自動変換（AUTOCONVERTPCM）に任せる。

use super::common::{Flow, MicLinks, MicRunner, OutLinks, OutRunner};
use crate::chains::SpeakerLink;
use crate::devices::{self, Cable, DeviceInfo, DeviceList};
use crate::settings::Settings;
use crate::state::{CableStatus, Shared};
use crate::{MicCommand, OutCommand};
use anyhow::{anyhow, Context, Result};
use clarivo_dsp::drift::DriftResampler;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wasapi::{
    initialize_mta, AudioCaptureClient, AudioClient, AudioRenderClient, Device, DeviceEnumerator,
    Direction, Handle, Role, SampleType, StreamMode, WaveFormat,
};

const RATE: usize = 48_000;

fn com_init() {
    let _ = initialize_mta().ok();
}

/// 音声スレッドを Windows の「Pro Audio」優先度で動かす（MMCSS）。
fn boost_thread() {
    unsafe {
        let mut index = 0u32;
        let _ = windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW(
            windows::core::w!("Pro Audio"),
            &mut index,
        );
    }
}

// ---------------------------------------------------------------------------
// デバイス一覧
// ---------------------------------------------------------------------------

fn collect(en: &DeviceEnumerator, dir: Direction) -> Result<Vec<DeviceInfo>> {
    let default_id = en.get_default_device(&dir).ok().and_then(|d| d.get_id().ok());
    let comm_id = en
        .get_default_device_for_role(&dir, &Role::Communications)
        .ok()
        .and_then(|d| d.get_id().ok());
    let coll = en.get_device_collection(&dir)?;
    let mut out = Vec::new();
    for dev in &coll {
        let Ok(dev) = dev else { continue };
        let Ok(id) = dev.get_id() else { continue };
        let name = dev.get_friendlyname().unwrap_or_else(|_| id.clone());
        out.push(DeviceInfo {
            is_default: default_id.as_deref() == Some(id.as_str()),
            is_default_communications: comm_id.as_deref() == Some(id.as_str()),
            id,
            name,
        });
    }
    Ok(out)
}

pub(crate) fn list_devices() -> Result<DeviceList> {
    com_init();
    let en = DeviceEnumerator::new()?;
    Ok(DeviceList {
        inputs: collect(&en, Direction::Capture)?,
        outputs: collect(&en, Direction::Render)?,
    })
}

fn update_cable_status(shared: &Shared, list: &DeviceList) {
    let st = CableStatus {
        mic: devices::find_cable(&list.outputs, Cable::Main).is_some(),
        game: devices::find_cable(&list.outputs, Cable::A).is_some(),
        chat: devices::find_cable(&list.outputs, Cable::B).is_some(),
    };
    shared.status.lock().cables = st;
}

/// 開いているデバイスの組み合わせ。既定デバイスの変更や VB-CABLE の追加・取り外しを
/// 2 秒おきに調べ、変わっていたら開き直す（Windows の通知を受け取る代わり）。
fn mic_signature(settings: &Settings, list: &DeviceList) -> (Option<String>, Option<String>) {
    (
        devices::pick_microphone(&list.inputs, settings.mic.input_device.as_deref()).map(|d| d.id.clone()),
        devices::pick_cable(&list.outputs, settings.mic.virtual_output.as_deref(), Cable::Main).map(|d| d.id.clone()),
    )
}

type OutSignature = [Option<String>; 4];

fn output_signature(settings: &Settings, list: &DeviceList) -> OutSignature {
    [
        devices::pick_headphones(&list.outputs, settings.output.headphones.as_deref()).map(|d| d.id.clone()),
        devices::pick_cable(&list.inputs, settings.output.game_source.as_deref(), Cable::A).map(|d| d.id.clone()),
        devices::pick_cable(&list.inputs, settings.output.chat_source.as_deref(), Cable::B).map(|d| d.id.clone()),
        if settings.stream.enabled {
            settings
                .stream
                .device
                .as_deref()
                .and_then(|id| list.outputs.iter().find(|d| d.id == id))
                .map(|d| d.id.clone())
        } else {
            None
        },
    ]
}

const DEVICE_CHECK: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// ストリーム
// ---------------------------------------------------------------------------

struct Capture {
    _client: AudioClient,
    cap: AudioCaptureClient,
    event: Option<Handle>,
    channels: usize,
    bytes: Vec<u8>,
    samples: Vec<f32>,
}

impl Capture {
    fn open(dev: &Device, channels: usize, event: bool, buffer_ms: i64) -> Result<Self> {
        let mut client = dev.get_iaudioclient()?;
        let fmt = WaveFormat::new(32, 32, &SampleType::Float, RATE, channels, None);
        let mode = if event {
            StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: buffer_ms * 10_000,
            }
        } else {
            StreamMode::PollingShared {
                autoconvert: true,
                buffer_duration_hns: buffer_ms * 10_000,
            }
        };
        client.initialize_client(&fmt, &Direction::Capture, &mode)?;
        let event = if event {
            Some(client.set_get_eventhandle()?)
        } else {
            None
        };
        let cap = client.get_audiocaptureclient()?;
        let frames = client.get_buffer_size()? as usize;
        client.start_stream()?;
        Ok(Self {
            _client: client,
            cap,
            event,
            channels,
            bytes: vec![0u8; frames.max(4800) * channels * 4],
            samples: Vec::with_capacity(frames.max(4800) * channels),
        })
    }

    /// 届いている全パケットを読み、インターリーブの f32 として返す。
    fn read(&mut self) -> Result<&[f32]> {
        self.samples.clear();
        loop {
            let n = match self.cap.get_next_packet_size()? {
                Some(n) if n > 0 => n as usize,
                _ => break,
            };
            let need = n * self.channels * 4;
            if self.bytes.len() < need {
                self.bytes.resize(need, 0);
            }
            let (frames, info) = self.cap.read_from_device(&mut self.bytes[..need])?;
            let count = frames as usize * self.channels;
            if info.flags.silent {
                self.samples.extend(std::iter::repeat(0.0).take(count));
            } else {
                self.samples.extend(
                    self.bytes[..count * 4]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                );
            }
        }
        Ok(&self.samples)
    }

    fn wait(&self, ms: u32) -> bool {
        match &self.event {
            Some(h) => h.wait_for_event(ms).is_ok(),
            None => {
                std::thread::sleep(Duration::from_millis(ms as u64));
                true
            }
        }
    }
}

struct Render {
    client: AudioClient,
    ren: AudioRenderClient,
    event: Option<Handle>,
    channels: usize,
    bytes: Vec<u8>,
}

impl Render {
    fn open(dev: &Device, channels: usize, event: bool, buffer_ms: i64, prefill: usize) -> Result<Self> {
        let mut client = dev.get_iaudioclient()?;
        let fmt = WaveFormat::new(32, 32, &SampleType::Float, RATE, channels, None);
        let mode = if event {
            StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: buffer_ms * 10_000,
            }
        } else {
            StreamMode::PollingShared {
                autoconvert: true,
                buffer_duration_hns: buffer_ms * 10_000,
            }
        };
        client.initialize_client(&fmt, &Direction::Render, &mode)?;
        let event = if event {
            Some(client.set_get_eventhandle()?)
        } else {
            None
        };
        let ren = client.get_audiorenderclient()?;
        let mut r = Self {
            client,
            ren,
            event,
            channels,
            bytes: Vec::with_capacity(48_000 * channels * 4),
        };
        // 開始前に無音で埋めておく（再生開始直後の音切れを防ぐ）
        let space = r.client.get_available_space_in_frames()? as usize;
        let fill = prefill.min(space);
        if fill > 0 {
            let zeros = vec![0.0f32; fill * channels];
            r.write(&zeros)?;
        }
        r.client.start_stream()?;
        Ok(r)
    }

    fn available(&self) -> Result<usize> {
        Ok(self.client.get_available_space_in_frames()? as usize)
    }

    fn padding(&self) -> Result<usize> {
        Ok(self.client.get_current_padding()? as usize)
    }

    fn write(&mut self, interleaved: &[f32]) -> Result<()> {
        let frames = interleaved.len() / self.channels;
        if frames == 0 {
            return Ok(());
        }
        self.bytes.clear();
        for s in interleaved {
            self.bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.ren.write_to_device(frames, &self.bytes, None)?;
        Ok(())
    }

    fn wait(&self, ms: u32) -> bool {
        match &self.event {
            Some(h) => h.wait_for_event(ms).is_ok(),
            None => {
                std::thread::sleep(Duration::from_millis(ms as u64));
                true
            }
        }
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}

fn open_device(en: &DeviceEnumerator, id: &str) -> Result<Device> {
    en.get_device(id)
        .with_context(|| format!("デバイスを開けません: {id}"))
}

/// コマンドを処理しながら `ms` だけ待つ。停止なら false。
fn idle(rx_try: &mut dyn FnMut() -> Option<Flow>, ms: u64) -> Option<Flow> {
    let steps = (ms / 50).max(1);
    for _ in 0..steps {
        if let Some(f) = rx_try() {
            match f {
                Flow::Continue => {}
                other => return Some(other),
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

// ---------------------------------------------------------------------------
// マイクスレッド
// ---------------------------------------------------------------------------

struct MicSession {
    capture: Capture,
    render: Option<Render>,
    signature: (Option<String>, Option<String>),
    last_check: Instant,
    drift: DriftResampler,
    mono: Vec<f32>,
    stereo: Vec<f32>,
}

/// 仮想マイクの再生バッファに保つ量（30ms）
const MIC_RENDER_TARGET: usize = 1440;

fn open_mic(settings: &Settings, shared: &Shared) -> Result<MicSession> {
    let list = list_devices()?;
    update_cable_status(shared, &list);
    let en = DeviceEnumerator::new()?;
    let mic = devices::pick_microphone(&list.inputs, settings.mic.input_device.as_deref())
        .ok_or_else(|| anyhow!("マイクが見つかりません"))?;
    let capture = Capture::open(&open_device(&en, &mic.id)?, 1, true, 20)
        .with_context(|| format!("マイクを開けません: {}", mic.name))?;
    let mut names = vec![mic.name.clone()];
    let render = match devices::pick_cable(
        &list.outputs,
        settings.mic.virtual_output.as_deref(),
        Cable::Main,
    ) {
        Some(vm) => {
            let r = Render::open(&open_device(&en, &vm.id)?, 2, false, 100, MIC_RENDER_TARGET)
                .with_context(|| format!("仮想マイクを開けません: {}", vm.name))?;
            names.push(vm.name.clone());
            Some(r)
        }
        None => {
            shared.notice(
                "仮想マイク（VB-CABLE）が見つかりません。インストールすると、処理した声を Discord などで使えます",
            );
            None
        }
    };
    {
        let mut st = shared.status.lock();
        st.mic.devices = names;
        st.mic.error = None;
        st.mic.running = true;
    }
    Ok(MicSession {
        signature: mic_signature(settings, &list),
        last_check: Instant::now(),
        capture,
        render,
        drift: DriftResampler::new(1, 480),
        mono: vec![0.0; 48_000],
        stereo: Vec::with_capacity(96_000),
    })
}

fn mic_loop(sess: &mut MicSession, runner: &mut MicRunner, rx: &Receiver<MicCommand>, shared: &Shared) -> Result<Flow> {
    loop {
        while let Ok(cmd) = rx.try_recv() {
            match runner.handle(cmd) {
                Flow::Continue => {}
                other => return Ok(other),
            }
        }
        if sess.last_check.elapsed() >= DEVICE_CHECK {
            sess.last_check = Instant::now();
            if let Ok(list) = list_devices() {
                update_cable_status(shared, &list);
                if mic_signature(&runner.settings, &list) != sess.signature {
                    log::info!("マイクまたは仮想マイクが変わったため開き直します");
                    return Ok(Flow::Reopen);
                }
            }
        }
        if !sess.capture.wait(100) {
            continue;
        }
        let data = sess.capture.read()?;
        runner.push(data);
        let drift = &mut sess.drift;
        runner.process_ready(|frame| drift.push_planar(&[frame]));
        if let Some(r) = sess.render.as_mut() {
            let padding = r.padding()?;
            if padding < MIC_RENDER_TARGET {
                let n = (MIC_RENDER_TARGET - padding).min(sess.mono.len());
                sess.drift.pull(&mut [&mut sess.mono[..n]]);
                sess.stereo.clear();
                for &s in &sess.mono[..n] {
                    sess.stereo.push(s);
                    sess.stereo.push(s);
                }
                r.write(&sess.stereo)?;
            }
        }
        let latency_ms = (runner.chain.latency() + 960 + MIC_RENDER_TARGET + sess.drift.fill()) as f32 / 48.0;
        shared.status.lock().mic.latency_ms = latency_ms;
    }
}

pub(crate) fn run_mic(
    settings: Settings,
    rx: Receiver<MicCommand>,
    links: MicLinks,
    shared: Arc<Shared>,
    speaker: Arc<SpeakerLink>,
) {
    com_init();
    boost_thread();
    let mut runner = MicRunner::new(settings, links, shared.clone(), speaker);
    loop {
        if !runner.settings.mic.enabled {
            shared.status.lock().mic.running = false;
            let flow = idle(
                &mut || rx.try_recv().ok().map(|c| runner.handle(c)),
                200,
            );
            if let Some(Flow::Stop) = flow {
                return;
            }
            continue;
        }
        match open_mic(&runner.settings, &shared) {
            Ok(mut sess) => match mic_loop(&mut sess, &mut runner, &rx, &shared) {
                Ok(Flow::Stop) => break,
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("マイク処理エラー: {e:#}");
                    {
                        let mut st = shared.status.lock();
                        st.mic.error = Some(format!("{e:#}"));
                        st.mic.running = false;
                    }
                    drop(sess);
                    if let Some(Flow::Stop) =
                        idle(&mut || rx.try_recv().ok().map(|c| runner.handle(c)), 1000)
                    {
                        break;
                    }
                }
            },
            Err(e) => {
                {
                    let mut st = shared.status.lock();
                    st.mic.error = Some(format!("{e:#}"));
                    st.mic.running = false;
                }
                if let Some(Flow::Stop) =
                    idle(&mut || rx.try_recv().ok().map(|c| runner.handle(c)), 2000)
                {
                    break;
                }
            }
        }
    }
    shared.status.lock().mic.running = false;
}

// ---------------------------------------------------------------------------
// 出力スレッド
// ---------------------------------------------------------------------------

struct OutSession {
    signature: OutSignature,
    last_check: Instant,
    game: Option<Capture>,
    game_channels: usize,
    chat: Option<Capture>,
    hp: Render,
    stream: Option<Render>,
    buf: Vec<f32>,
    sbuf: Vec<f32>,
}

/// 配信デバイスの再生バッファに保つ量（40ms）
const STREAM_TARGET: usize = 1920;

fn open_output(settings: &Settings, shared: &Shared) -> Result<OutSession> {
    let list = list_devices()?;
    update_cable_status(shared, &list);
    let en = DeviceEnumerator::new()?;
    let hp = devices::pick_headphones(&list.outputs, settings.output.headphones.as_deref())
        .ok_or_else(|| anyhow!("ヘッドホン（再生デバイス）が見つかりません"))?;
    let hp_render = Render::open(&open_device(&en, &hp.id)?, 2, true, 20, 480)
        .with_context(|| format!("ヘッドホンを開けません: {}", hp.name))?;
    let mut names = vec![hp.name.clone()];

    // Game は 7.1ch で取り込む（サラウンド用）。失敗したら 2ch で開き直す
    let (game, game_channels) = match devices::pick_cable(
        &list.inputs,
        settings.output.game_source.as_deref(),
        Cable::A,
    ) {
        Some(d) => {
            let dev = open_device(&en, &d.id)?;
            names.push(d.name.clone());
            match Capture::open(&dev, 8, false, 100) {
                Ok(c) => (Some(c), 8),
                Err(_) => (Some(Capture::open(&dev, 2, false, 100)?), 2),
            }
        }
        None => {
            shared.notice("Game 用の仮想デバイス（VB-CABLE A）が見つかりません");
            (None, 2)
        }
    };
    let chat = match devices::pick_cable(
        &list.inputs,
        settings.output.chat_source.as_deref(),
        Cable::B,
    ) {
        Some(d) => {
            names.push(d.name.clone());
            Some(Capture::open(&open_device(&en, &d.id)?, 2, false, 100)?)
        }
        None => {
            shared.notice("Chat 用の仮想デバイス（VB-CABLE B）が見つかりません");
            None
        }
    };
    let stream = if settings.stream.enabled {
        match settings
            .stream
            .device
            .as_deref()
            .and_then(|id| list.outputs.iter().find(|d| d.id == id))
        {
            Some(d) => {
                names.push(d.name.clone());
                Some(Render::open(&open_device(&en, &d.id)?, 2, false, 100, STREAM_TARGET)?)
            }
            None => {
                shared.notice("配信用ミックスの出力先が選ばれていません");
                None
            }
        }
    } else {
        None
    };
    {
        let mut st = shared.status.lock();
        st.output.devices = names;
        st.output.error = None;
        st.output.running = true;
        st.output.latency_ms = (720.0 + 480.0 + 960.0) / 48.0;
    }
    Ok(OutSession {
        signature: output_signature(settings, &list),
        last_check: Instant::now(),
        game,
        game_channels,
        chat,
        hp: hp_render,
        stream,
        buf: Vec::with_capacity(96_000),
        sbuf: Vec::with_capacity(96_000),
    })
}

fn output_loop(
    sess: &mut OutSession,
    runner: &mut OutRunner,
    rx: &Receiver<OutCommand>,
    shared: &Shared,
) -> Result<Flow> {
    runner.reset_inputs();
    runner.sync_stream();
    loop {
        while let Ok(cmd) = rx.try_recv() {
            match runner.handle(cmd) {
                Flow::Continue => {}
                other => return Ok(other),
            }
        }
        if sess.last_check.elapsed() >= DEVICE_CHECK {
            sess.last_check = Instant::now();
            if let Ok(list) = list_devices() {
                update_cable_status(shared, &list);
                if output_signature(&runner.settings, &list) != sess.signature {
                    log::info!("再生デバイスが変わったため開き直します");
                    return Ok(Flow::Reopen);
                }
            }
        }
        if !sess.hp.wait(100) {
            continue;
        }
        if let Some(g) = sess.game.as_mut() {
            let data = g.read()?;
            runner.push_game(data, sess.game_channels);
        }
        if let Some(c) = sess.chat.as_mut() {
            let data = c.read()?;
            runner.push_chat(data, 2);
        }
        let avail = sess.hp.available()?;
        if avail > 0 {
            runner.render(avail, &mut sess.buf);
            sess.hp.write(&sess.buf)?;
        }
        if let Some(s) = sess.stream.as_mut() {
            let padding = s.padding()?;
            if padding < STREAM_TARGET {
                runner.render_stream(STREAM_TARGET - padding, &mut sess.sbuf);
                s.write(&sess.sbuf)?;
            }
        }
    }
}

pub(crate) fn run_output(
    settings: Settings,
    rx: Receiver<OutCommand>,
    links: OutLinks,
    shared: Arc<Shared>,
) {
    com_init();
    boost_thread();
    let mut runner = OutRunner::new(settings, links, shared.clone());
    loop {
        if !runner.settings.output.enabled {
            shared.status.lock().output.running = false;
            if let Some(Flow::Stop) = idle(&mut || rx.try_recv().ok().map(|c| runner.handle(c)), 200) {
                return;
            }
            continue;
        }
        match open_output(&runner.settings, &shared) {
            Ok(mut sess) => match output_loop(&mut sess, &mut runner, &rx, &shared) {
                Ok(Flow::Stop) => break,
                Ok(_) => continue,
                Err(e) => {
                    log::warn!("出力処理エラー: {e:#}");
                    {
                        let mut st = shared.status.lock();
                        st.output.error = Some(format!("{e:#}"));
                        st.output.running = false;
                    }
                    drop(sess);
                    if let Some(Flow::Stop) =
                        idle(&mut || rx.try_recv().ok().map(|c| runner.handle(c)), 1000)
                    {
                        break;
                    }
                }
            },
            Err(e) => {
                {
                    let mut st = shared.status.lock();
                    st.output.error = Some(format!("{e:#}"));
                    st.output.running = false;
                }
                if let Some(Flow::Stop) =
                    idle(&mut || rx.try_recv().ok().map(|c| runner.handle(c)), 2000)
                {
                    break;
                }
            }
        }
    }
    shared.status.lock().output.running = false;
}
