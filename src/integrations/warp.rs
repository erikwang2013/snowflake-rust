// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! warp 0.4 集成。
//!
//! warp 没有提取器 trait，也没有 `with_state` —— 它靠**闭包捕获**把一个
//! `Clone` 的值接进过滤器链。所以这里没有 `impl`，只有一个组合子：
//!
//! ```no_run
//! use snowflake::guard::Guard;
//! use snowflake::integrations::warp::with_guard;
//! use warp::Filter;
//!
//! let guard = Guard::new(snowflake::Snowflake::default());
//! let route = warp::path("next")
//!     .and(with_guard(guard))
//!     .map(|guard: Guard| guard.id().map(|id| id.to_string()).unwrap_or_default());
//! # let _ = route;
//! ```
//!
//! 这与 warp 文档里 `warp::any().map(move || state.clone())` 的写法是同一件事，
//! 只是把「克隆共享状态」这一步收进了一个有名字的组合子，免得每个项目各写一遍。

use std::convert::Infallible;

use warp::Filter;

use crate::guard::Guard;

/// 造一个产出 [`Guard`] 的过滤器，供 `.and(...)` 接进路由链。
///
/// 每次请求克隆一次 `Arc`（引用计数递增），不含任何状态拷贝。
pub fn with_guard(guard: Guard) -> impl Filter<Extract = (Guard,), Error = Infallible> + Clone {
    warp::any().map(move || guard.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snowflake;
    use warp::Filter;

    fn guard() -> Guard {
        Guard::new(Snowflake::default())
    }

    #[tokio::test]
    async fn filter_yields_the_guard() {
        let route = warp::any()
            .and(with_guard(guard()))
            .map(|g: Guard| g.id().unwrap().to_string());

        let value = warp::test::request().filter(&route).await.unwrap();
        assert!(value.parse::<i64>().unwrap() > 0);
    }

    /// 处理器拿到的与外面注册的是同一个发号器：同一实例严格单调。
    #[tokio::test]
    async fn handler_and_state_share_one_generator() {
        let state = guard();
        let previous = state.id().unwrap();

        let route = warp::any().and(with_guard(state)).map(move |g: Guard| {
            let id = g.id().unwrap();
            assert!(id > previous, "不是同一个发号器: {id} <= {previous}");
            id.to_string()
        });

        let value = warp::test::request().filter(&route).await.unwrap();
        assert!(value.parse::<i64>().unwrap() > previous);
    }

    /// 过滤器必须 `Clone` —— warp 的每个组合子都要求这一点。
    #[tokio::test]
    async fn filter_is_cloneable_and_reusable() {
        let filter = with_guard(guard());
        let a = filter.clone();
        let b = filter;

        let route_a = warp::any().and(a).map(|g: Guard| g.id().is_ok());
        let route_b = warp::any().and(b).map(|g: Guard| g.id().is_ok());

        assert!(warp::test::request().filter(&route_a).await.unwrap());
        assert!(warp::test::request().filter(&route_b).await.unwrap());
    }
}
