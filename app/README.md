# ichigojam-app

[ichigojam-core](../core) を使った egui 製デスクトップフロントエンド。

## ビルドと実行

```bash
cargo run --release -p ichigojam-app
```

## キーボードショートカット (IchigoJam 標準準拠)

| キー | 動作                                             |
| ---- | ------------------------------------------------ |
| F1   | `CLS` を即時実行                                 |
| F2   | `LOAD` を挿入 (スロット番号を続けて入力 → Enter) |
| F3   | `SAVE` を挿入 (スロット番号を続けて入力 → Enter) |
| F4   | `LIST` を即時実行                                |
| F5   | `RUN` を即時実行                                 |
| F6   | `?FREE()` を即時実行                             |
| F7   | `?VER()` を即時実行                              |
| F8   | `VIDEO` を挿入 (モード番号を続けて入力 → Enter)  |
| F9   | `FILES` を即時実行                               |
| ESC  | プログラム中断 (Break)                           |
| F10  | ローマ字 → 半角カナ変換のオン/オフ               |

英字は常に大文字入力 (CAPS デフォルト ON、IchigoJam 慣習)。カナモード ON のあいだはウィンドウタイトルに `KANA` が表示される。

## ファイル保存先

`SAVE n` / `LOAD n` / `FILES` はホスト OS の `~/.ichigojam-rs/slot_NN.ijb` (NN はスロット番号 0-15) に LIST 領域のバイナリを直接読み書きする (`core::machine::Storage` の `DiskStorage` 実装)。

## 実装メモ

- **キー入力**: 物理キー位置 (`Event::Key.physical_key`) を USB HID Usage ID へ変換し、`core::keymap` の US/JA 表 (元 C ファーム `hid.h` 由来) を引いて IchigoJam 内部コードに翻訳する。OS のレイアウト変換を経由しないため `KBD` コマンドの効果が実入力に反映される
- **音声出力**: cpal で矩形波を生成。`Machine.current_tone_hz: f32` を `Arc<AtomicU32>` 経由で共有し、コールバックが波形を生成する
- **WAIT の実時間変換**: `core` 側は `wait_frames` を加算するだけなので、本アプリが `Instant` を使って 60Hz 換算の待機時間へ変換する (ProMotion など高リフレッシュレート環境でも正確に動作)
- **macOS**: IMK (Input Method Kit) 関連のログを stderr フィルタで抑制している
