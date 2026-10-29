//! 処理の中身（OS に依存しない）。Windows 側のスレッドからフレーム単位で呼ばれる。
//!
//! - [`MicChain`]: マイク → 仮想マイク（フェーズ1・4・5）
//! - [`OutputMixer`]: Game・Chat・モニター → ヘッドホン／配信（フェーズ2・3・5）

use crate::settings::{ChannelSettings, MicSettings, OutputSettings, SpeakerMode, StreamSettings};
use crate::state::Meters;
use clarivo_denoise::{DenoiseMode, Denoiser, Rnnoise};
use clarivo_dsp::biquad::{Biquad, Coefs, FilterKind};
use clarivo_dsp::dynamics::{Compressor, Gate, Leveler, Limiter};
use clarivo_dsp::mixer::{chatmix_gains, downmix_71_to_stereo, mix_into};
use clarivo_dsp::peq::Peq;
use clarivo_dsp::smoother::LinearSmoother;
use clarivo_dsp::surround::Virtualizer;
use clarivo_dsp::{db_to_gain, BLOCK, SAMPLE_RATE};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// 話者抽出モデルなど、マイクの途中に差し込む処理（フェーズ4）。
pub trait FrameProcessor: Send {
    /// 48kHz・モノラル・480 サンプルを処理する。
    fn process(&mut self, frame: &mut [f32]);
    /// 追加される遅延（サンプル）
    fn latency(&self) -> usize {
        0
    }
}

/// 話者判定スレッドとの連絡口。
pub struct SpeakerLink {
    /// 0 = 不明（開ける）, 1 = 本人, 2 = 他人
    pub decision: AtomicU8,
}

impl Default for SpeakerLink {
    fn default() -> Self {
        Self {
            decision: AtomicU8::new(0),
        }
    }
}

pub const DECISION_UNKNOWN: u8 = 0;
pub const DECISION_SELF: u8 = 1;
pub const DECISION_OTHER: u8 = 2;

// ---------------------------------------------------------------------------
// マイク
// ---------------------------------------------------------------------------

pub struct MicChain {
    settings: MicSettings,
    input_gain: LinearSmoother,
    hpf: Biquad,
    denoiser: Denoiser,
    vad: Rnnoise,
    pub last_vad: f32,
    gate: Gate,
    comp: Compressor,
    limiter: Limiter,
    mute: LinearSmoother,
    speaker_gain: LinearSmoother,
    speaker: Arc<SpeakerLink>,
    extractor: Option<Box<dyn FrameProcessor>>,
    aec: Option<sonora::AudioProcessing>,
    aec_out: Vec<f32>,
    /// 話者判定スレッドへ送る口（ノイズ除去後・ゲート前の音）
    tap: Option<rtrb::Producer<f32>>,
    pub notice: Option<String>,
}

impl MicChain {
    pub fn new(settings: &MicSettings, speaker: Arc<SpeakerLink>) -> Self {
        let (denoiser, notice) = Denoiser::create(settings.denoise, settings.strength_db);
        let mut c = Self {
            settings: settings.clone(),
            input_gain: LinearSmoother::new(db_to_gain(settings.gain_db), BLOCK as u32),
            hpf: Biquad::new(Coefs::design(
                FilterKind::HighPass,
                80.0,
                0.0,
                0.707,
                SAMPLE_RATE,
            )),
            denoiser,
            vad: Rnnoise::new(),
            last_vad: 0.0,
            gate: Gate::new(settings.gate.clone()),
            comp: Compressor::new(settings.compressor.clone()),
            limiter: Limiter::new(1, settings.limiter_db),
            mute: LinearSmoother::new(if settings.muted { 0.0 } else { 1.0 }, 960),
            speaker_gain: LinearSmoother::new(1.0, 960),
            speaker,
            extractor: None,
            aec: None,
            aec_out: vec![0.0; BLOCK],
            tap: None,
            notice,
        };
        c.update_aec();
        c
    }

    pub fn settings(&self) -> &MicSettings {
        &self.settings
    }

