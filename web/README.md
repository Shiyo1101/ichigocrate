# ichigocrate-web

[ichigocrate-core](../core) を使った WebAssembly フロントエンド。eframe は使わず canvas 2D へ直接 blit する軽量構成。

```
web/
├── src/
│   ├── lib.rs        # クレート docs + モジュール宣言 + 再エクスポート
│   ├── runner.rs     # IchigoCrateRunner: core を直接駆動し canvas へ blit
│   ├── keymap.rs     # KeyboardEvent.code → HID/BTN コード変換
│   ├── output.rs     # onPrint 用の VRAM 差分ヘルパ
│   ├── http.rs       # FetchHttpClient: ichigocrate 拡張 GET・POST の fetch 実装
│   └── storage.rs    # WebStorage: SAVE/LOAD/FILES の localStorage 実装
├── build.sh          # wasm ビルド + wasm-bindgen (--target web)
└── demo/
    ├── index.html    # 単体デモページ (マークアップ + スタイル)
    └── main.js       # wasm 初期化・rAF ループ・キーイベント配線
```

## ビルドと実行

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.121   # Cargo.lock と同一版
cd web && ./build.sh                               # pkg/ に wasm + JS グルー生成
python3 -m http.server                             # http://localhost:8000/demo/
```

ビルド成果物 (`pkg/`) は ES モジュールとして `import` できる。

## 外部制御 API (`IchigoCrateHandle`)

`IchigoCrateRunner` は描画・キー入力に加え、JS/TS から実行・入力・状態取得を行うメソッドを持つ。

```js
// storagePrefix と persist は省略可 (既定 ""/true)。
const r = new IchigoCrateRunner(canvas, "demo-1", true);
r.exec("PRINT 1+2"); // 1 行を直接実行 (停止中のみ)
r.loadProgram('10 ?"HI"\n20 GOTO 10');
r.run(); // RUN。無限ループはフレーム実行へ委譲
r.type("LIST\n"); // キーボード入力と同等 (実行中は INKEY/INPUT へ)
r.keyDown(28);
r.keyUp(28); // BTN()/INKEY() 用 物理キー (28=←)
r.break(); // ESC 注入で中断
r.reset(); // 電源 ON/OFF 相当の再起動
r.getScreenText(); // 画面スナップショット
r.getVar("A"); // 変数 A-Z
r.peek(0x900); // メモリ読み取り
r.onPrint((chunk) => (out += chunk)); // 画面出力ストリーミング購読
r.onError((e) => console.log(e.code, e.message)); // 実行時エラー通知 (ESC の Break は通知しない)
r.is_led(); // LED 点灯状態 (枠表示などフロント側で反映)
```

`exec` / `run` / `loadProgram` は停止中のみ受け付ける。実行中に使えるのは `type` / `keyDown` / `break` など。

## ストレージと出力

- **SAVE/LOAD/FILES**: `persist=true` なら localStorage に保存する (リロード後も残る)。`false` ならページを閉じると消える。`storagePrefix` で複数インスタンスのスロットを分ける
- **onPrint**: 画面の変化から出力を近似的に流すので、取りこぼすことがある。確実な画面の内容は `getScreenText()` で取る

## GET / POST (HTTPS 通信)

ichigocrate 拡張の `GET` / `POST` ([core の説明](../core/README.md#get--post-https-通信)) は `fetch` で通信する。内部ネットワークへ届かないよう、独自ヘッダ `X-Ichigocrate` を付けて必ず CORS の事前確認を経由させる。

そのため、**接続先サーバーが CORS を許可していないと Network error になる**。接続先側で次のように応答する必要がある。

```
Access-Control-Allow-Origin: <ページのオリジン または *>
Access-Control-Allow-Headers: X-Ichigocrate, Content-Type
```

(`Content-Type` は POST を受けるときだけ必要)

[kidspod](https://kidspod.club/) は CORS を許可していないため、Web 版からは取得できない (デスクトップ版では取得できる)。

> [!WARNING]
> `onPrint` や `getScreenText()` の文字列には、サーバーの応答がそのまま含まれうる。ページに出すときは `textContent` を使い、`innerHTML` で挿入しないこと (XSS になる)。

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

英字は既定で大文字入力 (実機と同じ)。CapsLock に追従させるには、`on_key` の第 5 引数に `KeyboardEvent.getModifierState("CapsLock")` を渡す。

## 未対応

- 音声出力 (BEEP / PLAY は鳴らない)
