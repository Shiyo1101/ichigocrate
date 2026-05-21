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
| F8   | `FILES` を即時実行                               |
| ESC  | プログラム中断 (Break)                           |

英字は常に大文字入力 (CAPS デフォルト ON、IchigoJam 慣習)。

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
  PLAY, TEMPO, SRND)
- 式評価 (算術、ビット演算、論理演算、比較、優先順位 5 段階)
- 関数 (ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS,
  SOUND, ANA (no-op), BTN (no-op), IN (no-op), SCR, VPEEK, POINT,
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
- F1-F8 ショートカット
- cpal 矩形波音声出力
- ESC によるブレーク
- macOS の IMK 関連ログ抑制 (stderr フィルタ)
- LED コマンド (実機 LED の代わりに画面の枠線を赤く点灯。`LED 0` で消灯)

**未実装 (スコープ外)**

- IoT 拡張 (IOT.IN / IOT.OUT)
- ローマ字かな変換
- 多言語フォント (中国語、ベトナム語、モンゴル語)
- I2C 通信
- USB キーボード固有のキーコード変換
- UART
- PWM / DAC / KBD コマンド
- VIDEO モード切替 (拡大表示)

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