    /// 設定を反映する。ノイズ除去のモードが変わったときだけモデルを作り直す。
    pub fn apply(&mut self, s: &MicSettings) {
        if s.denoise != self.denoiser.mode() {
            let (d, notice) = Denoiser::create(s.denoise, s.strength_db);
            self.denoiser = d;
            self.notice = notice;
        }
        self.input_gain.set_target(db_to_gain(s.gain_db));
        self.gate.apply(&s.gate);
        self.comp.apply(&s.compressor);
        self.limiter.set_ceiling_db(s.limiter_db);
        self.mute.set_target(if s.muted { 0.0 } else { 1.0 });
        let aec_changed = s.echo_cancel != self.settings.echo_cancel;
        self.settings = s.clone();
        if aec_changed {
            self.update_aec();
        }
    }

    fn update_aec(&mut self) {
        if self.settings.echo_cancel {
            use sonora::config::EchoCanceller;
            let config = sonora::Config {
                echo_canceller: Some(EchoCanceller::default()),
                ..Default::default()
            };
            let apm = sonora::AudioProcessing::builder()
                .config(config)
                .capture_config(sonora::StreamConfig::new(48_000, 1))
                .render_config(sonora::StreamConfig::new(48_000, 1))
                .build();
            self.aec = Some(apm);
        } else {
            self.aec = None;
        }
    }

    pub fn set_tap(&mut self, tap: Option<rtrb::Producer<f32>>) {
        self.tap = tap;
    }

    pub fn set_extractor(&mut self, ex: Option<Box<dyn FrameProcessor>>) {
        self.extractor = ex;
    }

    pub fn has_extractor(&self) -> bool {
        self.extractor.is_some()
    }

    pub fn denoise_mode(&self) -> DenoiseMode {
        self.denoiser.mode()
    }

    /// エコーキャンセルが参照信号（ヘッドホンに出している音）を必要とするか。
    pub fn wants_echo_reference(&self) -> bool {
        self.aec.is_some()
    }

    /// 処理による遅延（サンプル）の概算。
    pub fn latency(&self) -> usize {
        let dn = match self.denoiser.mode() {
            DenoiseMode::Off => 0,
            DenoiseMode::Light => BLOCK,
            DenoiseMode::High => 2 * BLOCK,
        };
        dn + self.limiter.latency() + self.extractor.as_ref().map(|e| e.latency()).unwrap_or(0)
    }

    /// 1 フレーム（480 サンプル）を処理する。`echo_ref` はヘッドホン出力のモノラル。
    pub fn process(&mut self, frame: &mut [f32], echo_ref: Option<&[f32]>, meters: &Meters) {
        debug_assert_eq!(frame.len(), BLOCK);
        for s in frame.iter_mut() {
            *s *= self.input_gain.next();
        }
        meters.mic_in.update(&[frame]);
        if self.settings.highpass {
            self.hpf.process(frame);
        }
        if let Some(apm) = self.aec.as_mut() {
            if let Some(r) = echo_ref {
                let mut sink = [0.0f32; BLOCK];
                let _ = apm.process_render_f32(&[r], &mut [&mut sink[..]]);
            }
            if apm
                .process_capture_f32(&[&*frame], &mut [&mut self.aec_out[..]])
                .is_ok()
            {
                frame.copy_from_slice(&self.aec_out);
            }
        }
        if self.settings.speaker.mode == SpeakerMode::Extract {
            if let Some(ex) = self.extractor.as_mut() {
                ex.process(frame);
            }
        }
        // 話者判定やメーター用に、ノイズ除去とは別に音声らしさを求める
        self.last_vad = self.vad.vad_only(frame);
        self.denoiser.process(frame, self.settings.strength_db);
        if self.settings.speaker.mode == SpeakerMode::Gate {
            if let Some(tap) = self.tap.as_mut() {
                // 判定スレッドが遅れていて満杯なら捨てる（音声処理を待たせない）
                if tap.slots() >= frame.len() {
                    for &s in frame.iter() {
                        let _ = tap.push(s);
                    }
                }
            }
        }

        // 話者ゲート：他人の声と判定されている間は下げる
        let speaker_target = if self.settings.speaker.mode == SpeakerMode::Gate
            && self.speaker.decision.load(Ordering::Relaxed) == DECISION_OTHER
        {
            db_to_gain(self.settings.speaker.attenuation_db)
        } else {
            1.0
        };
        self.speaker_gain.set_target(speaker_target);
        for s in frame.iter_mut() {
            *s *= self.speaker_gain.next();
        }

        self.gate.process(&mut [&mut *frame]);
        self.comp.process(&mut [&mut *frame]);
        self.limiter.process(&mut [&mut *frame]);
        for s in frame.iter_mut() {
            *s *= self.mute.next();
        }
        meters.mic_out.update(&[frame]);
        meters
            .gate_open
            .store(self.gate.is_open(), Ordering::Relaxed);
        meters.mic_reduction_db.store(self.comp.last_reduction_db);
    }
}

