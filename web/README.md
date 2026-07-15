# ichigocrate-web

[ichigocrate-core](../core) を使った WebAssembly フロントエンド。eframe は使わず canvas 2D へ直接 blit する軽量構成。

```
web/
├── src/
│   ├── lib.rs        # クレート docs + モジュール宣言 + 再エクスポート
│   ├── runner.rs     # IchigoCrateRunner: core を直接駆動し canvas へ blit
│   ├── keymap.rs     # KeyboardEvent.code → HID/BTN コード変換
│   ├── output.rs     # onPrint 用の VRAM 差分ヘルパ
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

ビルド成果物 (`pkg/`) は ES モジュールとして `import` でき、React ラッパや CDN 配布の土台になる。wasm サイズは ~110KB (eframe を載せないため軽量)。

## 外部制御 API (`IchigoCrateHandle`)

`IchigoCrateRunner` は描画/キー入力に加え、JS/TS から実行・入力・状態取得を行う命令ハンドルを公開する (React ではこの面を `IchigoCrateHandle` という ref 型で露出する)。`core` の公開関数へ委譲する薄いブリッジで、実行中プログラムも外部から駆動できる。

```js
// storagePrefix と persist は省略可 (既定 ""/true)。
const r = new IchigoCrateRunner(canvas, "demo-1", true);
r.exec("PRINT 1+2"); // 1 行を直接実行 (停止中のみ)
r.loadProgram('10 ?"HI"\n20 GOTO 10');
r.run(); // RUN。無限ループはフレーム実行へ委譲
r.type("LIST\n"); // キーボード入力と同等 (実行中は INKEY/INPUT へ)
r.keyDown(28);
r.keyUp(28); // BTN()/INKEY() 用 物理キー (28=←)
r.stop(); // ESC 注入で中断
r.getScreenText(); // 画面スナップショット
r.getVar("A"); // 変数 A-Z
r.peek(0x900); // メモリ読み取り
r.onPrint((chunk) => (out += chunk)); // 画面出力ストリーミング購読
r.is_led(); // LED 点灯状態 (枠表示などフロント側で反映)
```

実行モデル上、無限ループが常態なので「`exec()` の戻りで完了を待つ」設計は採らず、即時文は同期完了・`RUN` 等はフレーム分割実行へ委譲する。実行中は `type`/`keyDown`/`stop` のみ有効 (`exec`/`run` は停止中のみ受理) で、フレーム途中に割り込まない。

## ストレージと出力

- **SAVE/LOAD/FILES**: `core::machine::Storage` を Web 実装 (`WebStorage`) で差し替え。`persist=true` は localStorage (`{prefix}slot_NN` に base64 保存)、`persist=false` はセッション内のみの揮発メモリ。`storagePrefix` で複数インスタンスのスロットを分離する (同一オリジンでの共有を防ぐ)。リロード後もスロット内容は残る。
- **onPrint**: core を改変せず VRAM 差分で画面出力を近似ストリーミングする。1 フレーム内に画面外へスクロールし切った行や LOCATE で上書きした出力は取りこぼし得るため、確実な全画面状態は `getScreenText()` を併用する。

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

## 未対応

- 音声出力 (BEEP/PLAY は core 側で音程を計算するが、本フロントエンドは Web Audio 未接続)
