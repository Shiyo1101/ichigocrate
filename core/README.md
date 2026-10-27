# ichigocrate-core

IchigoJam BASIC インタプリタ本体。REPL・プログラム実行・INPUT 対話・WAIT といった実行状態の管理まで core に持ち、画面の色付け・キーイベントの変換・音声出力・ファイル保存先といったホスト依存の処理はフロントエンド ([app](../app), [web](../web)) に委譲する。

## ディレクトリ構成

```
core/
├── src/
│   ├── lib.rs           # 公開 API + exec_line / exec_line_bytes
│   ├── machine.rs       # Machine 構造体 (元 GLOBAL + RAM) と Storage トレイト
│   ├── session.rs       # ホスト共通の実行状態機械 (REPL / RUN / INPUT / WAIT / GET / 60Hz tick)
│   ├── net.rs           # ichigocrate 拡張 GET: URL 検証・HttpClient トレイト・接続先の判定
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
│   ├── errors.rs        # BASIC エラー型 (IchigoJam 標準のエラー番号互換 + 拡張)
│   └── font.rs          # 日本語フォント (256 文字 × 8x8)
└── tests/                # 機能別結合テスト (common ヘルパー共有)
```

## 移植範囲

元ファームウェア ([ichigojam-firm](https://github.com/IchigoJam/ichigojam-firm) の `IchigoJam_BASIC`) との対応。

### 実装済み

- コマンド: PRINT (`?`), LET, IF / THEN / ELSE, FOR / NEXT, GOTO, GOSUB / RETURN (`GSB` / `RTN`), END / STOP, CONT, REM (`'`), INPUT, LIST, NEW, RUN, RENUM, RESET, HELP, OK, CLV (`CLEAR`), CLS, CLT, CLK, CLP, LOAD, SAVE, LRUN, FILES, LED, POKE, COPY, LOCATE (`LC`), SCROLL, WAIT, DRAW, BEEP, PLAY, TEMPO, SRND, VIDEO, KBD
- 関数: ABS, RND, PEEK, INKEY, TICK, FREE, VER, LEN, FILE, LINE, POS, SOUND, BTN, SCR, VPEEK, POINT, SIN, COS, ASC, CHR$ / STR$ / DEC$ / HEX$ / BIN$

移植・ホスト境界に関わる補足:

- RESET — `Machine::power_on_reset` に委譲。LED・カナ入力・VIDEO 設定・PSG・乱数シードなど電源断で失われる状態も初期化する (SAVE/LOAD のストレージのみ保持)。ホストの外部リセット API ([web](../web) の `IchigoCrateHandle.reset()` 等) も同じ関数を使う
- INPUT — `Session` 経由なら Enter (`on_enter`) で値が確定し、ESC (`on_escape`) で中断される。`Machine` を直接使う場合は `is_awaiting_input` で入力待ちを検知し、確定行を `input_complete` へ渡す
- WAIT — 実時間での待機は `Session` が管理する ([アーキテクチャ要点](#アーキテクチャ要点) 参照)。実機と同じく ESC で待機を打ち切って中断する
- LIST — 画面に収まらない量は、画面 1 枚分 (画面の行数 - 1) ごとに 40 フレーム (約 0.67 秒) 止めながら表示する。ESC で中断できる (実機と同じ)
- ASC — 元ファーム同様 PEEK の別名。文字列リテラルは自身のアドレスに評価されるため `ASC("A")` は 65 になる
- LED — 状態を `Machine` に保持するのみで、点灯の表現はホスト側の責務
- VIDEO — 拡大時は `screen_cols()` / `screen_rows()` 自体が縮む (`32/24 >> 拡大段階`) ため、ホスト側も折り返しやカーソル範囲の計算にこの値を使う。clkdiv 引数 (実機の省電力用クロック分周) は読み飛ばす
- KBD — 実機はフラッシュへ保存するが本移植はメモリ内のみ (既定は JA)。`VER(2)` で現在値を返す

### 未実装

実行すると Syntax error になる。

- コマンド: OUT, CLO, PWM, DAC, UART, BPS, SLEEP, IOT.OUT, WS.LED
- 関数: IN, ANA, USR, I2CR, I2CW, IOT.IN

## ichigocrate 独自拡張

### GET (HTTPS 通信)

```basic
GET "https://kidspod.club/mj/2245"
```

指定した URL へ HTTPS で GET を送り、届いたレスポンスを解釈せずにそのまま扱う。URL は文字列リテラルのほか、文字列を置いた RAM のアドレスでも渡せる。

- 即時実行 (REPL で直接打ったとき) — レスポンスをキー入力として流し込む。改行ごとに Enter を押した扱いになるので、行番号付きの行はプログラムとして登録され、それ以外の行はその場で実行される ([kidspod](https://kidspod.club/) の `/mj/<プログラムID>` からプログラムを取り込める)
- プログラム実行中 — レスポンスを画面へ出力し、次の文へ進む
- レスポンスが届くまで実行は止まる。ESC で中断できる (Break)
- CR (`\r`) は捨てる。HTTP のステータスが 404 などでも本文はそのまま扱う

制限と安全のための仕様:

- https のみ。`https://` で始まらない URL は Illegal argument
- 内部ネットワーク (ループバック・プライベート・リンクローカルなど) へはつながない。URL に直接書かれた IP アドレスや `localhost` は core が Illegal argument にし、名前解決後の検査は各フロントエンドが行う
- レスポンスは 2KB (`net::MAX_RESPONSE_BYTES`) まで、応答待ちは 10 秒 (`net::REQUEST_TIMEOUT_MS`) まで。超えた場合や通信に失敗した場合は Network error
- リダイレクトは追わず、Cookie・認証情報は送らない

通信そのものはホストが `net::HttpClient` を実装して `Machine::set_http_client` で設定する。未設定なら Network error。

### 拡張エラー

| 番号 | メッセージ    | 内容                                              |
| ---- | ------------- | ------------------------------------------------- |
| 13   | Network error | GET の通信失敗 (接続不可・タイムアウト・上限超過) |

## テスト

```bash
cargo test -p ichigocrate-core
```

テストは `tests/` 以下に機能別のファイルとして置き (式評価・エラー・制御構文・コマンド・画面編集・描画・セッション・通信など)、共通ヘルパーは `tests/common/mod.rs` にまとめている。キー変換表 (`keymap.rs`) やローマ字カナ変換 (`romajikana.rs`) の単体テストは各モジュール内にある。

## アーキテクチャ要点

- 元 C コードのグローバル変数 (struct GLOBAL `_g` と RAM 領域) はすべて `Machine` 構造体に集約している。
- PC は仮想アドレスではなく `Machine.ram` への `usize` インデックスとして扱う。仮想アドレス変換は `OFFSET_RAMROM` (0x700) を加算するだけ。
- BASIC インタプリタは `basic_step()` で 1 文ずつ実行できる協調的設計。`basic_execute()` は RUN / GOTO で PC が LIST 領域へ移行した時点で呼出元に制御を返し、以降は `basic_step` を繰り返し呼んで実行を進める。
- ホストは `session::Session` を通して VM を動かす。キー入力は `feed_key` / `feed_char` / `on_enter` / `on_escape` / `press_fkey` で渡し、毎フレーム `tick(now_ms)` に現在時刻 (ミリ秒) を渡す。`tick` は 1 フレームあたりの実行文数を制限してプログラムを進め (無限ループでも UI を止めない)、`TICK()` カウンタと PSG を実時間の 60Hz に合わせて進める。
- WAIT は `Machine.wait_frames` を加算し、`Session::tick` がそれを実時間の期限に変換して待つ。フレームレートに依存しないため、高リフレッシュレート環境でも WAIT の長さは変わらない。
- `Session` を使わずに `exec_line` / `run_to_completion` で `Machine` を直接動かすこともできる (テストやヘッドレス用途。`run_to_completion` は WAIT を無視する)。
- ストレージは `machine::Storage` トレイトで抽象化している。ホストはこのトレイトを実装してファイル保存先を差し替える (未設定時の SAVE/LOAD は File error)。
- 描画は `render::render_mono` が VRAM/PCG を 1bpp ビットマップへ展開するところまでを担い、色付けはホスト側で行う。
