# ichigojam-rs

IchigoJam BASIC (子供向け教育用コンピュータ) の C ファームウェアを Rust に
書き換え、デスクトップ向け GUI アプリ化したもの。

## 構成

```
ichigojam-rs/
├── core/                 # no_std 可能な BASIC インタプリタ本体
│   ├── src/
│   │   ├── lib.rs        # エクスポート + exec_line
│   │   ├── machine.rs    # Machine 構造体 (元 GLOBAL + RAM)
│   │   ├── basic.rs      # トークナイザ + 式評価 + 文実行
│   │   ├── screen.rs     # VRAM 操作 + ピクセル描画
│   │   ├── psg.rs        # MML プレイヤ
│   │   ├── ram.rs        # RAM レイアウト定数
│   │   ├── keycodes.rs   # 制御コード/キーコード定数 (画面・入力・BTN 共通)
│   │   ├── tokens.rs     # トークン定義 (v1.4.3)
│   │   ├── errors.rs     # エラーメッセージ
│   │   └── font.rs       # 日本語フォント (256 文字 × 8x8)
│   └── tests/            # smoke + programs テスト
└── app/                  # egui デスクトップフロントエンド
    └── src/main.rs
```

## ビルドと実行

```bash
cargo run --release -p ichigojam-app
```

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

カナモード ON のあいだはウィンドウタイトルに `KANA` が表示される。
変換規則は本家 IchigoJam BASIC と同じで、半角カタカナ (JIS X 0201,
0xA1-0xDF) のみを扱う。例:

