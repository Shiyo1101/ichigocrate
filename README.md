# ichigojam-rs

IchigoJam BASIC (子供向け教育用コンピュータ) の C ファームウェアを Rust に書き換え、デスクトップ向け GUI アプリ化したもの。

## 構成

```
ichigojam-rs/
├── core/                 # no_std 可能な BASIC インタプリタ本体
│   ├── src/
│   │   ├── lib.rs        # エクスポート + exec_line
│   │   ├── machine.rs    # Machine 構造体 (元 GLOBAL + RAM)
│   │   ├── basic.rs      # トークナイザ + 式評価 + 文実行
│   │   ├── screen.rs     # VRAM 操作 + ピクセル描画
│   │   ├── render.rs     # VRAM → 1bpp 画面ビットマップ (フロント非依存)
│   │   ├── psg.rs        # MML プレイヤ
│   │   ├── ram.rs        # RAM レイアウト定数
│   │   ├── keycodes.rs   # 制御コード/キーコード定数 (画面・入力・BTN 共通)
│   │   ├── keymap.rs     # HID キーコード → ASCII 変換表 (US/JA)
│   │   ├── tokens.rs     # トークン定義 (v1.4.3)
│   │   ├── errors.rs     # エラーメッセージ
│   │   └── font.rs       # 日本語フォント (256 文字 × 8x8)
│   └── tests/            # 機能別結合テスト (common ヘルパー共有)
├── app/                  # egui デスクトップフロントエンド
│   └── src/main.rs
└── web/                  # WebAssembly フロントエンド (eframe 不使用・軽量)
    ├── src/
    │   ├── lib.rs        # クレート docs + モジュール宣言 + 再エクスポート
    │   ├── runner.rs     # IchigoJamRunner: core を直接駆動し canvas へ blit
    │   ├── keymap.rs     # KeyboardEvent.code → HID/BTN コード変換
    │   ├── output.rs     # onPrint 用の VRAM 差分ヘルパ
    │   └── storage.rs    # WebStorage: SAVE/LOAD/FILES の localStorage 実装
    ├── build.sh          # wasm ビルド + wasm-bindgen (--target web)
    └── demo/
        ├── index.html    # 単体デモページ (マークアップ + スタイル)
        └── main.js       # wasm 初期化・rAF ループ・キーイベント配線
```

`core` の描画は `render.rs` (1bpp ビットマップ生成) に集約され、デスクトップ
(egui) と Web (canvas 2D) の両フロントエンドが同じラスタライズ結果を共有する。

## ビルドと実行

### デスクトップ (ネイティブ)

```bash
cargo run --release -p ichigojam-app
```

### Web (WebAssembly)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.121   # Cargo.lock と同一版
cd ichigojam-rs/web && ./build.sh                  # pkg/ に wasm + JS グルー生成
python3 -m http.server                             # http://localhost:8000/demo/
```

ビルド成果物 (`pkg/`) は ES モジュールとして `import` でき、React ラッパや CDN
配布の土台になる。wasm サイズは ~110KB (eframe を載せないため軽量)。

#### 外部制御 API (`IchigoJamHandle`)

`IchigoJamRunner` は描画/キー入力に加え、JS/TS から実行・入力・状態取得を行う命令
ハンドルを公開する (React ではこの面を `IchigoJamHandle` という ref 型で露出する)。
`core` の公開関数へ委譲する薄いブリッジで、実行中プログラムも外部から駆動できる。

```js
// storagePrefix と persist は省略可 (既定 ""/true)。
const r = new IchigoJamRunner(canvas, "demo-1", true);
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

実行モデル上、無限ループが常態なので「`exec()` の戻りで完了を待つ」設計は採らず、
即時文は同期完了・`RUN` 等はフレーム分割実行へ委譲する。実行中は `type`/`keyDown`/
`stop` のみ有効 (`exec`/`run` は停止中のみ受理) で、フレーム途中に割り込まない。

#### ストレージと出力

- **SAVE/LOAD/FILES**: `core::machine::Storage` を Web 実装 (`WebStorage`) で差し替え。
  `persist=true` は localStorage (`{prefix}slot_NN` に base64 保存)、`persist=false`
  はセッション内のみの揮発メモリ。`storagePrefix` で複数インスタンスのスロットを
  分離する (同一オリジンでの共有を防ぐ)。リロード後もスロット内容は残る。
- **onPrint**: core を改変せず VRAM 差分で画面出力を近似ストリーミングする。1 フレーム
  内に画面外へスクロールし切った行や LOCATE で上書きした出力は取りこぼし得るため、
  確実な全画面状態は `getScreenText()` を併用する。

### キーボードショートカット (IchigoJam 標準準拠)

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

英字は常に大文字入力 (CAPS デフォルト ON、IchigoJam 慣習)。

