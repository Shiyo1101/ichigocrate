//! ichigocrate 拡張 `GET` のテスト: URL の検証、応答待ち、応答の受け渡し
//! (即時実行はキー入力として流し込み、プログラム実行中は画面へ出力)、
//! 失敗・タイムアウト・ESC による中断。

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use common::screen_text;
use ichigocrate_core::net::{
    HttpClient, HttpCompletion, HttpError, HttpMethod, HttpRequest, HttpResponse,
    MAX_RESPONSE_BYTES, REQUEST_TIMEOUT_MS,
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

/// HTTP のステータスに関わらず本文はそのまま扱う。
#[test]
fn error_status_body_is_still_output() {
    let (mut s, http) = new_session();
    run_program(&mut s, &["10 GET \"https://example.com/\""]);
    http.complete(Ok(HttpResponse {
        status: 404,
        body: b"NOT FOUND".to_vec(),
    }));
    assert_eq!(s.tick(0.0), None);
    assert!(screen_text(&s.machine).contains("NOT FOUND"));
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