- `KA` → カ、`KYA` → キャ、`SHI` → シ、`CHA` → チャ、`TSU` → ツ
- `NN` → ン、`KKA` → ッカ、`XTU` / `LTU` → ッ
- `GA` → ガ (カ+゛)、`PA` → パ (ハ+゜)
- `.` `,` `-` `[` `]` `/` `\` は対応するカタカナ記号にマップ

### ファイル保存先

`SAVE n` / `LOAD n` / `FILES` はホスト OS の
`~/.ichigojam-rs/slot_NN.ijb` (NN はスロット番号 0-15) に
LIST 領域のバイナリを直接読み書きする。

## 移植範囲

**実装済み**

- BASIC コア言語 (PRINT, LET, IF/THEN/ELSE, FOR/NEXT, GOTO/GOSUB/RETURN,
  GOSUB/RTN/GSB エイリアス, REM, INPUT (簡易), LIST, NEW, RUN, END,
  STOP, CONT, RENUM (簡易), HELP, OK, CLV, CLS, CLT, CLK, CLP,
  CLO, LED, OUT (no-op), POKE, COPY, LOCATE, SCROLL, WAIT, DRAW, BEEP,
  PLAY, TEMPO, SRND, VIDEO)
- 式評価 (算術、ビット演算、論理演算、比較、優先順位 5 段階)
- 関数 (ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS,
  SOUND, ANA (no-op), BTN (キーボード代用), IN (no-op), SCR, VPEEK, POINT,
  CHR$/STR$/DEC$/HEX$/BIN$, SIN/COS, USR (no-op))
- LIST 領域への行編集・削除・LIST 表示・RUN
- ストレージ (SAVE / LOAD / LRUN / FILES) — ホスト側は
  `Storage` トレイトで実装を差し替え可能。デスクトップアプリは
  `~/.ichigojam-rs/slot_NN.ijb` に読み書き
- PSG 音源 (BEEP / PLAY MML, テンポ・オクターブ・付点)
- 日本語フォント (ichigojam-jp.fnt)
- VRAM 文字描画 + PCG (書換可能キャラクタ) + ピクセル描画 (DRAW)
- 実時間ベースの WAIT (ディスプレイ周波数に依存せず正確に N/60 秒待機)
- egui キーボード入力 (テキスト + 矢印 + Backspace + Delete + Home/End)
- 大文字自動変換 (CAPS デフォルト ON)
- 行編集は挿入モード (IchigoJam 標準。文字間にカーソルを置いて入力すると
  後続文字を上書きせず挿入)。プログラム実行中の画面出力は上書きモード
- 挿入モードのカーソル縦移動はテキストエディタ風: ↑↓ で移動先の列が
  空白なら、その行のテキスト末尾 (なければ 0 列) へ引き戻す。上書きモードは
  実機同様に自由移動
- カーソル形状も実機準拠: 挿入モードは文字セルの左半分 (4px)、上書き
  モードは文字全体 (8px) を反転表示
- プログラム実行中はカーソルを非表示にし、キー入力による画面編集・カーソル
  移動も無効化する (IchigoJam 標準。INKEY()/BTN() 用のキー取得は継続)。
  プログラム側の LOCATE x,y,1 による明示的なカーソル表示は可能
- F1-F9 ショートカット
- VIDEO モード切替 (0:オフ 1:通常 2:反転 3:拡大 4:拡大反転)。拡大時は
  論理画面サイズ自体が `32/24 >> 拡大段階` に縮み (16x12 / 8x6 / 4x3)、
  折り返し位置やカーソル可動範囲も倍率に追従する
- cpal 矩形波音声出力
- ESC によるブレーク
- macOS の IMK 関連ログ抑制 (stderr フィルタ)
- LED コマンド (実機 LED の代わりに画面の枠線を赤く点灯。`LED 0` で消灯)
- ローマ字 → 半角カナ変換 (F10 で切替、JIS X 0201 カタカナ出力)
- BTN() — 実機ボタンの代わりにキーボードの押下状態を返す。
  `BTN()` / `BTN(0)` は本体ボタン相当でデスクトップでは常に 0。
  `BTN(n)` は ASCII コード `n` のキーが押下中なら 1
  (28:← 29:→ 30:↑ 31:↓ 32:スペース 88:X、英字 A-Z / 数字 0-9 も可)。
  `BTN(-1)` は押下中キーのビットマスク
  (bit0:← 1:→ 2:↑ 3:↓ 4:スペース 5:X)
- KBD コマンド (Ver1.5) — `KBD n` でキーボードレイアウト ID を切替
  (0:US、それ以外:JA)。実機はフラッシュへ永続化するが本移植はメモリ内のみ。
  現在値は `VER(2)` で参照可能。トークン番号も v1.5 仕様
  (KBD=126, DAC=127, IOT.OUT=128, ...) に準拠

**未実装 (スコープ外)**

- IoT 拡張 (IOT.IN / IOT.OUT)
- 多言語フォント (中国語、ベトナム語、モンゴル語)
- I2C 通信
- USB キーボード固有のキーコード変換
- UART
- PWM / DAC コマンド
- VIDEO の clkdiv 引数 (省電力時のクロック分周は実機固有のため読み飛ばし)

## 動作確認テスト

```bash
cargo test -p ichigojam-core
```

- `tests/smoke.rs` (8 件): 単純な構文 (PRINT, FOR, IF/GOTO, CLS, LIST,
  HEX/BIN, 変数, 行編集)
- `tests/programs.rs` (6 件): GOSUB/RETURN、フィボナッチ、ネスト IF/ELSE、
  WAIT+GOTO 協調的待機、SAVE/LOAD ラウンドトリップ、PEEK/POKE

## アーキテクチャ要点

- 元 C コードのグローバル変数 (struct GLOBAL `_g` と RAM 領域) はすべて
  `Machine` 構造体に集約した。
- PC は仮想アドレスではなく `Machine.ram` への `usize` インデックスとして
  扱う。仮想アドレス変換は `OFFSET_RAMROM` (0x700) を加算するだけ。
- BASIC インタプリタは `basic_step()` で 1 文ずつ実行できる協調的設計。
  `basic_execute()` は RUN / GOTO で PC が LIST 領域へ移行した時点で
  呼出元 (UI アプリ) に制御を返し、以降は毎フレーム `basic_step` を
  チャンク実行する。
- WAIT は `Machine.wait_frames` を加算するだけで、実時間への変換 (60Hz)
  と期限管理は egui アプリ側で `Instant` を使って行う。これにより
  ProMotion (120Hz) など高リフレッシュレート環境でも WAIT が正しく
  動作する。
- ストレージは `core::machine::Storage` トレイトで抽象化。アプリは
  `DiskStorage` を実装し、ファイル番号ごとにバイナリファイルとして保存。
- 音声出力は `Machine.current_tone_hz: f32` を読み取り、cpal の callback
  で矩形波を生成。共有は `Arc<AtomicU32>` 経由。
- PSG MML tick も `Instant` ベースで 60Hz に同期し、テンポを保つ。
