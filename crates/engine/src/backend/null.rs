//! Windows 以外で使う動作確認用のバックエンド。実デバイスは使わず、
//! 10ms ごとに小さなノイズをマイク入力として流し、出力は捨てる。

use super::common::{Flow, MicLinks, MicRunner, OutLinks, OutRunner};
use crate::chains::SpeakerLink;
use crate::devices::DeviceList;
use crate::settings::Settings;
use crate::state::Shared;
use crate::{MicCommand, OutCommand};
use clarivo_dsp::BLOCK;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn list_devices() -> anyhow::Result<DeviceList> {
    Ok(DeviceList::default())
}

pub(crate) fn run_mic(
    settings: Settings,
    rx: Receiver<MicCommand>,
    links: MicLinks,
    shared: Arc<Shared>,
    speaker: Arc<SpeakerLink>,
) {
    let mut runner = MicRunner::new(settings, links, shared.clone(), speaker);
    shared.status.lock().mic.running = true;
    let mut seed = 0x1234_5678u32;
    let mut frame = vec![0.0f32; BLOCK];
    loop {
        while let Ok(cmd) = rx.try_recv() {
            if let Flow::Stop = runner.handle(cmd) {
                shared.status.lock().mic.running = false;
                return;
            }
        }
        for s in frame.iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *s = (seed as f32 / u32::MAX as f32 - 0.5) * 0.001;
        }
        runner.push(&frame);
        runner.process_ready(|_| {});
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn run_output(
    settings: Settings,
    rx: Receiver<OutCommand>,
    links: OutLinks,
    shared: Arc<Shared>,
) {
    let mut runner = OutRunner::new(settings, links, shared.clone());
    shared.status.lock().output.running = true;
    let mut out = Vec::new();
    loop {
        while let Ok(cmd) = rx.try_recv() {
            match runner.handle(cmd) {
                Flow::Stop => {
                    shared.status.lock().output.running = false;
                    return;
                }
                Flow::Reopen => runner.sync_stream(),
                Flow::Continue => {}
            }
        }
        runner.render(BLOCK, &mut out);
        std::thread::sleep(Duration::from_millis(10));
    }
}
