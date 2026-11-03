//! ichigocrate 拡張 `GET` / `POST` のテスト: URL の検証、応答待ち、応答の受け渡し
//! (即時実行はキー入力として流し込み、プログラム実行中は画面へ出力)、
//! 失敗・タイムアウト・ESC による中断、どんな応答でも異常終了しないこと。

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use common::{screen_text, MemStore};
use ichigocrate_core::net::{
    HttpClient, HttpCompletion, HttpError, HttpMethod, HttpRequest, HttpResponse,
    MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, REQUEST_TIMEOUT_MS,
};
use ichigocrate_core::session::{Session, FRAME_MS};
use ichigocrate_core::{exec_line, BasicError, Machine};

#[derive(Debug, Default)]
struct MockState {
    requests: Vec<HttpRequest>,
    completions: VecDeque<HttpCompletion>,
    cancelled: Vec<u32>,
}

/// 発行されたリクエストを記録し、テストが用意した結果を返すクライアント。
#[derive(Debug, Default, Clone)]
struct MockHttp(Rc<RefCell<MockState>>);

impl HttpClient for MockHttp {
    fn start(&mut self, request: HttpRequest) {
        self.0.borrow_mut().requests.push(request);
    }

    fn poll(&mut self) -> Option<HttpCompletion> {
        self.0.borrow_mut().completions.pop_front()
    }

    fn cancel(&mut self, id: u32) {
        self.0.borrow_mut().cancelled.push(id);
    }
}

impl MockHttp {
    fn requests(&self) -> Vec<HttpRequest> {
        self.0.borrow().requests.clone()
    }

    fn last_id(&self) -> u32 {
        self.0
            .borrow()
            .requests
            .last()
            .expect("リクエストが無い")
            .id
    }

    /// 直近のリクエストへ結果を返す。
    fn complete(&self, result: Result<HttpResponse, HttpError>) {
        let id = self.last_id();
        self.0
            .borrow_mut()
            .completions
            .push_back(HttpCompletion { id, result });
    }

    fn respond(&self, body: &[u8]) {
        self.complete(Ok(HttpResponse {
            status: 200,
            body: body.to_vec(),
        }));
    }
}

fn new_session() -> (Session, MockHttp) {
    let http = MockHttp::default();
    let mut m = Machine::new();
    m.set_http_client(Box::new(http.clone()));
    (Session::new(m), http)
}

/// 文字列を 1 文字ずつタイプして Enter を押す。
fn type_line(s: &mut Session, text: &str) -> Option<BasicError> {
    for b in text.bytes() {
        let _ = s.feed_char(b);
    }
    s.on_enter()
}

/// プログラムを登録して RUN する。
fn run_program(s: &mut Session, program: &[&str]) {
    for line in program {
        let _ = s.exec_line(line.as_bytes());
    }
    let _ = s.exec_line(b"RUN");
}

#[test]
fn rejects_non_https_url_without_sending() {
    let (mut s, http) = new_session();
    for url in [
        "http://example.com/",
        "example.com/",
        "https://192.168.1.1/",
    ] {
        let err = type_line(&mut s, &format!("GET \"{url}\""));
        assert_eq!(err, Some(BasicError::IllegalArgument), "{url}");
    }
    assert!(http.requests().is_empty());
}

#[test]
fn get_without_http_client_is_network_error() {
    let mut m = Machine::new();
    assert_eq!(
        exec_line(&mut m, "GET \"https://example.com/\""),
        Err(BasicError::NetworkError)
    );
}

/// 即時実行の GET は応答をキー入力として流し込む。行番号付きの行は LIST に登録され、
/// それ以外の行 (kidspod の先頭の `OK` など) はその場で実行される。
#[test]
fn immediate_get_feeds_response_as_key_input() {
    let (mut s, http) = new_session();
    assert_eq!(
        type_line(&mut s, "GET \"https://kidspod.club/mj/2245\""),
        None
    );
    assert_eq!(
        http.requests(),
        vec![HttpRequest {
            id: 0,
            method: HttpMethod::Get,
            url: "https://kidspod.club/mj/2245".into(),
        }]
    );
    assert!(s.is_running(), "応答が届くまで待つ");
    assert_eq!(s.machine.list_size, 0);

    http.respond(b"OK\r\n10 ?\"HI\"\r\n20 END\r\n");
    assert_eq!(s.tick(0.0), None);
    assert!(!s.is_running());
    assert!(s.machine.list_size > 0, "行番号付きの行が登録される");

    let _ = type_line(&mut s, "CLS");
    let _ = type_line(&mut s, "RUN");
    let _ = s.tick(FRAME_MS);
    assert!(screen_text(&s.machine).contains("HI"));
}

