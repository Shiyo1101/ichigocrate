//! ichigocrate 独自拡張: HTTPS 通信 (`GET` 文)。
//!
//! 通信そのものはホストが [`HttpClient`] として実装し ([`Machine::set_http_client`])、
//! core は URL の検証・リクエストの発行・応答待ちの管理までを担う。応答の扱い
//! (キー入力として流し込む / 画面へ出力する) は [`crate::session::Session`] が行う。
//!
//! 安全のため、接続先は https のみ、かつインターネット上のアドレスに限る。
//! ホスト名を名前解決した結果の検査はネイティブ側のクライアントの責務で、
//! ここではリテラルの IP アドレスや `localhost` を URL の段階で拒否する。
//!
//! [`Machine::set_http_client`]: crate::Machine::set_http_client

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::ram::N_LINEBUF;

/// レスポンス本文の上限 (バイト)。LIST 1 本分 (約 1KB) をテキスト化したものに
/// 余裕を持たせた大きさ。超えた応答は Network error として扱う。
pub const MAX_RESPONSE_BYTES: usize = 2048;

/// 応答待ちの上限 (ミリ秒)。超えたらリクエストを取り消して Network error にする。
pub const REQUEST_TIMEOUT_MS: f64 = 10_000.0;

const HTTPS_SCHEME: &str = "https://";

/// HTTP メソッド。現状は GET のみ (POST は将来の拡張)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
}

/// ホストへ渡すリクエスト。`id` は応答との対応付けに使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub id: u32,
    pub method: HttpMethod,
    /// 検証済みの URL (`https://` で始まる印字可能 ASCII)。
    pub url: String,
}

/// 成功した応答。ステータスコードに関わらず本文はそのまま扱う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// 通信の失敗理由。BASIC 側ではどれも Network error になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    /// 接続・TLS・名前解決・接続先の拒否・CORS など、応答が得られなかった。
    Failed,
    /// 本文が [`MAX_RESPONSE_BYTES`] を超えた。
    TooLarge,
}

/// リクエスト `id` の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCompletion {
    pub id: u32,
    pub result: Result<HttpResponse, HttpError>,
}

/// ホスト側の HTTPS クライアント。通信は非同期に行い、完了したものを
/// [`HttpClient::poll`] で返す。
pub trait HttpClient: std::fmt::Debug {
    /// リクエストを開始する。完了を待たずに戻ること。
    fn start(&mut self, request: HttpRequest);
    /// 完了したリクエストの結果を 1 件取り出す。無ければ `None`。
    fn poll(&mut self) -> Option<HttpCompletion>;
    /// タイムアウトや ESC による取り消し。以後この `id` の結果は捨てられる。
    fn cancel(&mut self, _id: u32) {}
}

/// 応答待ち中のリクエスト。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingHttp {
    pub(crate) id: u32,
    /// 即時実行 (REPL) で発行されたか。真なら応答をキー入力として流し込み、
    /// 偽 (プログラム実行中) なら画面へ出力する。
    pub(crate) feeds_input: bool,
}

/// RAM 上の文字列から URL を取り出して検証する。不正なら `None`。
///
/// `https://` (大文字小文字は問わない) で始まり、印字可能 ASCII だけからなり、
/// ホスト部が空でなく内部ネットワークを指さないこと。スキームは小文字へ揃える。
pub(crate) fn parse_https_url(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= N_LINEBUF || !bytes.iter().all(u8::is_ascii_graphic) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let (scheme, rest) = text.split_at_checked(HTTPS_SCHEME.len())?;
    if !scheme.eq_ignore_ascii_case(HTTPS_SCHEME) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // userinfo (`user@host`) は使わない。紛らわしい URL で接続先を偽装されないよう拒否する。
    if authority.contains('@') {
        return None;
    }
    let host = host_of(authority)?;
    if is_internal_host_literal(host) {
        return None;
    }
    Some(format!("{HTTPS_SCHEME}{rest}"))
}

/// `host[:port]` / `[v6addr][:port]` からホスト部を取り出す。空なら `None`。
fn host_of(authority: &str) -> Option<&str> {
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split_once(']')?.0
    } else {
        authority.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then_some(host)
}

