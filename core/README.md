# ichigojam-core

IchigoJam BASIC インタプリタ本体。`no_std` 対応で、画面描画やキー入力、ファイル保存といったホスト依存の処理は外部 ([app](../app), [web](../web)) に委譲する。

## ディレクトリ構成

```
core/
├── src/
│   ├── lib.rs          # エクスポート + exec_line
│   ├── machine.rs       # Machine 構造体 (元 GLOBAL + RAM)
│   ├── basic/           # トークナイザ + 式評価 + 文実行
│   ├── screen.rs        # VRAM 操作 + ピクセル描画
│   ├── render.rs        # VRAM → 1bpp 画面ビットマップ (フロント非依存)
│   ├── psg.rs           # MML プレイヤ
│   ├── ram.rs           # RAM レイアウト定数
│   ├── keycodes.rs      # 制御コード/キーコード定数 (画面・入力・BTN 共通)
│   ├── keymap.rs        # HID キーコード → ASCII 変換表 (US/JA)
│   ├── romajikana.rs    # ローマ字 → 半角カナ変換
│   ├── tokens.rs        # トークン定義 (v1.4.3)
│   ├── errors.rs        # エラーメッセージ
│   └── font.rs          # 日本語フォント (256 文字 × 8x8)
└── tests/                # 機能別結合テスト (common ヘルパー共有)
```

## 移植範囲

**実装済み**

BASIC コア言語のコマンド一覧:

| コマンド         | 備考                                              |
| ---------------- | ------------------------------------------------- |
| PRINT            |                                                   |
| LET              |                                                   |
| IF / THEN / ELSE |                                                   |
| FOR / NEXT       |                                                   |
| GOTO             |                                                   |
| GOSUB / RETURN   | `GSB` / `RTN` エイリアスあり                      |
| REM              |                                                   |
| INPUT            | 対話入力対応 (下記参照)                           |
| LIST             |                                                   |
| NEW              | プログラムのみクリア (変数は残る)                 |
| RUN              |                                                   |
| END / STOP       |                                                   |
| CONT             |                                                   |
| RENUM            | GOTO/GOSUB 参照も書き換え                         |
| RESET            | 電源 ON/OFF による再起動相当 (下記参照)           |
| HELP             |                                                   |
| OK               |                                                   |
| CLV              | 変数クリア                                        |
| CLS              | 画面クリア                                        |
| CLT              | `TICK()` カウンタクリア                           |
| CLK              |                                                   |
| CLP              | PCG をフォントへ初期化                            |
| CLO              |                                                   |
| LOAD             |                                                   |
| SAVE             |                                                   |
| LRUN             | LOAD 直後に RUN                                   |
| FILES            |                                                   |
| LED              | 状態を `Machine` に保持。点灯表現はホスト側の責務 |
| OUT              | no-op                                             |
| POKE             |                                                   |
| COPY             |                                                   |
| LOCATE           |                                                   |
| SCROLL           |                                                   |
| WAIT             | `Machine.wait_frames` へ加算するのみ              |
| DRAW             |                                                   |
| BEEP             |                                                   |
| PLAY             | MML                                               |
| TEMPO            |                                                   |
| SRND             |                                                   |
| VIDEO            | モード切替 (下記参照)                             |
| KBD              | キーボードレイアウト切替 (Ver1.5、下記参照)       |

実装メモ (実機と同じ挙動の説明は省略し、移植・ホスト境界に関わる点のみ。WAIT/ストレージの詳細は [アーキテクチャ要点](#アーキテクチャ要点) 参照):

- 関数: ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS, SOUND, ANA (no-op), BTN, IN (no-op), SCR, VPEEK, POINT, CHR$/STR$/DEC$/HEX$/BIN$, SIN/COS, USR (no-op)
- RESET — `Machine::power_on_reset` に委譲。`basic_init` と異なり LED・カナ入力・VIDEO 設定・PSG・乱数シードなど、電源断で失われるハードウェア相当の状態も含めて再起動する (SAVE/LOAD のストレージのみフラッシュ相当として保持)。ホストが提供する外部リセット API ([web](../web) の `IchigoJamHandle.reset()` 等) も同じ関数に委譲する
- INPUT — `Machine::is_awaiting_input` でホストが入力待ちを検知し、確定行を `input_complete` へ渡すことで実行を再開する
- VIDEO — 拡大時は `screen_cols()`/`screen_rows()` 自体が縮む (`32/24 >> 拡大段階`) ため、折り返し・カーソル可動範囲の計算はホスト側もこの値を参照する必要がある
- KBD — 実機はフラッシュへ永続化するが本移植はメモリ内のみ (`Machine::new()` の既定は JA)

**未実装 (スコープ外)**

- IoT 拡張 (IOT.IN / IOT.OUT)
- 多言語フォント (中国語、ベトナム語、モンゴル語)
- I2C 通信
- UART
- PWM / DAC コマンド
- VIDEO の clkdiv 引数 (省電力時のクロック分周は実機固有のため読み飛ばし)

## テスト

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
- WAIT は `Machine.wait_frames` を加算するだけで、実時間への変換 (60Hz) と期限管理はホスト側の責務。これにより高リフレッシュレート環境でも WAIT が正しく動作する。
- ストレージは `core::machine::Storage` トレイトで抽象化。ホストはこのトレイトを実装してファイル保存先を差し替える。
- PSG MML tick はホスト側の時刻管理と同期させ、60Hz でテンポを保つ想定。
