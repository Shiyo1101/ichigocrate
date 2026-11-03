//! `samples/` のプログラムが読み込めて、意図どおりに動くことのテスト。
//! ネット対戦のサンプルは、受信箱方式の中継サーバー (ichigocrate-relay) を
//! まねたクライアントで 2 台を対戦させる。

mod common;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use common::screen_text;
use ichigocrate_core::net::{HttpClient, HttpCompletion, HttpMethod, HttpRequest, HttpResponse};
use ichigocrate_core::session::{Session, FRAME_MS};
use ichigocrate_core::Machine;

/// 受信箱 (URL ごとのメッセージの列)。2 台で共有する。
type Inboxes = Rc<RefCell<HashMap<String, VecDeque<Vec<u8>>>>>;

/// 中継サーバーの代わり。POST で URL の受信箱へ積み、GET で先頭を取り出す (空なら 0 バイト)。
#[derive(Debug)]
struct FakeRelay {
    inboxes: Inboxes,
    completed: VecDeque<HttpCompletion>,
}

impl HttpClient for FakeRelay {
    fn start(&mut self, request: HttpRequest) {
        let mut inboxes = self.inboxes.borrow_mut();
        let inbox = inboxes.entry(request.url).or_default();
        let (status, body) = match request.method {
            HttpMethod::Get => (200, inbox.pop_front().unwrap_or_default()),
            HttpMethod::Post { body } => {
                inbox.push_back(body);
                (204, Vec::new())
            }
        };
        self.completed.push_back(HttpCompletion {
            id: request.id,
            result: Ok(HttpResponse { status, body }),
        });
    }

    fn poll(&mut self) -> Option<HttpCompletion> {
        self.completed.pop_front()
    }
}

/// サンプルを読み込んだ ichigocrate を 1 台用意する。
fn load_sample(source: &str, inboxes: &Inboxes) -> Session {
    let mut m = Machine::new();
    m.set_http_client(Box::new(FakeRelay {
        inboxes: Rc::clone(inboxes),
        completed: VecDeque::new(),
    }));
    let mut s = Session::new(m);
    for line in source.lines() {
        assert_eq!(s.exec_line(line.as_bytes()), None, "読み込めない: {line}");
    }
    s
}

fn type_line(s: &mut Session, text: &str) {
    for b in text.bytes() {
        let _ = s.feed_char(b);
    }
    let _ = s.on_enter();
}

/// 2 台を交互に進める。`until` が真になるか、上限に達したら戻る。
fn run_both(
    a: &mut Session,
    b: &mut Session,
    t: &mut f64,
    until: impl Fn(&Session, &Session) -> bool,
) {
    for _ in 0..10_000 {
        if until(a, b) {
            return;
        }
        *t += FRAME_MS;
        assert_eq!(a.tick(*t), None, "{}", screen_text(&a.machine));
        assert_eq!(b.tick(*t), None, "{}", screen_text(&b.machine));
    }
    panic!(
        "終わらない\nA:\n{}\nB:\n{}",
        screen_text(&a.machine),
        screen_text(&b.machine)
    );
}

#[test]
fn net_oxo_two_players_play_to_the_end() {
    let source = include_str!("../../samples/net-oxo.bas");
    let inboxes = Inboxes::default();
    let mut a = load_sample(source, &inboxes);
    let mut b = load_sample(source, &inboxes);
    let _ = a.exec_line(b"RUN");
    let _ = b.exec_line(b"RUN");

    // 部屋番号とプレイヤー番号を答える
    let mut t = 0.0;
    for (s, player) in [(&mut a, "1"), (&mut b, "2")] {
        for answer in ["42", player] {
            for _ in 0..10 {
                if s.is_awaiting_input() {
                    break;
                }
                t += FRAME_MS;
                let _ = s.tick(t);
            }
            assert!(s.is_awaiting_input(), "{}", screen_text(&s.machine));
            type_line(s, answer);
        }
    }

    // A (O) は 1・2・3、B (X) は 4・5 を選ぶ。手番でないときは押しても読まれない
    // (相手の手を待っている間はキーを読まない) ので、手番が来るたびに押す。
    let moves = [
        (true, b'1'),
        (false, b'4'),
        (true, b'2'),
        (false, b'5'),
        (true, b'3'),
    ];
    for (by_a, key) in moves {
        run_both(&mut a, &mut b, &mut t, |a, b| {
            let s = if by_a { a } else { b };
            screen_text(&s.machine).contains("YOUR TURN")
        });
        let s = if by_a { &mut a } else { &mut b };
        let _ = s.feed_char(key);
        // 手を打って相手の番の表示 (WAIT...) に変わるまで進める
        run_both(&mut a, &mut b, &mut t, |a, b| {
            let s = if by_a { a } else { b };
            !screen_text(&s.machine).contains("YOUR TURN")
        });
    }
    run_both(&mut a, &mut b, &mut t, |a, b| {
        !a.is_running() && !b.is_running()
    });

    for s in [&a, &b] {
        let screen = screen_text(&s.machine);
        assert!(screen.contains("O WIN!"), "{screen}");
    }
    // 送った手はすべて相手に読まれ、受信箱には何も残らない
    assert!(inboxes.borrow().values().all(VecDeque::is_empty));
    // 部屋 42 のプレイヤーごとの受信箱を使う
    let urls: Vec<String> = inboxes.borrow().keys().cloned().collect();
    for box_name in ["A", "B"] {
        let url =
            format!("https://ichigocrate-relay.ichigocrate-relay.workers.dev/r/0042/{box_name}");
        assert!(urls.contains(&url), "{urls:?}");
    }
}