/// URL に直接書かれたホストが内部ネットワークを指すか (`localhost` やプライベート IP)。
/// 名前解決が必要なホスト名はここでは判定できないので、ホスト側のクライアントが検査する。
fn is_internal_host_literal(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    let lower = lower.trim_end_matches('.');
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .is_ok_and(|ip| !is_internet_address(ip))
}

/// インターネット上の (= 内部ネットワークではない) アドレスか。
///
/// ループバック・プライベート・リンクローカル・CGNAT・マルチキャスト・予約済みなどを
/// 除外する。IPv4 を埋め込んだ IPv6 (`::ffff:a.b.c.d`) は中の IPv4 で判定する。
/// ネイティブ版のクライアントは名前解決の結果をこれで検査してから接続する。
pub fn is_internet_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internet_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_internet_v4(v4),
            None => is_internet_v6(v6),
        },
    }
}

fn is_internet_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0 // 0.0.0.0/8 (このネットワーク)
        || (a == 100 && (b & 0xc0) == 64) // 100.64.0.0/10 (CGNAT)
        || (a == 192 && b == 0 && ip.octets()[2] == 0) // 192.0.0.0/24 (IETF 予約)
        || (a == 198 && (b & 0xfe) == 18) // 198.18.0.0/15 (ベンチマーク用)
        || a >= 240) // 240.0.0.0/4 (予約)
}

fn is_internet_v6(ip: Ipv6Addr) -> bool {
    let seg0 = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (seg0 & 0xfe00) == 0xfc00 // fc00::/7 (ユニークローカル)
        || (seg0 & 0xffc0) == 0xfe80 // fe80::/10 (リンクローカル)
        || (seg0 & 0xffc0) == 0xfec0 // fec0::/10 (旧サイトローカル)
        || (seg0 == 0x2001 && ip.segments()[1] == 0x0db8)) // 2001:db8::/32 (文書用)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_urls() {
        assert_eq!(
            parse_https_url(b"https://kidspod.club/mj/2245").as_deref(),
            Some("https://kidspod.club/mj/2245")
        );
        // スキームは大文字でもよく、小文字に揃える (REPL は既定で大文字入力)
        assert_eq!(
            parse_https_url(b"HTTPS://KIDSPOD.CLUB/MJ/2245").as_deref(),
            Some("https://KIDSPOD.CLUB/MJ/2245")
        );
        assert!(parse_https_url(b"https://example.com:8443/a?b=1#c").is_some());
        assert!(parse_https_url(b"https://8.8.8.8/").is_some());
    }

    #[test]
    fn rejects_non_https_or_malformed_urls() {
        for url in [
            &b"http://example.com/"[..],
            b"ftp://example.com/",
            b"example.com/",
            b"https://",
            b"https:///path",
            b"https://exa mple.com/",
            b"https://user@example.com/",
            b"https://example.com/\xea",
        ] {
            assert_eq!(
                parse_https_url(url),
                None,
                "{:?}",
                String::from_utf8_lossy(url)
            );
        }
    }

    #[test]
    fn rejects_internal_host_literals() {
        for url in [
            &b"https://localhost/"[..],
            b"https://LOCALHOST./",
            b"https://app.localhost/",
            b"https://127.0.0.1/",
            b"https://192.168.1.1/",
            b"https://10.0.0.1:443/",
            b"https://169.254.169.254/",
            b"https://[::1]/",
            b"https://[fe80::1]/",
            b"https://[::ffff:192.168.0.1]/",
        ] {
            assert_eq!(
                parse_https_url(url),
                None,
                "{:?}",
                String::from_utf8_lossy(url)
            );
        }
    }

    #[test]
    fn classifies_internet_addresses() {
        let internal = [
            "0.0.0.0",
            "10.1.2.3",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.0.0.1",
            "192.168.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "ff02::1",
            "::ffff:10.0.0.1",
            "2001:db8::1",
        ];
        for ip in internal {
            assert!(!is_internet_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_internet_address(ip.parse().unwrap()), "{ip}");
        }
    }
}
