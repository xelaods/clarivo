//! 話者抽出モデルの差し込み口（段階B、試験機能）。
//!
//! 混ざった音から登録した声だけを残すモデル（WeSep などで学習・書き出したもの）を
//! 読み込んでマイクの途中に入れる。モデルの入出力は次の形に合わせてもらう。
//!
//! - 入力 0: 混合音 `float32 [1, 8000]`（16kHz、0.5 秒）
//! - 入力 1: 声紋 `float32 [1, D]`（Clarivo の登録で作る声紋と同じもの）
//! - 出力 0: 抽出した音 `float32 [1, 8000]`
//!
//! 推論は専用スレッドで行い、音声スレッドとはリングバッファでつなぐ。
//! 0.1 秒ごとに直近 0.5 秒を処理するので、遅延は約 0.2 秒増える。

use anyhow::{Context, Result};
use clarivo_dsp::resample::{Decimator3, Interpolator3};
use clarivo_engine::chains::FrameProcessor;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tract_onnx::prelude::*;

pub const WINDOW: usize = 8_000;
pub const HOP: usize = 1_600;
const FADE: usize = 160;

type Plan = TypedSimplePlan<TypedModel>;

fn load_plan(path: &Path, emb_dim: usize) -> Result<Plan> {
    Ok(tract_onnx::onnx()
        .model_for_path(path)
        .with_context(|| format!("話者抽出モデルを読めません: {}", path.display()))?
        .with_input_fact(0, f32::fact([1, WINDOW]).into())?
        .with_input_fact(1, f32::fact([1, emb_dim]).into())?
        .into_optimized()?
        .into_runnable()?)
}

pub struct Extractor {
    to_worker: rtrb::Producer<f32>,
    from_worker: rtrb::Consumer<f32>,
    dec: Decimator3,
    interp: Interpolator3,
    tmp16: Vec<f32>,
    tmp48: Vec<f32>,
    out48: VecDeque<f32>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Extractor {
    pub fn load(path: &Path, embedding: Vec<f32>) -> Result<Self> {
        let plan = load_plan(path, embedding.len())?;
        let (to_worker, mut worker_in) = rtrb::RingBuffer::<f32>::new(WINDOW * 4);
        let (mut worker_out, from_worker) = rtrb::RingBuffer::<f32>::new(WINDOW * 4);
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let emb_arr = tract_ndarray::Array2::from_shape_vec((1, embedding.len()), embedding)?;
        let handle = std::thread::Builder::new()
            .name("clarivo-extract".into())
            .spawn(move || {
                // TValue はスレッドをまたげないので、このスレッドの中で作る
                let emb: TValue = Tensor::from(emb_arr).into();
                let mut hist: VecDeque<f32> = VecDeque::from(vec![0.0; WINDOW]);
                let mut pending = 0usize;
                let mut prev_tail = vec![0.0f32; FADE];
                let mut input = vec![0.0f32; WINDOW];
                while !stop2.load(Ordering::Relaxed) {
                    while let Ok(s) = worker_in.pop() {
                        hist.push_back(s);
                        hist.pop_front();
                        pending += 1;
                    }
                    if pending < HOP {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    pending -= HOP;
                    for (d, s) in input.iter_mut().zip(hist.iter()) {
                        *d = *s;
                    }
                    let Ok(mix) = tract_ndarray::Array2::from_shape_vec((1, WINDOW), input.clone())
                    else {
                        continue;
                    };
                    let result = plan.run(tvec!(Tensor::from(mix).into(), emb.clone()));
                    let y: Vec<f32> = match result {
                        Ok(out) => match out[0].to_array_view::<f32>() {
                            Ok(v) => v.iter().copied().collect(),
                            Err(_) => input.clone(),
                        },
                        Err(e) => {
                            log::warn!("話者抽出の推論に失敗: {e}");
                            input.clone()
                        }
                    };
                    if y.len() < WINDOW {
                        continue;
                    }
                    // 未来側に 1 区間分の余裕がある部分を出す（つなぎ目は短く重ねる）
                    let start = WINDOW - 2 * HOP;
                    let mut seg: Vec<f32> = y[start..start + HOP + FADE].to_vec();
                    for i in 0..FADE {
                        let w = i as f32 / FADE as f32;
                        seg[i] = prev_tail[i] * (1.0 - w) + seg[i] * w;
                    }
                    prev_tail.copy_from_slice(&seg[HOP..HOP + FADE]);
                    for &s in &seg[..HOP] {
                        let _ = worker_out.push(s);
                    }
                }
            })?;
        // 最初の 0.2 秒分は無音で埋めておく（推論結果が届くまでの遅延）
        let mut out48 = VecDeque::with_capacity(48_000);
        out48.extend(std::iter::repeat(0.0).take(HOP * 2 * 3));
        Ok(Self {
            to_worker,
            from_worker,
            dec: Decimator3::new(),
            interp: Interpolator3::new(),
            tmp16: Vec::with_capacity(480),
            tmp48: Vec::with_capacity(HOP * 3 * 4),
            out48,
            stop,
            handle: Some(handle),
        })
    }
}

impl FrameProcessor for Extractor {
    fn process(&mut self, frame: &mut [f32]) {
        self.tmp16.clear();
        self.dec.process(frame, &mut self.tmp16);
        for &s in &self.tmp16 {
            let _ = self.to_worker.push(s);
        }
        let mut got = Vec::new();
        while let Ok(s) = self.from_worker.pop() {
            got.push(s);
        }
        if !got.is_empty() {
            self.tmp48.clear();
            self.interp.process(&got, &mut self.tmp48);
            self.out48.extend(self.tmp48.iter().copied());
        }
        for s in frame.iter_mut() {
            *s = self.out48.pop_front().unwrap_or(0.0);
        }
        // 遅れが溜まりすぎたら詰める
        let max = HOP * 3 * 4;
        if self.out48.len() > max {
            let drop = self.out48.len() - HOP * 3 * 2;
            self.out48.drain(..drop);
        }
    }

    fn latency(&self) -> usize {
        HOP * 2 * 3
    }
}

impl Drop for Extractor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
