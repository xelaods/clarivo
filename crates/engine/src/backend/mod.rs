//! OS ごとの音声入出力。Windows は WASAPI、それ以外は動作確認用の無音バックエンド。

mod common;

#[cfg(windows)]
mod wasapi_backend;
#[cfg(windows)]
pub(crate) use wasapi_backend::{list_devices, run_mic, run_output};

#[cfg(not(windows))]
mod null;
#[cfg(not(windows))]
pub(crate) use null::{list_devices, run_mic, run_output};

pub(crate) use common::split_links;
