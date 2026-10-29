//! バックエンド共通の処理（フレームの組み立て、録音、スレッド間のやり取り）。

use crate::chains::{MicChain, MixInput, OutputMixer, SpeakerLink};
use crate::settings::Settings;
use crate::state::Shared;
use crate::{Links, MicCommand, OutCommand, Recording};
use clarivo_dsp::drift::DriftResampler;
use clarivo_dsp::BLOCK;
use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

pub(crate) struct MicLinks {
    pub mic_to_out: rtrb::Producer<f32>,
    pub ref_from_out: rtrb::Consumer<f32>,
    pub speaker_tap: Option<rtrb::Producer<f32>>,
}

pub(crate) struct OutLinks {
    pub mic_from_mic: rtrb::Consumer<f32>,
    pub ref_to_mic: rtrb::Producer<f32>,
}

pub(crate) fn split_links(l: Links) -> (MicLinks, OutLinks) {
    (
        MicLinks {
            mic_to_out: l.mic_to_out,
            ref_from_out: l.ref_from_out,
            speaker_tap: Some(l.speaker_tap),
        },
        OutLinks {
            mic_from_mic: l.mic_from_mic,
            ref_to_mic: l.ref_to_mic,
        },
    )
}

/// デバイスの再オープンが必要な変更があったか。
pub(crate) fn mic_devices_changed(a: &Settings, b: &Settings) -> bool {
    a.mic.input_device != b.mic.input_device
        || a.mic.virtual_output != b.mic.virtual_output
        || a.mic.enabled != b.mic.enabled
}

pub(crate) fn output_devices_changed(a: &Settings, b: &Settings) -> bool {
    a.output.headphones != b.output.headphones
        || a.output.game_source != b.output.game_source
        || a.output.chat_source != b.output.chat_source
        || a.output.enabled != b.output.enabled
        || a.stream.enabled != b.stream.enabled
        || a.stream.device != b.stream.device
}

pub(crate) enum Flow {
    Continue,
    /// デバイスを開き直す
    Reopen,
    Stop,
}

struct ActiveRecording {
    raw: Vec<f32>,
    processed: Vec<f32>,
    frames: usize,
    reply: Sender<Recording>,
}

/// マイク側の処理（デバイスから届いたサンプルを 10ms 単位で処理する）。
pub(crate) struct MicRunner {
    pub chain: MicChain,
    pub settings: Settings,
    links: MicLinks,
    shared: Arc<Shared>,
    pending: VecDeque<f32>,
    frame: Vec<f32>,
    raw: Vec<f32>,
    echo: Vec<f32>,
    recording: Option<ActiveRecording>,
    load: f32,
}

impl MicRunner {
    pub fn new(
        settings: Settings,
        mut links: MicLinks,
        shared: Arc<Shared>,
        speaker: Arc<SpeakerLink>,
    ) -> Self {
        let mut chain = MicChain::new(&settings.mic, speaker);
        chain.set_tap(links.speaker_tap.take());
        if let Some(n) = chain.notice.take() {
            shared.notice(n);
        }
        Self {
            chain,
            settings,
            links,
            shared,
            pending: VecDeque::with_capacity(BLOCK * 16),
            frame: vec![0.0; BLOCK],
            raw: vec![0.0; BLOCK],
            echo: vec![0.0; BLOCK],
            recording: None,
            load: 0.0,
        }
    }

    pub fn handle(&mut self, cmd: MicCommand) -> Flow {
        match cmd {
            MicCommand::Apply(s) => {
                let reopen = mic_devices_changed(&self.settings, &s);
                self.chain.apply(&s.mic);
                if let Some(n) = self.chain.notice.take() {
                    self.shared.notice(n);
                }
                self.settings = *s;
                if reopen {
                    Flow::Reopen
                } else {
                    Flow::Continue
                }
            }
            MicCommand::Record { frames, reply } => {
                self.recording = Some(ActiveRecording {
                    raw: Vec::with_capacity(frames),
                    processed: Vec::with_capacity(frames),
                    frames,
                    reply,
                });
                Flow::Continue
            }
            MicCommand::SetExtractor(ex) => {
                self.chain.set_extractor(ex);
                Flow::Continue
            }
            MicCommand::Stop => Flow::Stop,
        }
    }

    /// 取り込んだモノラルのサンプルを追加する。
    pub fn push(&mut self, samples: &[f32]) {
        self.pending.extend(samples.iter().copied());
        // 処理が大きく遅れた場合は古いものを捨てる（遅延が溜まり続けないように）
        let max = BLOCK * 20;
        if self.pending.len() > max {
            let drop = self.pending.len() - BLOCK * 2;
            self.pending.drain(..drop);
        }
    }