/// 流し込んだ行がプログラムを走らせたら、終わって REPL へ戻ってから続きを流す。
#[test]
fn feed_resumes_after_fed_line_finishes_running() {
    let (mut s, http) = new_session();
    let _ = type_line(&mut s, "GET \"https://example.com/\"");
    http.respond(b"10 WAIT 60\nRUN\n20 ?\"LATE\"\n");
    let _ = s.tick(0.0);
    assert!(s.is_running(), "RUN で WAIT 中");
    let mut t = 0.0;
    while s.is_running() {
        t += FRAME_MS;
        let _ = s.tick(t);
        assert!(t < 5000.0, "WAIT が終わらない");
    }
    // RUN 終了後に 20 行目が登録される
    let _ = type_line(&mut s, "LIST");
    assert!(screen_text(&s.machine).contains("20 ?\"LATE\""));
}

/// プログラム実行中の GET は応答を画面へ出力し、応答が届くまで次の文へ進まない。
#[test]
fn program_get_prints_response_and_waits() {
    let (mut s, http) = new_session();
    run_program(
        &mut s,
        &["10 GET \"https://example.com/\"", "20 ?\"AFTER\""],
    );
    for i in 0..3 {
        assert_eq!(s.tick(f64::from(i) * FRAME_MS), None);
        assert!(s.is_running());
    }
    assert!(!screen_text(&s.machine).contains("AFTER"));

    http.respond(b"ABC\r\n10 ?1\r\n");
    let _ = s.tick(4.0 * FRAME_MS);
    let t = screen_text(&s.machine);
    assert!(t.contains("ABC") && t.contains("AFTER"), "{t}");
    assert!(!s.is_running());
    // 画面へ出すだけで、応答中の行番号付きの行は登録しない
    let _ = type_line(&mut s, "CLS");
    let _ = type_line(&mut s, "LIST");
    assert!(!screen_text(&s.machine).contains("10 ?1"));
}

/// 画面に `first` の行があり、その次の行が `second` で始まるか。
fn has_lines(screen: &str, first: &str, second: &str) -> bool {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    lines
        .windows(2)
        .any(|w| w[0] == first && w[1].starts_with(second))
}

/// ステータスが 2xx 以外なら本文は使わず、ステータスを表示して Network error で止める。
#[test]
fn error_status_shows_status_and_stops() {
    // プログラム実行中: 本文は画面に出さず、次の文へ進まない
    let (mut s, http) = new_session();
    run_program(
        &mut s,
        &["10 GET \"https://example.com/\"", "20 ?\"AFTER\""],
    );
    http.complete(Ok(HttpResponse {
        status: 404,
        body: b"NOT FOUND".to_vec(),
    }));
    assert_eq!(s.tick(0.0), Some(BasicError::NetworkError));
    assert!(!s.is_running());
    let t = screen_text(&s.machine);
    assert!(has_lines(&t, "HTTP 404", "Network error in 10"), "{t}");
    assert!(!t.contains("NOT FOUND") && !t.contains("AFTER"), "{t}");

    // 即時実行: エラーページの本文をキー入力として流し込まない
    let (mut s, http) = new_session();
    let _ = type_line(&mut s, "GET \"https://kidspod.club/mj/0\"");
    http.complete(Ok(HttpResponse {
        status: 500,
        body: b"10 ?\"X\"\nerror!!".to_vec(),
    }));
    assert_eq!(s.tick(0.0), Some(BasicError::NetworkError));
    assert!(!s.is_running());
    assert_eq!(s.machine.list_size, 0);
    let t = screen_text(&s.machine);
    assert!(has_lines(&t, "HTTP 500", "Network error"), "{t}");
    assert!(!t.contains("Syntax error"), "{t}");
}

/// 2xx ならどれでも成功として本文を扱う。
#[test]
fn any_2xx_status_is_success() {
    let (mut s, http) = new_session();
    run_program(&mut s, &["10 GET \"https://example.com/\""]);
    http.complete(Ok(HttpResponse {
        status: 204,
        body: b"DONE".to_vec(),
    }));
    assert_eq!(s.tick(0.0), None);
    assert!(screen_text(&s.machine).contains("DONE"));
}

