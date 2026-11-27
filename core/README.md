# ichigocrate-core

IchigoJam BASIC インタプリタ本体。REPL・プログラム実行・INPUT 対話・WAIT といった実行状態の管理まで core に持ち、画面の色付け・キーイベントの変換・音声出力・ファイル保存先といったホスト依存の処理はフロントエンド ([app](../app), [web](../web)) に委譲する。

## ディレクトリ構成

```
core/
├── src/
│   ├── lib.rs           # 公開 API + exec_line / exec_line_bytes
│   ├── machine.rs       # Machine 構造体 (元 GLOBAL + RAM) と Storage トレイト
│   ├── session.rs       # ホスト共通の実行状態機械 (REPL / RUN / INPUT / WAIT / GET・POST / 60Hz tick)
│   ├── net.rs           # ichigocrate 拡張 GET・POST: URL 検証・HttpClient トレイト・接続先の判定
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

実機との違い:

- LED — 状態を持つだけで、点灯の表現 (画面の枠など) はフロントエンドが行う
- VIDEO — clkdiv 引数 (実機の省電力用クロック分周) は読み飛ばす
- KBD — 設定はメモリ内のみ (実機はフラッシュへ保存する)

### 未実装

実行すると Syntax error になる。

- コマンド: OUT, CLO, PWM, DAC, UART, BPS, SLEEP, IOT.OUT, WS.LED
- 関数: IN, ANA, USR, I2CR, I2CW, IOT.IN

## ichigocrate 独自拡張

### GET / POST (HTTPS 通信)

```basic
GET "https://kidspod.club/mj/2245"
POST "https://example.com/room/1","MOVE 3"
POST "https://example.com/room/1",#800,16
```

指定した URL へ HTTPS で GET / POST を送る。URL と本文は文字列か、それを置いた RAM のアドレスで渡す。`POST url,アドレス,長さ` なら RAM 上のバイト列をそのまま送る (2KB まで)。

レスポンスは解釈せずに次のように扱う。届くまで実行は止まり、ESC で中断できる。

- 即時実行 — キー入力として流し込む。行番号付きの行はプログラムとして登録される ([kidspod](https://kidspod.club/) の `/mj/<プログラムID>` から取り込める)
- プログラム実行中 — 画面へ出力する
- ステータスが 2xx 以外なら `HTTP 404` のように表示して Network error で止める

#### レスポンスをメモリへ書き込む

```basic
GET  url,書き込み先,最大長
POST url,"文字列",書き込み先,最大長
POST url,送信元,長さ,書き込み先,最大長
```

- 画面にもキー入力にも出さず、書き込み先へそのまま書き込む。空きがあれば後ろに NUL を付ける (`?STR$(#1200)` で表示できる)
- 書き込んだ長さは `NET()` で読める
- 最大長より長いレスポンスは書き込まず Out of memory で止める
- 書き込み先には **NET 領域 `#1200`〜`#19FF` (2KB, ichigocrate 独自)** を使うとよい。ゲームの変数や PCG と衝突せず、RESET でだけ消える

#### 待たずに進める (`NET GET` / `NET POST`)

```basic
NET GET  url,書き込み先,最大長
NET POST url,"文字列",書き込み先,最大長
NET POST url,送信元,長さ,書き込み先,最大長
```

- 頭に `NET` を付けると、レスポンスを待たずに次の文へ進む。書き込み先は必須
- `NET()` は通信中 -1、終われば書き込んだ長さ、失敗すれば -2。失敗してもプログラムは止めない
- リクエストは同時に 1 つだけ。通信中に次を出すと前のものは取り消される

```basic
10 NET GET "https://relay.example/room/42",#1200,2048
20 ' ...ゲームの処理...
30 IF NET()=-1 GOTO 20
40 IF NET()=-2 ?"ERR":END
50 ?STR$(#1200)
```

制限:

- https のみ。内部ネットワーク (localhost・プライベートアドレスなど) へはつながない
- レスポンスは 2KB まで、応答待ちは 10 秒まで。リダイレクトは追わず、Cookie は送らない

通信そのものはホストが `net::HttpClient` を実装して `Machine::set_http_client` で設定する。

### 拡張エラー

| 番号 | メッセージ    | 内容                 |
| ---- | ------------- | -------------------- |
| 13   | Network error | GET・POST の通信失敗 |

## テスト

```bash
cargo test -p ichigocrate-core
```

## アーキテクチャ要点

- 元 C コードのグローバル変数と RAM は `Machine` 構造体に集約している。
- ホストは `session::Session` を通して動かす。キー入力を `feed_key` / `on_enter` / `on_escape` などで渡し、毎フレーム `tick(now_ms)` を呼ぶ。`tick` は 1 フレームあたりの実行文数を制限し (無限ループでも UI を止めない)、WAIT と `TICK()` を実時間に合わせる。
- テストやヘッドレス用途では `exec_line` / `run_to_completion` で `Machine` を直接動かせる (WAIT は無視する)。
- ファイル保存先は `machine::Storage` トレイトで差し替える。描画は `render::render_mono` が 1bpp ビットマップまでを作り、色付けはホストが行う。
