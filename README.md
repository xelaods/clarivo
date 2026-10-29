# Clarivo

Windows 用のオーディオアプリです（SteelSeries Sonar 風）。AI でマイクのノイズを消し、ゲームと通話の音を分けて、それぞれに EQ をかけられます。

## できること

- **マイク**：AI ノイズ除去（軽量の RNNoise ／ 高品質の DeepFilterNet3）、ノイズゲート、コンプレッサー、リミッター、エコーキャンセル、マイクモニター、録音テスト（処理前と処理後の聞き比べ）
- **登録した声だけを通す**：自分の声を 30 秒登録すると、自分が話していない間の家族やテレビの声を下げる（話者ゲート）。話者抽出モデル（ONNX）の差し込みにも対応（試験機能）
- **Game ／ Chat の 2 チャンネル**：通話アプリ（Discord など）を自動で Chat に、それ以外を Game に振り分け、ChatMix で比率を調整
- **イコライザー**：10 バンドのパラメトリック EQ（チャンネルごと）、足音強調などのプリセット、小さい音を持ち上げる処理、AutoEq のヘッドホン補正
- **バーチャルサラウンド**（7.1ch → ヘッドホン）、**配信用ミックス**（OBS 向け）
- ゲームごとのプロファイル自動切り替え、ホットキー、オーバーレイ、タスクトレイ常駐、Windows 起動時の自動開始、コマンドメニュー（Ctrl+K）

## インストール

1. [Releases](https://github.com/xelaods/clarivo/releases) か、[Actions](https://github.com/xelaods/clarivo/actions) の最新ビルドの「clarivo-setup」から `Clarivo_x.y.z_x64-setup.exe` を入手して実行します。
2. [VB-CABLE](https://vb-audio.com/Cable/) をインストールします。
   - **VB-CABLE**（無料）：仮想マイク用
   - **VB-CABLE A+B**（寄付制）：Game と Chat 用
3. Windows を再起動し、Clarivo を起動します。初回はセットアップガイドが開きます。
4. Discord などのマイク（入力デバイス）に **CABLE Output (VB-Audio Virtual Cable)** を選びます。出力デバイスは「既定」のままで構いません。

Clarivo を終了すると、Windows の既定デバイスとアプリ別の出力先は元に戻ります。アンインストール時も自動で戻します。

## 開発

```
cd ui && npm ci && npm run build   # 画面
cargo test --release               # 音声エンジン・DSP など（Windows 以外では無音のバックエンドで動く）
ui/node_modules/.bin/tauri build   # アプリ本体とインストーラー（Windows）
```

| 場所 | 内容 |
| --- | --- |
| `crates/dsp` | EQ・ダイナミクス・サラウンド・リサンプルなどの音声処理部品 |
| `crates/denoise` | RNNoise ／ DeepFilterNet3 |
| `crates/engine` | 設定、処理チェーン、WASAPI の入出力 |
| `crates/routing` | アプリ別の振り分け、既定デバイスの切り替え |
| `crates/speaker` | 声の登録、話者ゲート、話者抽出 |
| `src-tauri` | アプリ本体（トレイ、ホットキー、オーバーレイ、プロファイル） |
| `ui` | 画面（React ＋ [EasyUI](https://easyui.site/)） |

ログは `%LOCALAPPDATA%\com.xelaods.clarivo\logs\clarivo.log` にあります（設定ページの「ログフォルダを開く」）。

## ライセンス

MIT。使用しているオープンソースと学習済みモデルのライセンスは、アプリの設定ページ「Clarivo について」に表示しています。話者モデル（WeSpeaker ResNet34-LM）は CC BY 4.0 です。
