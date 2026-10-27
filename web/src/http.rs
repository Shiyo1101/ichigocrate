//! Web 版の HTTPS クライアント (ichigocrate 拡張 `GET` 用の [`HttpClient`] 実装)。
//!
//! ブラウザの `fetch` で通信し、完了した結果をキューに積んで [`HttpClient::poll`] で返す。
//! ブラウザからは接続先の IP を調べられないため、内部ネットワークへの到達は次で防ぐ。
//!
//! - 独自ヘッダ [`PREFLIGHT_HEADER`] を付け、必ず CORS の事前確認 (preflight) を経由させる。
//!   CORS を明示的に許可したサーバーにしか本番のリクエストが届かないので、ルーターなど
//!   CORS を許可していない機器にはリクエストの中身が届かず、副作用も起きない
//! - Cookie・認証情報は送らない (`credentials: "omit"`)。リダイレクトは失敗として扱う
//!
//! 接続先が CORS を許可していなければ応答は読めない (Network error)。CORS の設定は
//! 接続先サーバー側 (利用者) の責務。

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ichigocrate_core::net::{
    HttpClient, HttpCompletion, HttpError, HttpMethod, HttpRequest, HttpResponse,
    MAX_RESPONSE_BYTES,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Request, RequestCache, RequestCredentials, RequestInit, RequestMode, RequestRedirect, Response,
};

/// CORS の事前確認を必ず発生させるための独自ヘッダ (CORS-safelisted でない名前)。
/// 接続先サーバーは `Access-Control-Allow-Headers` でこのヘッダを許可する必要がある。
const PREFLIGHT_HEADER: &str = "X-Ichigocrate";

#[derive(Debug, Default)]
pub struct FetchHttpClient {
    completed: Rc<RefCell<VecDeque<HttpCompletion>>>,
}

impl HttpClient for FetchHttpClient {
    fn start(&mut self, request: HttpRequest) {
        let completed = Rc::clone(&self.completed);
        spawn_local(async move {
            let result = match request.method {
                HttpMethod::Get => fetch(&request.url, "GET").await,
            };
            completed.borrow_mut().push_back(HttpCompletion {
                id: request.id,
                result,
            });
        });
    }

    fn poll(&mut self) -> Option<HttpCompletion> {
        self.completed.borrow_mut().pop_front()
    }
}

async fn fetch(url: &str, method: &str) -> Result<HttpResponse, HttpError> {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_mode(RequestMode::Cors);
    init.set_credentials(RequestCredentials::Omit);
    init.set_redirect(RequestRedirect::Error);
    init.set_cache(RequestCache::NoStore);
    let request = Request::new_with_str_and_init(url, &init).map_err(|_| HttpError::Failed)?;
    request
        .headers()
        .set(PREFLIGHT_HEADER, "1")
        .map_err(|_| HttpError::Failed)?;

    let window = web_sys::window().ok_or(HttpError::Failed)?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| HttpError::Failed)?
        .dyn_into()
        .map_err(|_| HttpError::Failed)?;
    let status = response.status();
    let buffer = JsFuture::from(response.array_buffer().map_err(|_| HttpError::Failed)?)
        .await
        .map_err(|_| HttpError::Failed)?;
    let body = js_sys::Uint8Array::new(&buffer).to_vec();
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(HttpError::TooLarge);
    }
    Ok(HttpResponse { status, body })
}