    /// 溜まったサンプルを 10ms ずつ処理し、処理後のフレームを `sink` に渡す。
    pub fn process_ready(&mut self, mut sink: impl FnMut(&[f32])) {
        while self.pending.len() >= BLOCK {
            let t0 = Instant::now();
            for (d, s) in self.frame.iter_mut().zip(self.pending.drain(..BLOCK)) {
                *d = s;
            }
            if self.recording.is_some() {
                self.raw.copy_from_slice(&self.frame);
            }
            // エコーキャンセルの参照音。使わないときは溜めずに捨てる
            let echo_ref = if self.chain.wants_echo_reference()
                && self.links.ref_from_out.slots() >= BLOCK
            {
                for d in self.echo.iter_mut() {
                    *d = self.links.ref_from_out.pop().unwrap_or(0.0);
                }
                // 遅れて溜まりすぎていたら追いつく
                while self.links.ref_from_out.slots() > BLOCK * 10 {
                    let _ = self.links.ref_from_out.pop();
                }
                Some(&self.echo[..])
            } else {
                if !self.chain.wants_echo_reference() {
                    while self.links.ref_from_out.pop().is_ok() {}
                }
                None
            };
            self.chain.process(&mut self.frame, echo_ref, &self.shared.meters);
            if self.links.mic_to_out.slots() >= BLOCK {
                for &s in self.frame.iter() {
                    let _ = self.links.mic_to_out.push(s);
                }
            }
            if let Some(rec) = self.recording.as_mut() {
                rec.raw.extend_from_slice(&self.raw);
                rec.processed.extend_from_slice(&self.frame);
                if rec.raw.len() >= rec.frames {
                    let mut rec = self.recording.take().unwrap();
                    rec.raw.truncate(rec.frames);
                    rec.processed.truncate(rec.frames);
                    let _ = rec.reply.send(Recording {
                        raw: rec.raw,
                        processed: rec.processed,
                    });
                }
            }
            sink(&self.frame);
            let used = t0.elapsed().as_secs_f32() / 0.01;
            self.load = self.load * 0.95 + used * 0.05;
            self.shared.meters.mic_load.store(self.load);
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
/// 出力側の処理。Game・Chat・モニター音をそれぞれのずれ補正を通して 10ms ずつ混ぜる。
pub(crate) struct OutRunner {
    pub mixer: OutputMixer,
    pub settings: Settings,
    links: OutLinks,
    shared: Arc<Shared>,
    game: DriftResampler,
    chat: DriftResampler,
    monitor: DriftResampler,
    /// ヘッドホンへ書き出す前の 2ch インターリーブ
    out_fifo: VecDeque<f32>,
    /// 配信用（配信デバイス側のずれ補正）
    pub stream: Option<DriftResampler>,
    bufs: Vec<Vec<f32>>,
    mic_tmp: Vec<f32>,
    stream_l: Vec<f32>,
    stream_r: Vec<f32>,
    load: f32,
}

/// 各入力の目標バッファ量（15ms）
const INPUT_TARGET: usize = 720;

#[cfg_attr(not(windows), allow(dead_code))]
impl OutRunner {
    pub fn new(settings: Settings, links: OutLinks, shared: Arc<Shared>) -> Self {
        let mixer = OutputMixer::new(
            &settings.output,
            &settings.game,
            &settings.chat,
            &settings.stream,
            settings.mic.monitor,
            settings.mic.monitor_db,
        );
        let stream = settings
            .stream
            .enabled
            .then(|| DriftResampler::new(2, INPUT_TARGET * 2));
        Self {
            mixer,
            settings,
            links,
            shared,
            game: DriftResampler::new(8, INPUT_TARGET),
            chat: DriftResampler::new(2, INPUT_TARGET),
            monitor: DriftResampler::new(1, INPUT_TARGET),
            out_fifo: VecDeque::with_capacity(BLOCK * 16),
            stream,
            // game 8 + chat 2 + mic 1 + out 2 + stream 2
            bufs: (0..15).map(|_| vec![0.0; BLOCK]).collect(),
            mic_tmp: Vec::with_capacity(48_000),
            stream_l: vec![0.0; 48_000],
            stream_r: vec![0.0; 48_000],
            load: 0.0,
        }
    }

    pub fn handle(&mut self, cmd: OutCommand) -> Flow {
        match cmd {
            OutCommand::Apply(s) => {
                let reopen = output_devices_changed(&self.settings, &s);
                self.mixer.apply(
                    &s.output,
                    &s.game,
                    &s.chat,
                    &s.stream,
                    s.mic.monitor,
                    s.mic.monitor_db,
                );
                self.settings = *s;
                if reopen {
                    Flow::Reopen
                } else {
                    Flow::Continue
                }
            }
            OutCommand::Stop => Flow::Stop,
        }
    }

    pub fn push_game(&mut self, interleaved: &[f32], channels: usize) {
        self.game.push_interleaved(interleaved, channels);
    }

    pub fn push_chat(&mut self, interleaved: &[f32], channels: usize) {
        self.chat.push_interleaved(interleaved, channels);
    }

    /// 入力の無いチャンネル（デバイス未接続）は無音で埋める。
    pub fn reset_inputs(&mut self) {
        self.game.clear();
        self.chat.clear();
        self.monitor.clear();
    }

    /// ヘッドホン用に `frames` フレーム分の 2ch インターリーブを `out` に書く。
    pub fn render(&mut self, frames: usize, out: &mut Vec<f32>) {
        // マイクスレッドからのモニター音を受け取る
        self.mic_tmp.clear();
        while let Ok(s) = self.links.mic_from_mic.pop() {
            self.mic_tmp.push(s);
        }
        if !self.mic_tmp.is_empty() {
            self.monitor.push_planar(&[&self.mic_tmp]);
        }

        while self.out_fifo.len() < frames * 2 {
            let t0 = Instant::now();
            let (game_b, rest) = self.bufs.split_at_mut(8);
            let (chat_b, rest) = rest.split_at_mut(2);
            let (mic_b, rest) = rest.split_at_mut(1);
            let (out_b, stream_b) = rest.split_at_mut(2);
            if let [a, b, c, d, e, f, g, h] = game_b {
                self.game.pull(&mut [
                    &mut a[..], &mut b[..], &mut c[..], &mut d[..], &mut e[..], &mut f[..],
                    &mut g[..], &mut h[..],
                ]);
            }
            if let [l, r] = chat_b {
                self.chat.pull(&mut [&mut l[..], &mut r[..]]);
            }
            self.monitor.pull(&mut [&mut mic_b[0][..]]);
            let input = MixInput {
                game: [
                    &game_b[0], &game_b[1], &game_b[2], &game_b[3], &game_b[4], &game_b[5],
                    &game_b[6], &game_b[7],
                ],
                chat: [&chat_b[0], &chat_b[1]],
                mic: &mic_b[0],
            };
            let (ol, or) = out_b.split_at_mut(1);
            let stream_active = self.stream.is_some();
            let (sl, sr) = stream_b.split_at_mut(1);
            let stream_out = if stream_active {
                Some((&mut sl[0][..], &mut sr[0][..]))
            } else {
                None
            };
            self.mixer
                .process(&input, &mut ol[0], &mut or[0], stream_out, &self.shared.meters);
            for i in 0..BLOCK {
                self.out_fifo.push_back(ol[0][i]);
                self.out_fifo.push_back(or[0][i]);
            }
            // エコーキャンセル用にモノラルで送る（マイクスレッドが使わなければ捨てられる）
            if self.links.ref_to_mic.slots() >= BLOCK {
                for i in 0..BLOCK {
                    let _ = self.links.ref_to_mic.push(0.5 * (ol[0][i] + or[0][i]));
                }
            }
            if let Some(st) = self.stream.as_mut() {
                st.push_planar(&[&sl[0], &sr[0]]);
            }
            let used = t0.elapsed().as_secs_f32() / 0.01;
            self.load = self.load * 0.95 + used * 0.05;
            self.shared.meters.output_load.store(self.load);
        }
        out.clear();
        out.extend(self.out_fifo.drain(..frames * 2));
    }

    /// 配信デバイス用に `frames` フレーム分を書く。
    pub fn render_stream(&mut self, frames: usize, out: &mut Vec<f32>) {
        out.clear();
        out.resize(frames * 2, 0.0);
        if let Some(st) = self.stream.as_mut() {
            let frames = frames.min(self.stream_l.len());
            let (l, r) = (&mut self.stream_l[..frames], &mut self.stream_r[..frames]);
            st.pull(&mut [&mut *l, &mut *r]);
            for i in 0..frames {
                out[i * 2] = l[i];
                out[i * 2 + 1] = r[i];
            }
        }
    }

    /// 配信ミックスの有効・無効が変わったときに呼ぶ。
    pub fn sync_stream(&mut self) {
        if self.settings.stream.enabled && self.stream.is_none() {
            self.stream = Some(DriftResampler::new(2, INPUT_TARGET * 2));
        } else if !self.settings.stream.enabled {
            self.stream = None;
        }
    }
}