// ---------------------------------------------------------------------------
// 出力ミキサー
// ---------------------------------------------------------------------------

struct ChannelProc {
    eq: Peq,
    leveler: Leveler,
    volume: LinearSmoother,
    settings: ChannelSettings,
}

impl ChannelProc {
    fn new(s: &ChannelSettings) -> Self {
        let mut eq = Peq::new(2);
        eq.apply(&s.eq);
        Self {
            eq,
            leveler: Leveler::new(s.leveler.clone()),
            volume: LinearSmoother::new(0.0, BLOCK as u32),
            settings: s.clone(),
        }
    }

    fn apply(&mut self, s: &ChannelSettings) {
        self.eq.apply(&s.eq);
        self.leveler.apply(&s.leveler);
        self.settings = s.clone();
    }

    fn process(&mut self, l: &mut [f32], r: &mut [f32], extra_gain: f32) {
        self.eq.process(&mut [&mut *l, &mut *r]);
        self.leveler.process(&mut [&mut *l, &mut *r]);
        let target = if self.settings.muted {
            0.0
        } else {
            db_to_gain(self.settings.volume_db) * extra_gain
        };
        self.volume.set_target(target);
        for i in 0..l.len() {
            let g = self.volume.next();
            l[i] *= g;
            r[i] *= g;
        }
    }
}

pub struct OutputMixer {
    game: ChannelProc,
    chat: ChannelProc,
    virtualizer: Virtualizer,
    chat_denoise: Option<Denoiser>,
    hp_eq: Peq,
    master: LinearSmoother,
    monitor: LinearSmoother,
    limiter: Limiter,
    stream_limiter: Limiter,
    stream_game: LinearSmoother,
    stream_chat: LinearSmoother,
    stream_mic: LinearSmoother,
    output: OutputSettings,
    stream: StreamSettings,
    monitor_on: bool,
    monitor_db: f32,
    // 作業用バッファ
    gl: Vec<f32>,
    gr: Vec<f32>,
    cl: Vec<f32>,
    cr: Vec<f32>,
    mono: Vec<f32>,
}