#[test]
fn failure_and_oversized_response_are_network_errors() {
    for result in [
        Err(HttpError::Failed),
        Err(HttpError::TooLarge),
        Ok(HttpResponse {
            status: 200,
            body: vec![b'A'; MAX_RESPONSE_BYTES + 1],
        }),
    ] {
        let (mut s, http) = new_session();
        run_program(
            &mut s,
            &["10 GET \"https://example.com/\"", "20 ?\"AFTER\""],
        );
        http.complete(result);
        assert_eq!(s.tick(0.0), Some(BasicError::NetworkError));
        assert!(!s.is_running());
        let t = screen_text(&s.machine);
        assert!(t.contains("Network error in 10"), "{t}");
        assert!(!t.contains("AFTER"));
    }
}

#[test]
fn timeout_cancels_request() {
    let (mut s, http) = new_session();
    run_program(&mut s, &["10 GET \"https://example.com/\""]);
    assert_eq!(s.tick(0.0), None);
    assert_eq!(s.tick(REQUEST_TIMEOUT_MS - 1.0), None);
    assert_eq!(s.tick(REQUEST_TIMEOUT_MS), Some(BasicError::NetworkError));
    assert_eq!(http.0.borrow().cancelled, vec![http.last_id()]);
}

/// ESC で応答待ちを中断できる。後から届いた古い応答は捨てられる。
#[test]
fn escape_cancels_and_ignores_late_response() {
    let (mut s, http) = new_session();
    let _ = type_line(&mut s, "GET \"https://example.com/\"");
    s.on_escape();
    assert_eq!(s.tick(0.0), Some(BasicError::Break));
    assert!(!s.is_running());
    assert_eq!(http.0.borrow().cancelled, vec![0]);

    http.respond(b"10 ?\"LATE\"\n");
    let _ = type_line(&mut s, "GET \"https://example.com/2\"");
    assert_eq!(s.tick(FRAME_MS), None);
    assert!(
        s.is_running(),
        "古い応答を新しいリクエストの応答と取り違えない"
    );
    assert_eq!(s.machine.list_size, 0);
}

/// URL は文字列リテラルだけでなく、そのアドレスでも渡せる。
#[test]
fn url_can_be_given_by_address() {
    let (mut s, http) = new_session();
    // 配列領域に URL を置き、その仮想アドレスを渡す
    let addr = ichigocrate_core::OFFSET_RAMROM + ichigocrate_core::ram::OFFSET_RAM_VAR;
    let url = b"https://example.com/x\0";
    s.machine.ram[ichigocrate_core::ram::OFFSET_RAM_VAR..][..url.len()].copy_from_slice(url);
    let _ = type_line(&mut s, &format!("GET {addr}"));
    assert_eq!(http.requests()[0].url, "https://example.com/x");
}

/// POST url,"文字列" は文字列を本文として送る。
#[test]
fn post_sends_string_body() {
    let (mut s, http) = new_session();
    assert_eq!(
        type_line(&mut s, "POST \"https://example.com/room/1\",\"HELLO\""),
        None
    );
    assert_eq!(
        http.requests(),
        vec![HttpRequest {
            id: 0,
            method: HttpMethod::Post {
                body: b"HELLO".to_vec()
            },
            url: "https://example.com/room/1".into(),
        }]
    );
    assert!(s.is_running(), "応答が届くまで待つ");
}

/// POST url,アドレス,長さ は RAM 上の生のバイト列を送る (NUL や `"` も含められる)。
#[test]
fn post_sends_raw_bytes_from_address() {
    let (mut s, http) = new_session();
    let addr = ichigocrate_core::OFFSET_RAMROM + ichigocrate_core::ram::OFFSET_RAM_VAR;
    let _ = type_line(
        &mut s,
        &format!("POKE {addr},1,0,34,255:POST \"https://example.com/\",{addr},4"),
    );
    assert_eq!(
        http.requests()[0].method,
        HttpMethod::Post {
            body: vec![1, 0, 34, 255]
        }
    );

    // 長さ 0 なら本文は空
    let (mut s, http) = new_session();
    let _ = type_line(&mut s, &format!("POST \"https://example.com/\",{addr},0"));
    assert_eq!(http.requests()[0].method, HttpMethod::Post { body: vec![] });
}

