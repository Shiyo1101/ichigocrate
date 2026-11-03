//! ネイティブ版の HTTPS クライアント (ichigocrate 拡張 `GET` / `POST` 用の [`HttpClient`] 実装)。
//!
//! リクエストごとにスレッドを起こして ureq で通信し、結果をチャネルで返すので
//! UI は固まらない。内部ネットワークへ到達させないため、次のように接続する。
//!
//! - https のみ。証明書は ureq 同梱の公開ルート証明書 (webpki-roots) だけで検証し、
//!   OS の証明書ストア (社内 CA が入っていることがある) は使わない
//! - 名前解決の結果が 1 つでも内部ネットワークのアドレスなら接続しない。検査した
//!   アドレスへそのまま接続するので、検査後の引き直し (DNS リバインディング) も効かない
//! - リダイレクトは追わない。OS・環境変数のプロキシ設定も使わない
//! - Cookie・認証情報は送らない

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use ichigocrate_core::net::{
    is_internet_address, HttpClient, HttpCompletion, HttpError, HttpMethod, HttpRequest,
    HttpResponse, MAX_RESPONSE_BYTES, POST_CONTENT_TYPE, REQUEST_TIMEOUT_MS,
};
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use ureq::Agent;

/// 送信する User-Agent。
const USER_AGENT: &str = concat!("ichigocrate/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
pub struct NativeHttpClient {
    agent: Agent,
    tx: Sender<HttpCompletion>,
    rx: Receiver<HttpCompletion>,
}

impl NativeHttpClient {
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .proxy(None)
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_millis(REQUEST_TIMEOUT_MS as u64)))
            .build();
        let agent = Agent::with_parts(
            config,
            DefaultConnector::default(),
            InternetOnlyResolver::default(),
        );
        let (tx, rx) = mpsc::channel();
        Self { agent, tx, rx }
    }
}

impl HttpClient for NativeHttpClient {
    fn start(&mut self, request: HttpRequest) {
        let agent = self.agent.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = send(&agent, &request);
            // 受け手 (Session) が先に破棄されていれば結果は不要なので無視する。
            let _ = tx.send(HttpCompletion {
                id: request.id,
                result,
            });
        });
    }

    fn poll(&mut self) -> Option<HttpCompletion> {
        self.rx.try_recv().ok()
    }
}

fn send(agent: &Agent, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
    let response = match &request.method {
        HttpMethod::Get => agent.get(&request.url).call(),
        HttpMethod::Post { body } => agent
            .post(&request.url)
            .header("Content-Type", POST_CONTENT_TYPE)
            .send(body.as_slice()),
    };
    let mut response = response.map_err(|_| HttpError::Failed)?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES as u64)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => HttpError::TooLarge,
            _ => HttpError::Failed,
        })?;
    Ok(HttpResponse { status, body })
}

/// 名前解決の結果がすべてインターネット上のアドレスのときだけ通す resolver。
#[derive(Debug, Default)]
struct InternetOnlyResolver(DefaultResolver);

impl Resolver for InternetOnlyResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addrs = self.0.resolve(uri, config, timeout)?;
        if addrs.iter().all(|addr| is_internet_address(addr.ip())) {
            Ok(addrs)
        } else {
            Err(ureq::Error::HostNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実際にネットワークへつなぐため既定では実行しない (`cargo test -- --ignored`)。
    #[test]
    #[ignore = "ネットワーク接続が必要"]
    fn fetches_kidspod_program() {
        let client = NativeHttpClient::new();
        let request = HttpRequest {
            id: 0,
            method: HttpMethod::Get,
            url: "https://kidspod.club/mj/2245".into(),
        };
        let response = send(&client.agent, &request).unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.starts_with(b"OK\n10 "));
    }

    /// 本文と Content-Type がそのまま届く (受け取った内容を返すエコーサービスで確認)。
    #[test]
    #[ignore = "ネットワーク接続が必要"]
    fn posts_raw_body() {
        let client = NativeHttpClient::new();
        let request = HttpRequest {
            id: 0,
            method: HttpMethod::Post {
                body: b"HELLO ICHIGO".to_vec(),
            },
            url: "https://postman-echo.com/post".into(),
        };
        let response = send(&client.agent, &request).unwrap();
        assert_eq!(response.status, 200);
        let text = String::from_utf8_lossy(&response.body);
        assert!(text.contains("application/octet-stream"), "{text}");
        // 本文は "HELLO ICHIGO" のバイト値の配列として返ってくる
        assert!(
            text.contains("[72,69,76,76,79,32,73,67,72,73,71,79]"),
            "{text}"
        );
    }

    /// 公開 DNS で 127.0.0.1 を返すホスト名でも、名前解決後の検査で接続しない。
    #[test]
    #[ignore = "ネットワーク接続が必要"]
    fn refuses_hostname_resolving_to_loopback() {
        let client = NativeHttpClient::new();
        // 接続を試みた結果の失敗 (ポートが閉じている等) ではなく、resolver が拒否したこと
        let err = client
            .agent
            .get("https://localtest.me/")
            .call()
            .unwrap_err();
        assert!(matches!(err, ureq::Error::HostNotFound), "{err:?}");
    }

    /// http は ureq 側でも拒否される (core の URL 検証と二重)。
    #[test]
    fn refuses_plain_http() {
        let client = NativeHttpClient::new();
        let err = client.agent.get("http://example.com/").call().unwrap_err();
        assert!(matches!(err, ureq::Error::RequireHttpsOnly(_)), "{err:?}");
    }
}
