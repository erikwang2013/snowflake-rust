// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! poem 3 集成。
//!
//! 用 `.data(guard)` 注册，处理器把守卫写在参数里：
//!
//! ```no_run
//! # mod demo {
//! // `.data(...)` 来自 EndpointExt
//! use poem::{EndpointExt, Route, get, handler};
//! use snowflake::guard::Guard;
//!
//! #[handler]
//! async fn create_order(guard: Guard) -> String {
//!     guard.id().map(|id| id.to_string()).unwrap_or_default()
//! }
//!
//! # fn main() {
//! let guard = Guard::new(snowflake::Snowflake::default());
//! let app = Route::new().at("/", get(create_order)).data(guard);
//! # let _ = app;
//! # }
//! # }
//! ```
//!
//! poem 的 `.data(v)` 要求 `T: Clone + Send + Sync + 'static`；[`Guard`] 满足，
//! 且克隆的只是一次 `Arc` 引用计数递增。

use poem::http::StatusCode;
use poem::web::{FromRequest, RequestBody};
use poem::{Error, Request, Result};

use super::GuardNotConfigured;
use crate::guard::Guard;

impl<'a> FromRequest<'a> for Guard {
    async fn from_request(req: &'a Request, _body: &mut RequestBody) -> Result<Self> {
        req.extensions().get::<Guard>().cloned().ok_or_else(|| {
            Error::from_string(
                GuardNotConfigured.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snowflake;
    use poem::Request;

    fn guard() -> Guard {
        Guard::new(Snowflake::default())
    }

    async fn extract(req: &Request) -> Result<Guard> {
        let mut body = RequestBody::default();
        Guard::from_request(req, &mut body).await
    }

    #[tokio::test]
    async fn extracts_from_request_extensions() {
        let mut req = Request::builder().finish();
        req.extensions_mut().insert(guard());

        let extracted = extract(&req).await.unwrap();
        assert!(extracted.id().unwrap() > 0);
    }

    /// 忘了 `.data(...)` 时是 500，不是 400 —— 这是服务端装配问题。
    #[tokio::test]
    async fn missing_data_is_a_500() {
        let req = Request::builder().finish();
        // 不用 unwrap_err()：它要求 Ok 侧（Guard）实现 Debug，而 Guard 故意没有
        match extract(&req).await {
            Err(err) => assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR),
            Ok(_) => panic!("没注册 data 时应当报错"),
        }
    }

    /// 处理器拿到的与注册进去的是同一个发号器：同一实例严格单调。
    #[tokio::test]
    async fn handler_and_state_share_one_generator() {
        let state = guard();
        let previous = state.id().unwrap();

        let mut req = Request::builder().finish();
        req.extensions_mut().insert(state);

        let extracted = extract(&req).await.unwrap();
        assert!(extracted.id().unwrap() > previous);
    }

    /// 端到端：`.data(guard)` 注册 + 处理器参数提取，走一遍真实请求。
    #[tokio::test]
    async fn data_registration_is_end_to_end() {
        use poem::{EndpointExt, Route, get, handler};

        #[handler]
        async fn create_order(guard: Guard) -> String {
            guard.id().unwrap().to_string()
        }

        let app = Route::new().at("/next", get(create_order)).data(guard());
        let client = poem::test::TestClient::new(app);

        let response = client.get("/next").send().await;
        response.assert_status_is_ok();

        // TestResponse(pub Response)：从内层 Response 取 body
        let text = response.0.into_body().into_string().await.unwrap();
        assert!(text.parse::<i64>().unwrap() > 0);
    }
}
