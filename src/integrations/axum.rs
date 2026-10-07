// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! axum 0.8 集成。
//!
//! 实现 [`FromRequestParts`]，于是处理器可以直接把 [`Guard`] 写在参数里：
//!
//! ```no_run
//! use axum::{routing::get, Router};
//! use snowflake::guard::Guard;
//! use snowflake::integrations::Guarded;
//!
//! #[derive(Clone)]
//! struct AppState { snowflake: Guard }
//! impl Guarded for AppState {
//!     fn guard(&self) -> &Guard { &self.snowflake }
//! }
//!
//! async fn create_order(guard: Guard) -> String {
//!     guard.id().map(|id| id.to_string()).unwrap_or_default()
//! }
//!
//! let state = AppState { snowflake: /* 你的守卫 */ todo!() };
//! let app: Router = Router::new().route("/", get(create_order)).with_state(state);
//! # let _ = app;
//! ```
//!
//! # 提取是不会失败的
//!
//! `Guarded::guard()` 返回引用，取不到守卫只可能是状态装配错了 —— 那是编译期
//! 就该发现的事，不是运行期。所以 `Rejection` 是 [`Infallible`]：这个提取器
//! 永远不会把请求拒掉。
//!
//! 发号本身的失败（时钟回拨超容忍、epoch 耗尽、序列耗尽）发生在处理器体里，
//! 由调用方决定怎么回应 —— 本库不替应用决定「时钟回拨该返回 500 还是 503」。

use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use super::Guarded;
use crate::guard::Guard;

/// 提取器实现的宿主类型。写出来只是为了让「守卫是怎么进来的」这件事
/// 在文档与再导出里有个名字。
pub type GuardExtractor = Guard;

impl<S> FromRequestParts<S> for Guard
where
    S: Guarded + Send + Sync,
{
    type Rejection = Infallible;

    /// 从应用状态取守卫，克隆一份交给处理器。
    ///
    /// 克隆的是 `Arc`，不是生成器状态 —— 每请求的成本就是一次引用计数递增。
    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(state.guard().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snowflake;
    use axum::Router;
    use axum::body::Body;
    use axum::extract::Extension;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt as _;

    fn guard() -> Guard {
        Guard::new(Snowflake::default())
    }

    /// 状态就是一把守卫时，零样板可用。
    #[derive(Clone)]
    struct BareState {
        snowflake: Guard,
    }
    impl Guarded for BareState {
        fn guard(&self) -> &Guard {
            &self.snowflake
        }
    }

    /// 真实形状：守卫只是状态里的一个字段，旁边还有别的东西。
    #[derive(Clone)]
    struct AppState {
        #[allow(dead_code)]
        pool: std::sync::Arc<String>,
        snowflake: Guard,
    }
    impl Guarded for AppState {
        fn guard(&self) -> &Guard {
            &self.snowflake
        }
    }

    /// 直接调 trait 方法，不起服务器 —— 确认签名接得上。
    #[tokio::test]
    async fn extracts_directly_from_parts() {
        let state = AppState {
            pool: std::sync::Arc::new("pool".into()),
            snowflake: guard(),
        };
        let (mut parts, _) = Request::new(()).into_parts();

        let extracted = Guard::from_request_parts(&mut parts, &state).await.unwrap();
        assert!(extracted.id().unwrap() > 0);
    }

    /// 端到端：处理器参数里直接写 `Guard`。
    #[tokio::test]
    async fn handler_can_take_a_guard_argument() {
        async fn handler(guard: Guard) -> String {
            guard.id().unwrap().to_string()
        }

        let state = AppState {
            pool: std::sync::Arc::new("pool".into()),
            snowflake: guard(),
        };
        let app = Router::new().route("/", get(handler)).with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let id: i64 = std::str::from_utf8(&body).unwrap().parse().unwrap();
        assert!(id > 0);
    }

    /// 处理器拿到的与状态里的是同一个发号器。
    ///
    /// 状态先用**它自己**的守卫发一个 ID 放进扩展里；同一个实例是严格单调的，
    /// 所以处理器发出来的 ID 必须大于它 —— 等于或小于都说明拿到了另一份状态
    /// （那就是同毫秒重复发号的经典事故，正是 `Shared` 要防的那件事）。
    #[tokio::test]
    async fn handler_and_state_share_one_generator() {
        async fn handler(guard: Guard, Extension(previous): Extension<i64>) -> String {
            let id = guard.id().unwrap();
            assert!(
                id > previous,
                "处理器与状态不是同一个实例: {id} <= {previous}"
            );
            id.to_string()
        }

        let state = BareState { snowflake: guard() };
        let previous = state.snowflake.id().unwrap();

        let app = Router::new()
            .route("/", get(handler))
            .layer(Extension(previous))
            .with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
