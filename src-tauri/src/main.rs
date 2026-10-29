// リリースビルドではコンソール窓を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // アンインストーラーから呼ばれる：振り分けと既定デバイスを元に戻して終わる
    if std::env::args().any(|a| a == "--reset-audio") {
        clarivo_lib::reset_audio_cli();
        return;
    }
    clarivo_lib::run();
}
