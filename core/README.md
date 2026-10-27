# ichigocrate-core

IchigoJam BASIC インタプリタ本体。REPL・プログラム実行・INPUT 対話・WAIT といった実行状態の管理まで core に持ち、画面の色付け・キーイベントの変換・音声出力・ファイル保存先といったホスト依存の処理はフロントエンド ([app](../app), [web](../web)) に委譲する。

## ディレクトリ構成

```
core/
├── src/
│   ├── lib.rs           # 公開 API + exec_line / exec_line_bytes
│   ├── machine.rs       # Machine 構造体 (元 GLOBAL + RAM) と Storage トレイト
│   ├── session.rs       # ホスト共通の実行状態機械 (REPL / RUN / INPUT / WAIT / 60Hz tick)
│   ├── basic.rs         # 実行ループと文のディスパッチ
│   ├── basic/           # tokenizer / expr (式評価) / commands (各文) / sin
│   ├── screen.rs        # VRAM 操作 + ピクセル描画
│   ├── render.rs        # VRAM → 1bpp 画面ビットマップ (フロント非依存)
│   ├── psg.rs           # MML プレイヤ
│   ├── ram.rs           # RAM レイアウト定数
│   ├── keycodes.rs      # 制御コード/キーコード定数 (画面・入力・BTN 共通)
│   ├── keymap.rs        # HID キーコード → ASCII 変換表 (US/JA)
│   ├── romajikana.rs    # ローマ字 → 半角カナ変換
│   ├── tokens.rs        # トークン定義 (v1.4.3)
│   ├── errors.rs        # BASIC エラー型 (IchigoJam 標準のエラー番号互換)
│   └── font.rs          # 日本語フォント (256 文字 × 8x8)
└── tests/                # 機能別結合テスト (common ヘルパー共有)
```

## 移植範囲

**実装済み**

BASIC コア言語のコマンド一覧:

| コマンド         | 備考                                              |
| ---------------- | ------------------------------------------------- |
| PRINT            | `?` エイリアスあり                                |
| LET              |                                                   |
| IF / THEN / ELSE |                                                   |
| FOR / NEXT       |                                                   |
| GOTO             |                                                   |
| GOSUB / RETURN   | `GSB` / `RTN` エイリアスあり                      |
| REM              | `'` エイリアスあり                                |
| INPUT            | 対話入力対応 (下記参照)                           |
| LIST             |                                                   |
| NEW              | プログラムのみクリア (変数は残る)                 |
| RUN              |                                                   |
| END / STOP       |                                                   |
| CONT             |                                                   |
| RENUM            | GOTO/GOSUB 参照も書き換え                         |
| RESET            | 電源 ON/OFF による再起動相当 (下記参照)           |
| HELP             | メモリマップを表示                                |
| OK               | `OK 2` で応答メッセージを抑制                     |
| CLV              | 変数クリア。`CLEAR` エイリアスあり                |
| CLS              | 画面クリア                                        |
| CLT              | `TICK()` カウンタクリア                           |
| CLK              |                                                   |
| CLP              | PCG をフォントへ初期化                            |
| CLO              | no-op                                             |
| LOAD             |                                                   |
| SAVE             |                                                   |
| LRUN             | LOAD 直後に RUN                                   |
| FILES            |                                                   |
| LED              | 状態を `Machine` に保持。点灯表現はホスト側の責務 |
| OUT              | no-op                                             |
| POKE             |                                                   |
| COPY             |                                                   |
| LOCATE           | `LC` エイリアスあり                               |
| SCROLL           |                                                   |
| WAIT             | 実時間での待機は `Session` が管理 (下記参照)      |
| DRAW             |                                                   |
| BEEP             |                                                   |
| PLAY             | MML                                               |
| TEMPO            |                                                   |
| SRND             |                                                   |
| VIDEO            | モード切替 (下記参照)                             |
| KBD              | キーボードレイアウト切替 (Ver1.5、下記参照)       |

