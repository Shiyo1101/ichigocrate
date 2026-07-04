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

- BASIC コア言語 (PRINT, LET, IF/THEN/ELSE, FOR/NEXT, GOTO/GOSUB/RETURN, GOSUB/RTN/GSB エイリアス, REM, INPUT, LIST, NEW, RUN, END, STOP, CONT, RENUM, HELP, OK, CLV, CLS, CLT, CLK, CLP, CLO, LED, OUT (no-op), POKE, COPY, LOCATE, SCROLL, WAIT, DRAW, BEEP, PLAY, TEMPO, SRND, VIDEO)
- INPUT — 対話入力対応。プロンプト (`"文字列",` または既定の `?`) を表示して実行を中断し、1 行入力を式として評価して変数/配列要素へ代入する。ホストは `Machine::is_awaiting_input` で待機を検知し、確定行を `input_complete` へ渡す。空入力やパース不能な入力は代入をスキップし、変数は元の値を保つ
- RENUM — 行番号の振り直しに加え、GOTO/GOSUB の数値リテラル参照も新しい行番号へ書き換える。新番号の桁数が元の桁数を超えて行内に収まらない場合は `Illegal argument`
- 式評価 (算術、ビット演算、論理演算、比較、優先順位 5 段階)
- 関数 (ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS, SOUND, ANA (no-op), BTN (キーボード代用), IN (no-op), SCR, VPEEK, POINT, CHR$/STR$/DEC$/HEX$/BIN$, SIN/COS, USR (no-op))
- LIST 領域への行編集・削除・LIST 表示・RUN
- ストレージ (SAVE / LOAD / LRUN / FILES) — `core::machine::Storage` トレイトで実装をホスト側に委譲
- PSG 音源 (BEEP / PLAY MML, テンポ・オクターブ・付点)
- 日本語フォント (ichigojam-jp.fnt)
- VRAM 文字描画 + PCG (書換可能キャラクタ) + ピクセル描画 (DRAW)
- WAIT (`Machine.wait_frames` の加算。実時間への変換はホスト側の責務)
- 大文字自動変換 (CAPS デフォルト ON)
- 行編集は挿入モード (IchigoJam 標準。文字間にカーソルを置いて入力すると後続文字を上書きせず挿入)。プログラム実行中の画面出力は上書きモード
- 挿入モードのカーソル縦移動はテキストエディタ風: ↑↓ で移動先の列が空白なら、その行のテキスト末尾 (なければ 0 列) へ引き戻す。上書きモードは実機同様に自由移動
- カーソル形状も実機準拠: 挿入モードは文字セルの左半分 (4px)、上書きモードは文字全体 (8px) を反転表示
- プログラム実行中はカーソルを非表示にし、キー入力による画面編集・カーソル移動も無効化する (IchigoJam 標準。INKEY()/BTN() 用のキー取得は継続)。プログラム側の LOCATE x,y,1 による明示的なカーソル表示は可能
- VIDEO モード切替 (0:オフ 1:通常 2:反転 3:拡大 4:拡大反転)。拡大時は論理画面サイズ自体が `32/24 >> 拡大段階` に縮み (16x12 / 8x6 / 4x3)、折り返し位置やカーソル可動範囲も倍率に追従する
- ESC によるブレーク
- LED コマンド (状態を Machine に保持。実際の点灯表現はフロントエンドの責務)
- ローマ字 → 半角カナ変換 (JIS X 0201 カタカナ出力)。変換規則は本家 IchigoJam BASIC と同じ:
  - `KA` → カ、`KYA` → キャ、`SHI` → シ、`CHA` → チャ、`TSU` → ツ
  - `NN` → ン、`KKA` → ッカ、`XTU` / `LTU` → ッ
  - `GA` → ガ (カ+゛)、`PA` → パ (ハ+゜)
  - `.` `,` `-` `[` `]` `/` `\` は対応するカタカナ記号にマップ
- BTN() — 実機ボタンの代わりにキーボードの押下状態を返す。`BTN()` / `BTN(0)` は本体ボタン相当でデスクトップでは常に 0。`BTN(n)` は ASCII コード `n` のキーが押下中なら 1 (28:← 29:→ 30:↑ 31:↓ 32:スペース 88:X、英字 A-Z / 数字 0-9 も可)。`BTN(-1)` は押下中キーのビットマスク (bit0:← 1:→ 2:↑ 3:↓ 4:スペース 5:X)
- KBD コマンド (Ver1.5) — `KBD n` でキーボードレイアウト ID を切替 (0:US、それ以外:JA)。デフォルトは JA (日本語配列)。`keymap` が引く表が即時切替わり、物理キー位置からの文字解釈が US/JA で変わる (例: 日本語キーボードで Shift+2 を打つと `KBD 0` では `@`、`KBD 1` では `"`)。現在値は `VER(2)` で参照可能。実機はフラッシュへ永続化するが本移植はメモリ内のみ。トークン番号も v1.5 仕様 (KBD=126, DAC=127, IOT.OUT=128, ...) に準拠

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