カナモード ON のあいだはウィンドウタイトルに `KANA` が表示される。変換規則は本家 IchigoJam BASIC と同じで、半角カタカナ (JIS X 0201, 0xA1-0xDF) のみを扱う。例:

- `KA` → カ、`KYA` → キャ、`SHI` → シ、`CHA` → チャ、`TSU` → ツ
- `NN` → ン、`KKA` → ッカ、`XTU` / `LTU` → ッ
- `GA` → ガ (カ+゛)、`PA` → パ (ハ+゜)
- `.` `,` `-` `[` `]` `/` `\` は対応するカタカナ記号にマップ

### ファイル保存先

`SAVE n` / `LOAD n` / `FILES` はホスト OS の `~/.ichigojam-rs/slot_NN.ijb` (NN はスロット番号 0-15) に LIST 領域のバイナリを直接読み書きする。

## 移植範囲

**実装済み**

- BASIC コア言語 (PRINT, LET, IF/THEN/ELSE, FOR/NEXT, GOTO/GOSUB/RETURN, GOSUB/RTN/GSB エイリアス, REM, INPUT, LIST, NEW, RUN, END, STOP, CONT, RENUM, HELP, OK, CLV, CLS, CLT, CLK, CLP, CLO, LED, OUT (no-op), POKE, COPY, LOCATE, SCROLL, WAIT, DRAW, BEEP, PLAY, TEMPO, SRND, VIDEO)
- INPUT — 対話入力対応。プロンプト (`"文字列",` または既定の `?`) を表示して実行を中断し、1 行入力を式として評価して変数/配列要素へ代入する。ホストは `Machine::is_awaiting_input` で待機を検知し、確定行を `input_complete` へ渡す。空入力やパース不能な入力は代入をスキップし、変数は元の値を保つ
- RENUM — 行番号の振り直しに加え、GOTO/GOSUB の数値リテラル参照も新しい行番号へ書き換える。新番号の桁数が元の桁数を超えて行内に収まらない場合は `Illegal argument`
- 式評価 (算術、ビット演算、論理演算、比較、優先順位 5 段階)
- 関数 (ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS, SOUND, ANA (no-op), BTN (キーボード代用), IN (no-op), SCR, VPEEK, POINT, CHR$/STR$/DEC$/HEX$/BIN$, SIN/COS, USR (no-op))
- LIST 領域への行編集・削除・LIST 表示・RUN
- ストレージ (SAVE / LOAD / LRUN / FILES) — ホスト側は `Storage` トレイトで実装を差し替え可能。デスクトップアプリは `~/.ichigojam-rs/slot_NN.ijb` に読み書き
- PSG 音源 (BEEP / PLAY MML, テンポ・オクターブ・付点)
- 日本語フォント (ichigojam-jp.fnt)
- VRAM 文字描画 + PCG (書換可能キャラクタ) + ピクセル描画 (DRAW)
- 実時間ベースの WAIT (ディスプレイ周波数に依存せず正確に N/60 秒待機)
- egui キーボード入力 — 物理キー位置 (`Event::Key.physical_key`) を USB HID Usage ID へ変換し、`keymap` の US/JA 表 (元 C ファーム `hid.h` 由来) を引いて IchigoJam 内部コードに翻訳する。OS のレイアウト変換を経由しないため `KBD` コマンドの効果が実入力に反映される
- 大文字自動変換 (CAPS デフォルト ON)
- 行編集は挿入モード (IchigoJam 標準。文字間にカーソルを置いて入力すると後続文字を上書きせず挿入)。プログラム実行中の画面出力は上書きモード
- 挿入モードのカーソル縦移動はテキストエディタ風: ↑↓ で移動先の列が空白なら、その行のテキスト末尾 (なければ 0 列) へ引き戻す。上書きモードは実機同様に自由移動
- カーソル形状も実機準拠: 挿入モードは文字セルの左半分 (4px)、上書きモードは文字全体 (8px) を反転表示
- プログラム実行中はカーソルを非表示にし、キー入力による画面編集・カーソル移動も無効化する (IchigoJam 標準。INKEY()/BTN() 用のキー取得は継続)。プログラム側の LOCATE x,y,1 による明示的なカーソル表示は可能
- F1-F9 ショートカット
- VIDEO モード切替 (0:オフ 1:通常 2:反転 3:拡大 4:拡大反転)。拡大時は論理画面サイズ自体が `32/24 >> 拡大段階` に縮み (16x12 / 8x6 / 4x3)、折り返し位置やカーソル可動範囲も倍率に追従する
- cpal 矩形波音声出力
- ESC によるブレーク
- macOS の IMK 関連ログ抑制 (stderr フィルタ)
- LED コマンド (実機 LED の代わりに画面の枠線を赤く点灯。`LED 0` で消灯)
- ローマ字 → 半角カナ変換 (F10 で切替、JIS X 0201 カタカナ出力)
- BTN() — 実機ボタンの代わりにキーボードの押下状態を返す。`BTN()` / `BTN(0)` は本体ボタン相当でデスクトップでは常に 0。`BTN(n)` は ASCII コード `n` のキーが押下中なら 1 (28:← 29:→ 30:↑ 31:↓ 32:スペース 88:X、英字 A-Z / 数字 0-9 も可)。`BTN(-1)` は押下中キーのビットマスク (bit0:← 1:→ 2:↑ 3:↓ 4:スペース 5:X)
- KBD コマンド (Ver1.5) — `KBD n` でキーボードレイアウト ID を切替 (0:US、それ以外:JA)。デフォルトは JA (日本語配列)。`keymap` が引く表が即時切替わり、物理キー位置からの文字解釈が US/JA で変わる (例: 日本語キーボードで Shift+2 を打つと `KBD 0` では `@`、`KBD 1` では `"`)。現在値は `VER(2)` で参照可能。実機はフラッシュへ永続化するが本移植はメモリ内のみ。トークン番号も v1.5 仕様 (KBD=126, DAC=127, IOT.OUT=128, ...) に準拠

