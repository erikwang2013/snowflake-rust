// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! actix-web 4 集成。
//!
//! 守卫注册进 `app_data`，处理器把它直接写在参数里：
//!
//! ```no_run
//! # mod demo {
//! use actix_web::{web, App};
//! use snowflake::guard::Guard;
//!
//! async fn create_order(guard: Guard) -> String {
//!     guard.id().map(|id| id.to_string()).unwrap_or_default()
//! }
//!
//! # fn main() {
//! let guard = Guard::new(snowflake::Snowflake::default());
//! let app = move || {
//!     App::new()
//!         .app_data(web::Data::new(guard.clone()))
//!         .route("/", web::get().to(create_order))
//! };
//! # let _ = app;
//! # }
//! # }
//! ```
//!
//! # 与 [`Guarded`](super::Guarded) 的关系
//!
//! actix 没有「单一应用状态」这个概念，它的 `app_data` 是按**类型**取值的，
//! 所以这里走的是 `Data<Guard>` 这条路，而不是 [`Guarded`](super::Guarded)。
//!
//! 状态结构体比较大时，注册 `Data<AppState>` 并让 `AppState` 实现
//! [`Guarded`](super::Guarded)，然后在处理器里写 `state.guard()` —— 那条路
//! 一样通，只是取用方式不同。

use std::future::{Ready, ready};

use actix_web::dev::Payload;
use actix_web::web::Data;
use actix_web::{Error, FromRequest, HttpRequest};

use super::GuardNotConfigured;
use crate::guard::Guard;

impl FromRequest for Guard {
    type Error = Error;
    /// 纯同步取值，不需要装箱 —— 拷贝一次 `Arc` 而已。
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        match req.app_data::<Data<Guard>>() {
            Some(data) => {
                // `Data<T>` 的 `Deref::Target` 是 `Arc<T>`（不是 `T`），所以这里
                // 靠连续两次解引用拿到 `&Guard`，而不是 `(**data).clone()`
                // —— 那会得到一个 `Arc<Guard>`。
                let guard: &Guard = data;
                ready(Ok(guard.clone()))
            }
            // 忘了 app_data(...) —— 装配错误，500 而不是 4xx
            None => ready(Err(actix_web::error::ErrorInternalServerError(
                GuardNotConfigured,
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snowflake;
    use actix_web::test;

    fn guard() -> Guard {
        Guard::new(Snowflake::default())
    }

    #[actix_web::test]
    async fn extracts_from_app_data() {
        let req = test::TestRequest::default()
            .app_data(Data::new(guard()))
            .to_http_request();

        let extracted = Guard::from_request(&req, &mut Payload::None).await.unwrap();
        assert!(extracted.id().unwrap() > 0);
    }

    /// 忘了注册时给出明确的 500，而不是 panic 或 400。
    #[actix_web::test]
    async fn missing_app_data_is_a_500() {
        let req = test::TestRequest::default().to_http_request();
        // 不用 unwrap_err()：它要求 Ok 侧（Guard）实现 Debug，而 Guard 故意没有
        let err = match Guard::from_request(&req, &mut Payload::None).await {
            Err(err) => err,
            Ok(_) => panic!("没注册 app_data 时应当报错"),
        };
        assert_eq!(
            err.as_response_error().status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// 处理器拿到的与注册进去的是同一个发号器：同一实例严格单调，
    /// 提出的 ID 必须大于注册方先前发出的那个。
    #[actix_web::test]
    async fn handler_and_state_share_one_generator() {
        let state = guard();
        let previous = state.id().unwrap();

        let req = test::TestRequest::default()
            .app_data(Data::new(state))
            .to_http_request();
        let extracted = Guard::from_request(&req, &mut Payload::None).await.unwrap();

        assert!(extracted.id().unwrap() > previous);
    }

    /// `Data::from(arc)` 复用同一个分配，不嵌套第二层 Arc。
    #[actix_web::test]
    async fn data_from_arc_shares_the_allocation() {
        use std::sync::Arc;

        let shared = Arc::new(guard());
        let previous = shared.id().unwrap();

        let req = test::TestRequest::default()
            .app_data(Data::from(shared.clone()))
            .to_http_request();
        let extracted = Guard::from_request(&req, &mut Payload::None).await.unwrap();

        assert!(extracted.id().unwrap() > previous);
        // 原 Arc 与 Data 里的仍是同一份
        assert_eq!(Arc::strong_count(&shared), 2);
    }
}
