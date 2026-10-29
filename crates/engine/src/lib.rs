//! Clarivo の音声エンジン。
//!
//! 2 本のスレッドで動く。
//! - マイクスレッド: 実マイク → [`chains::MicChain`] → 仮想マイク（VB-CABLE）
//! - 出力スレッド: 仮想チャンネル（Game・Chat）→ [`chains::OutputMixer`] → ヘッドホン／配信
//!
//! スレッド間は rtrb のロックフリーなリングバッファでつなぐ
//! （マイクのモニター音、エコーキャンセル用の参照音、話者判定用の音声）。

pub mod chains;
pub mod devices;
pub mod settings;
pub mod state;

mod backend;

use chains::{FrameProcessor, SpeakerLink};
use devices::DeviceList;
use parking_lot::Mutex;
use settings::Settings;
use state::Shared;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// 録音テストの結果（48kHz モノラル）。
#[derive(Debug, Clone, Default)]
pub struct Recording {
    /// 処理前（入力ゲインのみ）
    pub raw: Vec<f32>,
    /// 処理後（仮想マイクに出している音）
    pub processed: Vec<f32>,
}

/// 録音の完了待ち。
pub struct PendingRecording {
    rx: Receiver<Recording>,
    seconds: f32,
}

impl PendingRecording {
    pub fn wait(self) -> anyhow::Result<Recording> {
        let wait = Duration::from_secs_f32(self.seconds + 5.0);
        self.rx
            .recv_timeout(wait)
            .map_err(|_| anyhow::anyhow!("録音できませんでした（マイクから音が届いていません）"))
    }
}

pub(crate) enum MicCommand {
    Apply(Box<Settings>),
    Record {
        frames: usize,
        reply: Sender<Recording>,
    },
    SetExtractor(Option<Box<dyn FrameProcessor>>),
    Stop,
}

pub(crate) enum OutCommand {
    Apply(Box<Settings>),
    Stop,
}

/// スレッド間をつなぐリングバッファ一式。
pub(crate) struct Links {
    /// マイク → 出力：処理済みのマイク音（モニター・配信用）
    pub mic_to_out: rtrb::Producer<f32>,
    pub mic_from_mic: rtrb::Consumer<f32>,
    /// 出力 → マイク：ヘッドホンに出した音（エコーキャンセル用）
    pub ref_to_mic: rtrb::Producer<f32>,
    pub ref_from_out: rtrb::Consumer<f32>,
    /// マイク → 話者判定：ノイズ除去後のマイク音
    pub speaker_tap: rtrb::Producer<f32>,
}

struct Pipeline<C> {
    tx: Sender<C>,
    handle: Option<JoinHandle<()>>,
}