/// 本文が大きすぎる・長さが負・RAM の外にかかる・URL が https でないものは送らない。
#[test]
fn post_rejects_invalid_arguments_without_sending() {
    let (mut s, http) = new_session();
    let addr = ichigocrate_core::OFFSET_RAMROM;
    let too_long = MAX_REQUEST_BYTES + 1;
    for line in [
        format!("POST \"https://example.com/\",{addr},{too_long}"),
        format!("POST \"https://example.com/\",{addr},-1"),
        "POST \"https://example.com/\",0,1".to_string(),
        "POST \"https://example.com/\",#7FFF,2".to_string(),
        "POST \"http://example.com/\",\"X\"".to_string(),
    ] {
        assert_eq!(
            type_line(&mut s, &line),
            Some(BasicError::IllegalArgument),
            "{line}"
        );
    }
    assert_eq!(
        type_line(&mut s, "POST \"https://example.com/\""),
        Some(BasicError::SyntaxError)
    );
    assert!(http.requests().is_empty());
}

/// POST の応答の扱いは GET と同じ (プログラム実行中は画面へ出力して次の文へ進む)。
#[test]
fn program_post_prints_response() {
    let (mut s, http) = new_session();
    run_program(
        &mut s,
        &["10 POST \"https://a.jp/\",\"M3\"", "20 ?\"AFTER\""],
    );
    assert_eq!(s.tick(0.0), None);
    assert!(s.is_running());
    http.respond(b"YOUR TURN\n");
    let _ = s.tick(FRAME_MS);
    let t = screen_text(&s.machine);
    assert!(t.contains("YOUR TURN") && t.contains("AFTER"), "{t}");

    // LIST では POST のまま表示される
    let _ = type_line(&mut s, "CLS");
    let _ = type_line(&mut s, "LIST");
    assert!(screen_text(&s.machine).contains("10 POST \"https://a.jp/\",\"M3\""));
}

/// 決まった種から擬似乱数列を作る (xorshift64)。テストを再現可能にするため外部クレートは使わない。
fn xorshift(mut seed: u64) -> impl FnMut() -> u64 {
    move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }
}

/// どんな応答 (最大 2KB の任意のバイト列) を受け取っても、即時実行での流し込み・
/// 実行中の画面出力のどちらでも異常終了 (パニック) しない。応答はサーバー側が
/// 自由に決められるので、ホスト側の安全はこれに依存する。
#[test]
fn arbitrary_responses_never_panic() {
    // 半分は完全にランダムなバイト列、半分は BASIC として解釈されやすい文字の並び
    const BASIC_LIKE: &[u8] = b"0123456789 ?\"\n:,=+-*/()#ABCPOKERUNGOTOLISTSAVEGET";
    let mut next = xorshift(0x1234_5678_9abc_def0);
    let mut fed_programs = 0;
    for round in 0..100 {
        let len = (next() % (MAX_RESPONSE_BYTES as u64 + 1)) as usize;
        let body: Vec<u8> = (0..len)
            .map(|_| {
                let r = next();
                if round % 2 == 0 {
                    r as u8
                } else {
                    BASIC_LIKE[(r % BASIC_LIKE.len() as u64) as usize]
                }
            })
            .collect();
        for in_program in [false, true] {
            let http = MockHttp::default();
            let mut m = Machine::new();
            m.set_http_client(Box::new(http.clone()));
            m.set_storage(Box::<MemStore>::default());
            let mut s = Session::new(m);
            if in_program {
                run_program(&mut s, &["10 GET \"https://example.com/\""]);
            } else {
                let _ = type_line(&mut s, "GET \"https://example.com/\"");
            }
            http.respond(&body);
            let mut t = 0.0;
            for frame in 0..300 {
                t += FRAME_MS;
                let _ = s.tick(t);
                // 流し込んだ行が無限ループや長い WAIT を走らせることがあるので途中で止める
                if frame >= 200 {
                    s.on_escape();
                }
            }
            fed_programs += usize::from(!in_program && s.machine.list_size > 0);
        }
    }
    // 流し込みが実際に行われている (応答が素通りされていない) ことの確認
    assert!(fed_programs > 0);
}
