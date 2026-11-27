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
| Cmd+V / Ctrl+V | プログラムの貼り付け (確認画面が出る。下記) |

英字は既定で大文字入力 (CAPS 既定 ON、実機と同じ)。CAPS が ON の間は Shift 併用で小文字になり、CapsLock を押すと反転する。egui は CapsLock キーを報告しないため、英字を Shift なしで打ったときに OS が生成した文字から CapsLock の状態変化を検知している (切り替えは次に英字を打ったときに反映される)。カナモード ON のあいだはウィンドウタイトルに `KANA` が表示される。

## プログラムの貼り付け

Cmd+V (macOS) / Ctrl+V で、クリップボードのテキストを打ち込める。いきなり打ち込まず、確認画面を出す。

- 確認画面には、行数・大きさと、IchigoJam のフォントで描いた中身 (画面と同じ 32 文字幅で折り返す) を出す
- 注意も出す: 行番号の無い行はその場で実行される (`RUN` など)、199 文字を超える行は切れる、IchigoJam で表せない文字 (全角文字など) は飛ばす、実行中のプログラムは止める
- `NEW FIRST` にチェックがあると、先に `NEW` して今のプログラムを消す。行番号付きの行を含むときは既定でチェックが入る
- Enter で貼り付け、Esc で取り消し。確認画面の間のキー入力は BASIC へ渡さない
- 改行は CRLF / CR / LF のどれでもよく、半角カナは IchigoJam のカナになる。打ち込みは 1 行ずつ Enter を押した扱いで、途中で ESC を押すと残りを取り消す

テキストの変換は `core::paste`、流し込みは `Session::queue_paste` (`GET` の応答の流し込みと同じ仕組み) が行う。

## ファイル保存先

`SAVE n` / `LOAD n` / `FILES` はホスト OS の `~/.ichigocrate/slot_NN.ijb` (NN はスロット番号 0-15) に LIST 領域のバイナリを直接読み書きする (`core::machine::Storage` の `DiskStorage` 実装)。

## GET / POST (HTTPS 通信)

ichigocrate 拡張の `GET` / `POST` ([core の説明](../core/README.md#get--post-https-通信)) は、`ureq` で通信する (`src/http.rs`)。リクエストごとにスレッドで通信するので UI は固まらない。内部ネットワークへ到達させないため、次のようにしている。

- 証明書は ureq 同梱の公開ルート証明書 (webpki-roots) だけで検証し、OS の証明書ストアは使わない (社内 CA のサービスにつながらないように)
- 名前解決の結果が 1 つでも内部ネットワークのアドレスなら接続しない。検査したアドレスへそのまま接続するので、検査後に名前を引き直してすり替える攻撃 (DNS リバインディング) も効かない
- リダイレクトは追わず、OS・環境変数のプロキシ設定も使わない

実際の通信を伴うテストは既定では実行しない (`cargo test -p ichigocrate-app -- --ignored` で実行)。

## 実装メモ

- **キー入力**: 物理キー位置 (`Event::Key.physical_key`) を USB HID Usage ID へ変換し、`core::keymap` の US/JA 表 (元 C ファーム `hid.h` 由来) を引いて IchigoJam 内部コードに翻訳する。OS のレイアウト変換を経由しないため `KBD` コマンドの効果が実入力に反映される
- **音声出力**: cpal で矩形波を生成。`Machine.current_tone_hz: f32` を `Arc<AtomicF32>` 経由で共有し、コールバックが波形を生成する
- **時刻の受け渡し**: 起動からの経過時間 (ms) を `Instant` で測り、毎フレーム `Session::tick` へ渡す。WAIT の実時間変換や 60Hz tick は core の `Session` が行うので、ProMotion など高リフレッシュレート環境でも WAIT の長さは変わらない
- **macOS**: IMK (Input Method Kit) 関連のログを stderr フィルタで抑制している