pub struct Engine {
    shared: Arc<Shared>,
    speaker: Arc<SpeakerLink>,
    mic: Option<Pipeline<MicCommand>>,
    out: Option<Pipeline<OutCommand>>,
    /// 話者判定スレッドが取り出す口（`take_speaker_tap` で一度だけ渡す）
    speaker_tap: Mutex<Option<rtrb::Consumer<f32>>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
            speaker: Arc::new(SpeakerLink::default()),
            mic: None,
            out: None,
            speaker_tap: Mutex::new(None),
        }
    }

    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn speaker_link(&self) -> Arc<SpeakerLink> {
        self.speaker.clone()
    }

    /// 話者判定スレッド用に、マイク音の取り出し口を受け取る（開始ごとに作り直される）。
    pub fn take_speaker_tap(&self) -> Option<rtrb::Consumer<f32>> {
        self.speaker_tap.lock().take()
    }

    pub fn is_running(&self) -> bool {
        self.mic.is_some() || self.out.is_some()
    }

    /// エンジンを開始する（動いていれば止めてから開始し直す）。
    pub fn start(&mut self, settings: &Settings) {
        self.stop();
        let (mic_to_out, mic_from_mic) = rtrb::RingBuffer::new(48_000);
        let (ref_to_mic, ref_from_out) = rtrb::RingBuffer::new(48_000);
        let (speaker_tap, speaker_rx) = rtrb::RingBuffer::new(48_000 * 2);
        *self.speaker_tap.lock() = Some(speaker_rx);
        let links = Links {
            mic_to_out,
            mic_from_mic,
            ref_to_mic,
            ref_from_out,
            speaker_tap,
        };
        let (mic_tx, mic_rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel();
        let (mic_links, out_links) = backend::split_links(links);
        let s = Box::new(settings.clone());
        let shared = self.shared.clone();
        let speaker = self.speaker.clone();
        let mic_settings = s.clone();
        let mic_handle = std::thread::Builder::new()
            .name("clarivo-mic".into())
            .spawn(move || backend::run_mic(*mic_settings, mic_rx, mic_links, shared, speaker))
            .expect("マイクスレッドを開始できません");
        let shared = self.shared.clone();
        let out_handle = std::thread::Builder::new()
            .name("clarivo-output".into())
            .spawn(move || backend::run_output(*s, out_rx, out_links, shared))
            .expect("出力スレッドを開始できません");
        self.mic = Some(Pipeline {
            tx: mic_tx,
            handle: Some(mic_handle),
        });
        self.out = Some(Pipeline {
            tx: out_tx,
            handle: Some(out_handle),
        });
    }

    /// 設定を反映する。デバイスが変わった場合はスレッド内で開き直す。
    pub fn apply(&self, settings: &Settings) {
        if let Some(m) = &self.mic {
            let _ = m.tx.send(MicCommand::Apply(Box::new(settings.clone())));
        }
        if let Some(o) = &self.out {
            let _ = o.tx.send(OutCommand::Apply(Box::new(settings.clone())));
        }
    }

    /// 話者抽出モデルを差し込む（None で外す）。
    pub fn set_extractor(&self, ex: Option<Box<dyn FrameProcessor>>) {
        if let Some(m) = &self.mic {
            let _ = m.tx.send(MicCommand::SetExtractor(ex));
        }
    }

    /// マイクの録音（処理前と処理後）を始める。結果は返り値の受け口に届く。
    /// 呼び出し側はエンジンのロックを外してから待てるので、録音中も設定を変えられる。
    pub fn start_record(&self, seconds: f32) -> anyhow::Result<PendingRecording> {
        let Some(m) = &self.mic else {
            anyhow::bail!("エンジンが動いていません");
        };
        let seconds = seconds.clamp(0.5, 60.0);
        let frames = (seconds * 48_000.0) as usize;
        let (tx, rx): (Sender<Recording>, Receiver<Recording>) = mpsc::channel();
        m.tx.send(MicCommand::Record { frames, reply: tx })
            .map_err(|_| anyhow::anyhow!("マイクスレッドが止まっています"))?;
        Ok(PendingRecording { rx, seconds })
    }

    /// マイクを `seconds` 秒録音する。終わるまで待つ。
    pub fn record(&self, seconds: f32) -> anyhow::Result<Recording> {
        self.start_record(seconds)?.wait()
    }

    pub fn stop(&mut self) {
        if let Some(mut m) = self.mic.take() {
            let _ = m.tx.send(MicCommand::Stop);
            if let Some(h) = m.handle.take() {
                let _ = h.join();
            }
        }
        if let Some(mut o) = self.out.take() {
            let _ = o.tx.send(OutCommand::Stop);
            if let Some(h) = o.handle.take() {
                let _ = h.join();
            }
        }
        let mut st = self.shared.status.lock();
        st.mic.running = false;
        st.output.running = false;
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 接続されている音声デバイスの一覧。
pub fn list_devices() -> anyhow::Result<DeviceList> {
    backend::list_devices()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実デバイスが無い環境（null バックエンド）で、開始・設定反映・録音・停止が通ること。
    #[cfg(not(windows))]
    #[test]
    fn engine_lifecycle_on_null_backend() {
        let mut e = Engine::new();
        let mut s = Settings::default();
        s.mic.denoise = clarivo_denoise::DenoiseMode::Light;
        e.start(&s);
        assert!(e.is_running());
        assert!(e.take_speaker_tap().is_some());
        s.mic.muted = true;
        e.apply(&s);
        let rec = e.record(0.5).unwrap();
        assert_eq!(rec.raw.len(), 24_000);
        assert_eq!(rec.processed.len(), 24_000);
        e.stop();
        assert!(!e.is_running());
    }
}