実装メモ (移植・ホスト境界に関わる点のみ):

- 関数: ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS, SOUND, ANA (no-op), BTN, IN (no-op), SCR, VPEEK, POINT, CHR$/STR$/DEC$/HEX$/BIN$, SIN/COS, USR (no-op)
- RESET — `Machine::power_on_reset` に委譲。`basic_init` と異なり LED・カナ入力・VIDEO 設定・PSG・乱数シードなど、電源断で失われるハードウェア相当の状態も含めて再起動する (SAVE/LOAD のストレージのみフラッシュ相当として保持)。ホストが提供する外部リセット API ([web](../web) の `IchigoCrateHandle.reset()` 等) も同じ関数に委譲する
- INPUT — `Session` 経由なら Enter (`on_enter`) で値が確定し、ESC (`on_escape`) で中断される。`Machine` を直接使う場合は `is_awaiting_input` で入力待ちを検知し、確定行を `input_complete` へ渡して実行を再開する
- VIDEO — 拡大時は `screen_cols()`/`screen_rows()` 自体が縮む (`32/24 >> 拡大段階`) ため、折り返し・カーソル可動範囲の計算はホスト側もこの値を参照する必要がある
- KBD — 実機はフラッシュへ永続化するが本移植はメモリ内のみ (`Machine::new()` の既定は JA)。`VER(2)` で現在値を返す

**未実装 (スコープ外)**

- IoT 拡張 (IOT.IN / IOT.OUT)
- 多言語フォント (中国語、ベトナム語、モンゴル語)
- I2C 通信
- UART
- PWM / DAC コマンド
- SLEEP / SWITCH / BPS / WS.LED
- VIDEO の clkdiv 引数 (省電力時のクロック分周は実機固有のため読み飛ばし)

## テスト

```bash
cargo test -p ichigocrate-core
```

テストは `tests/` 以下に機能別のファイルとして置き (式評価・エラー・制御構文・コマンド・画面編集・描画・セッションなど)、共通ヘルパーは `tests/common/mod.rs` にまとめている。キー変換表 (`keymap.rs`) やローマ字カナ変換 (`romajikana.rs`) の単体テストは各モジュール内にある。

## アーキテクチャ要点

- 元 C コードのグローバル変数 (struct GLOBAL `_g` と RAM 領域) はすべて `Machine` 構造体に集約している。
- PC は仮想アドレスではなく `Machine.ram` への `usize` インデックスとして扱う。仮想アドレス変換は `OFFSET_RAMROM` (0x700) を加算するだけ。
- BASIC インタプリタは `basic_step()` で 1 文ずつ実行できる協調的設計。`basic_execute()` は RUN / GOTO で PC が LIST 領域へ移行した時点で呼出元に制御を返し、以降は `basic_step` を繰り返し呼んで実行を進める。
- ホストは `session::Session` を通して VM を動かす。キー入力は `feed_key` / `feed_char` / `on_enter` / `on_escape` / `press_fkey` で渡し、毎フレーム `tick(now_ms)` に現在時刻 (ミリ秒) を渡す。`tick` は 1 フレームあたりの実行文数を制限してプログラムを進め (無限ループでも UI を止めない)、`TICK()` カウンタと PSG を実時間の 60Hz に合わせて進める。
- WAIT は `Machine.wait_frames` を加算し、`Session::tick` がそれを実時間の期限に変換して待つ。フレームレートに依存しないため、高リフレッシュレート環境でも WAIT の長さは変わらない。
- `Session` を使わずに `exec_line` / `run_to_completion` で `Machine` を直接動かすこともできる (テストやヘッドレス用途。`run_to_completion` は WAIT を無視する)。
- ストレージは `machine::Storage` トレイトで抽象化している。ホストはこのトレイトを実装してファイル保存先を差し替える (未設定時の SAVE/LOAD は File error)。
- 描画は `render::render_mono` が VRAM/PCG を 1bpp ビットマップへ展開するところまでを担い、色付けはホスト側で行う。
