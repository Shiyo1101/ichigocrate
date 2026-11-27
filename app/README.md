# ichigocrate-app

[ichigocrate-core](../core) を使った egui 製デスクトップフロントエンド。

## ビルドと実行

```bash
cargo run --release -p ichigocrate-app
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
| CapsLock | 英字の大文字/小文字入力の切り替え (CAPS)      |
| Cmd+V / Ctrl+V | プログラムの貼り付け (確認画面が出る) |

英字は既定で大文字入力 (実機と同じ)。CapsLock の切り替えは、次に英字を打ったときに反映される。カナモード中はウィンドウタイトルに `KANA` が出る。

## プログラムの貼り付け

Cmd+V (macOS) / Ctrl+V でクリップボードのプログラムを打ち込む。確認画面で中身を確かめ、Enter で貼り付け、Esc で取り消す。`NEW FIRST` にチェックを入れると、今のプログラムを消してから貼り付ける。

## ファイル保存先

`SAVE n` / `LOAD n` / `FILES` は `~/.ichigocrate/slot_NN.ijb` (NN はスロット番号 0-15) に読み書きする。

## GET / POST (HTTPS 通信)

ichigocrate 拡張の `GET` / `POST` ([core の説明](../core/README.md#get--post-https-通信)) は `ureq` で通信する。内部ネットワークへはつながない (名前解決した結果のアドレスも検査する)。OS の証明書ストアとプロキシ設定は使わない。

実際に通信するテストは `cargo test -p ichigocrate-app -- --ignored` で実行する。