/// 1 ブロック分の入力。
pub struct MixInput<'a> {
    /// Game：7.1ch（FL FR FC LFE BL BR SL SR）
    pub game: [&'a [f32]; 8],
    /// Chat：2ch
    pub chat: [&'a [f32]; 2],
    /// 処理済みマイク（モニター・配信用）
    pub mic: &'a [f32],
}

impl OutputMixer {
    pub fn new(
        output: &OutputSettings,
        game: &ChannelSettings,
        chat: &ChannelSettings,
        stream: &StreamSettings,
        monitor: bool,
        monitor_db: f32,
    ) -> Self {
        let mut hp_eq = Peq::new(2);
        hp_eq.apply(&output.headphone_eq);
        let mut virtualizer = Virtualizer::new(SAMPLE_RATE);
        virtualizer.apply(&game.surround);
        let mut m = Self {
            game: ChannelProc::new(game),
            chat: ChannelProc::new(chat),
            virtualizer,
            chat_denoise: None,
            hp_eq,
            master: LinearSmoother::new(db_to_gain(output.master_db), BLOCK as u32),
            monitor: LinearSmoother::new(0.0, BLOCK as u32),
            limiter: Limiter::new(2, -0.3),
            stream_limiter: Limiter::new(2, -1.0),
            stream_game: LinearSmoother::new(0.0, BLOCK as u32),
            stream_chat: LinearSmoother::new(0.0, BLOCK as u32),
            stream_mic: LinearSmoother::new(0.0, BLOCK as u32),
            output: output.clone(),
            stream: stream.clone(),
            monitor_on: monitor,
            monitor_db,
            gl: vec![0.0; BLOCK],
            gr: vec![0.0; BLOCK],
            cl: vec![0.0; BLOCK],
            cr: vec![0.0; BLOCK],
            mono: vec![0.0; BLOCK],
        };
        m.update_chat_denoise();
        m
    }

    pub fn apply(
        &mut self,
        output: &OutputSettings,
        game: &ChannelSettings,
        chat: &ChannelSettings,
        stream: &StreamSettings,
        monitor: bool,
        monitor_db: f32,
    ) {
        self.game.apply(game);
        let denoise_changed = chat.denoise != self.chat.settings.denoise;
        self.chat.apply(chat);
        if denoise_changed {
            self.update_chat_denoise();
        }
        self.virtualizer.apply(&game.surround);
        self.hp_eq.apply(&output.headphone_eq);
        self.master.set_target(db_to_gain(output.master_db));
        self.output = output.clone();
        self.stream = stream.clone();
        self.monitor_on = monitor;
        self.monitor_db = monitor_db;
    }

    fn update_chat_denoise(&mut self) {
        self.chat_denoise = if self.chat.settings.denoise {
            // 受信音は軽量モードで十分（相手側でも処理されていることが多い）
            Some(Denoiser::create(DenoiseMode::Light, self.chat.settings.denoise_strength_db).0)
        } else {
            None
        };
    }

    /// 1 ブロックを処理して、ヘッドホン用（と配信用）の 2ch を書く。
    pub fn process(
        &mut self,
        input: &MixInput,
        out_l: &mut [f32],
        out_r: &mut [f32],
        stream_out: Option<(&mut [f32], &mut [f32])>,
        meters: &Meters,
    ) {
        let n = out_l.len();
        debug_assert_eq!(n, BLOCK);
        let (game_mix, chat_mix) = chatmix_gains(self.output.chatmix);

        // --- Game ---
        if self.game.settings.surround.enabled {
            self.virtualizer
                .process(&input.game, &mut self.gl[..n], &mut self.gr[..n]);
        } else {
            downmix_71_to_stereo(&input.game, &mut self.gl[..n], &mut self.gr[..n]);
        }
        // 配信用は ChatMix の影響を受けない音量で使うので、先に EQ だけかけた音を残す
        self.game
            .process(&mut self.gl[..n], &mut self.gr[..n], 1.0);
        meters.game.update(&[&self.gl[..n], &self.gr[..n]]);

        // --- Chat ---
        self.cl[..n].copy_from_slice(&input.chat[0][..n]);
        self.cr[..n].copy_from_slice(&input.chat[1][..n]);
        if let Some(dn) = self.chat_denoise.as_mut() {
            for i in 0..n {
                self.mono[i] = 0.5 * (self.cl[i] + self.cr[i]);
            }
            dn.process(&mut self.mono[..n], self.chat.settings.denoise_strength_db);
            self.cl[..n].copy_from_slice(&self.mono[..n]);
            self.cr[..n].copy_from_slice(&self.mono[..n]);
        }
        self.chat
            .process(&mut self.cl[..n], &mut self.cr[..n], 1.0);
        meters.chat.update(&[&self.cl[..n], &self.cr[..n]]);

        // --- 配信用ミックス ---
        if let Some((sl, sr)) = stream_out {
            self.stream_game
                .set_target(db_to_gain(self.stream.game_db));
            self.stream_chat
                .set_target(db_to_gain(self.stream.chat_db));
            self.stream_mic.set_target(if self.stream.include_mic {
                db_to_gain(self.stream.mic_db)
            } else {
                0.0
            });
            for i in 0..n {
                let g = self.stream_game.next();
                let c = self.stream_chat.next();
                let m = self.stream_mic.next() * input.mic[i];
                sl[i] = self.gl[i] * g + self.cl[i] * c + m;
                sr[i] = self.gr[i] * g + self.cr[i] * c + m;
            }
            self.stream_limiter.process(&mut [&mut *sl, &mut *sr]);
            meters.stream.update(&[&*sl, &*sr]);
        }

        // --- ヘッドホン ---
        out_l.fill(0.0);
        out_r.fill(0.0);
        mix_into(out_l, &self.gl[..n], game_mix);
        mix_into(out_r, &self.gr[..n], game_mix);
        mix_into(out_l, &self.cl[..n], chat_mix);
        mix_into(out_r, &self.cr[..n], chat_mix);
        self.monitor.set_target(if self.monitor_on {
            db_to_gain(self.monitor_db)
        } else {
            0.0
        });
        for i in 0..n {
            let m = input.mic[i] * self.monitor.next();
            out_l[i] += m;
            out_r[i] += m;
        }
        self.hp_eq.process(&mut [&mut *out_l, &mut *out_r]);
        for i in 0..n {
            let g = self.master.next();
            out_l[i] *= g;
            out_r[i] *= g;
        }
        self.limiter.process(&mut [&mut *out_l, &mut *out_r]);
        meters.master.update(&[&*out_l, &*out_r]);
        meters
            .limiter_reduction_db
            .store(self.limiter.last_reduction_db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use std::f32::consts::PI;

    fn tone(freq: f32, amp: f32, offset: usize) -> Vec<f32> {
        (0..BLOCK)
            .map(|i| (2.0 * PI * freq * (offset + i) as f32 / SAMPLE_RATE).sin() * amp)
            .collect()
    }

    #[test]
    fn mic_chain_passes_voice_like_tone_and_mutes() {
        let mut s = Settings::default().mic;
        s.denoise = DenoiseMode::Off;
        s.compressor.enabled = false;
        let meters = Meters::new();
        let mut chain = MicChain::new(&s, Arc::new(SpeakerLink::default()));
        let mut out_energy = 0.0;
        for b in 0..50 {
            let mut f = tone(440.0, 0.3, b * BLOCK);
            chain.process(&mut f, None, &meters);
            if b > 10 {
                out_energy += f.iter().map(|x| x * x).sum::<f32>();
            }
        }
        assert!(out_energy > 0.3 * 0.3 / 2.0 * BLOCK as f32 * 39.0 * 0.8);
        s.muted = true;
        chain.apply(&s);
        let mut last = Vec::new();
        for b in 50..60 {
            let mut f = tone(440.0, 0.3, b * BLOCK);
            chain.process(&mut f, None, &meters);
            last = f;
        }
        assert!(last.iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn speaker_gate_attenuates_other_voices() {
        let mut s = Settings::default().mic;
        s.denoise = DenoiseMode::Off;
        s.gate.enabled = false;
        s.compressor.enabled = false;
        s.speaker.mode = SpeakerMode::Gate;
        let link = Arc::new(SpeakerLink::default());
        let meters = Meters::new();
        let mut chain = MicChain::new(&s, link.clone());
        link.decision.store(DECISION_OTHER, Ordering::Relaxed);
        let mut f = Vec::new();
        for b in 0..10 {
            f = tone(300.0, 0.3, b * BLOCK);
            chain.process(&mut f, None, &meters);
        }
        let peak = f.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(peak < 0.3 * db_to_gain(-30.0), "peak {peak}");
    }

    #[test]
    fn chatmix_and_monitor() {
        let st = Settings::default();
        let mut out = st.output.clone();
        out.chatmix = 1.0; // チャットのみ
        let mut mixer = OutputMixer::new(&out, &st.game, &st.chat, &st.stream, false, -6.0);
        let meters = Meters::new();
        let game = tone(1000.0, 0.2, 0);
        let zero = vec![0.0f32; BLOCK];
        let input = MixInput {
            game: [&game, &game, &zero, &zero, &zero, &zero, &zero, &zero],
            chat: [&zero, &zero],
            mic: &zero,
        };
        let mut l = vec![0.0; BLOCK];
        let mut r = vec![0.0; BLOCK];
        for _ in 0..5 {
            mixer.process(&input, &mut l, &mut r, None, &meters);
        }
        // チャットのみなのでゲーム音は聞こえない
        assert!(l.iter().all(|x| x.abs() < 1e-4));
        // 配信ミックスは ChatMix に関係なくゲーム音が入る
        let mut sl = vec![0.0; BLOCK];
        let mut sr = vec![0.0; BLOCK];
        for _ in 0..5 {
            mixer.process(&input, &mut l, &mut r, Some((&mut sl, &mut sr)), &meters);
        }
        assert!(sl.iter().any(|x| x.abs() > 0.1));
    }

    #[test]
    fn surround_output_is_finite_and_bounded() {
        let mut st = Settings::default();
        st.game.surround.enabled = true;
        let mut mixer =
            OutputMixer::new(&st.output, &st.game, &st.chat, &st.stream, false, -6.0);
        let meters = Meters::new();
        let loud = tone(200.0, 1.0, 0);
        let input = MixInput {
            game: [&loud; 8],
            chat: [&loud, &loud],
            mic: &loud,
        };
        let mut l = vec![0.0; BLOCK];
        let mut r = vec![0.0; BLOCK];
        for _ in 0..20 {
            mixer.process(&input, &mut l, &mut r, None, &meters);
            assert!(l.iter().chain(r.iter()).all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }
}