**未実装 (スコープ外)**

- IoT 拡張 (IOT.IN / IOT.OUT)
- 多言語フォント (中国語、ベトナム語、モンゴル語)
- I2C 通信
- UART
- PWM / DAC コマンド
- VIDEO の clkdiv 引数 (省電力時のクロック分周は実機固有のため読み飛ばし)

## 動作確認テスト

```bash
cargo test -p ichigojam-core
```

テストは機能ごとにファイル分割し、共通ヘルパー (`screen_text` / `vram_line` / `var`) は `tests/common/mod.rs` に集約している (計 93 件)。

- `tests/print.rs` (9 件): PRINT 出力と数値/文字列フォーマット (HEX$/BIN$/DEC$)
- `tests/editor.rs` (15 件): カーソル移動・編集モード・行編集/LIST・VIDEO モード切替・KBD の US/JA テーブル切替
- `tests/graphics.rs` (15 件): グラフィック文字 (128-255) のバイト保持・CHR$ 境界・POKE→VPEEK の全バイト範囲ラウンドトリップ
- `tests/render.rs` (5 件): VRAM→1bpp ビットマップ展開・反転・VIDEO オフ・カーソル (全角/挿入半角) の描画
- `tests/commands.rs` (17 件): 代入/CLS/BTN/NEW/CLV/CLK/CLT/CLP/LED/SRND/SCROLL/COPY/BEEP/PLAY/OK の挙動
- `tests/control_flow.rs` (7 件): FOR/NEXT・GOTO/IF・@LABEL ジャンプ・END/STOP・CONT
- `tests/input.rs` (5 件): INPUT の対話入力 (入力待ち・式評価・空入力・中断)
- `tests/renum.rs` (4 件): RENUM の再採番と GOTO/GOSUB 参照書換
- `tests/programs.rs` (16 件): GOSUB/RETURN、フィボナッチ、ネスト IF/ELSE、WAIT+GOTO 協調的待機、SAVE/LOAD ラウンドトリップ (グラフィック文字含む)、LIST 表示でのバイト透過、DRAW の引数別経路、PEEK/POKE

## アーキテクチャ要点

- 元 C コードのグローバル変数 (struct GLOBAL `_g` と RAM 領域) はすべて `Machine` 構造体に集約した。
- PC は仮想アドレスではなく `Machine.ram` への `usize` インデックスとして扱う。仮想アドレス変換は `OFFSET_RAMROM` (0x700) を加算するだけ。
- BASIC インタプリタは `basic_step()` で 1 文ずつ実行できる協調的設計。`basic_execute()` は RUN / GOTO で PC が LIST 領域へ移行した時点で呼出元 (UI アプリ) に制御を返し、以降は毎フレーム `basic_step` をチャンク実行する。
- WAIT は `Machine.wait_frames` を加算するだけで、実時間への変換 (60Hz) と期限管理は egui アプリ側で `Instant` を使って行う。これにより ProMotion (120Hz) など高リフレッシュレート環境でも WAIT が正しく動作する。
- ストレージは `core::machine::Storage` トレイトで抽象化。アプリは `DiskStorage` を実装し、ファイル番号ごとにバイナリファイルとして保存。
- 音声出力は `Machine.current_tone_hz: f32` を読み取り、cpal の callback で矩形波を生成。共有は `Arc<AtomicU32>` 経由。
- PSG MML tick も `Instant` ベースで 60Hz に同期し、テンポを保つ。
