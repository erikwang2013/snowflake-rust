// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! rocket 0.5 集成。
//!
//! 用 `manage` 注册，处理器把它写在参数里：
//!
//! ```no_run
//! # mod demo {
//! use rocket::{get, routes, Build, Rocket};
//! use snowflake::guard::Guard;
//!
//! #[get("/")]
//! fn create_order(guard: Guard) -> String {
//!     guard.id().map(|id| id.to_string()).unwrap_or_default()
//! }
//!
//! # fn main() {
//! let guard = Guard::new(snowflake::Snowflake::default());
//! let rocket: Rocket<Build> = rocket::build().manage(guard).mount("/", routes![create_order]);
//! # let _ = rocket;
//! # }
//! # }
//! ```
//!
//! rocket 的请求守卫走 `Outcome`，且 trait 是 `#[rocket::async_trait]` 的
//! （rocket 自己 re-export 了那个宏，所以不必额外依赖 `async-trait`）。
//!
//! # 托管状态里放的是 [`Guard`] 本身
//!
//! [`Guard`] 内部已经是 `Arc`，克隆一次只是引用计数递增，所以这里是
//! `manage(guard)` 而不是 `manage(Arc::new(guard))` —— rocket 的守卫拿到的是
//! 借用，本来也不需要外面再套一层共享。

use rocket::State;
use rocket::http::Status;
// 注意是 `request::Outcome`（两参数别名 = `outcome::Outcome<S, (Status, E), Status>`），
// 不是 `outcome::Outcome`（三参数）。守卫的返回值用的就是这个别名。
use rocket::request::{FromRequest, Outcome, Request};

use super::GuardNotConfigured;
use crate::guard::Guard;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Guard {
    type Error = GuardNotConfigured;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.guard::<&State<Guard>>().await {
            Outcome::Success(state) => Outcome::Success(state.inner().clone()),
            // 忘了 manage(...)：装配错误 → 500，不 Forward（没有别的路由能处理它）。
            // 别名的 Error 分支装的是 `(Status, E)` 而不是裸的 E。
            Outcome::Error(_) | Outcome::Forward(_) => {
                Outcome::Error((Status::InternalServerError, GuardNotConfigured))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::local::blocking::Client;
    use rocket::{get, routes};
    use crate::Snowflake;

    fn guard() -> Guard {
        Guard::new(Snowflake::default())
    }

    #[get("/next")]
    fn next_id_route(guard: Guard) -> String {
        guard.id().unwrap().to_string()
    }

    #[get("/parse/<id>")]
    fn parse_route(guard: Guard, id: i64) -> String {
        let parsed = guard.parse_id(id);
        format!("{}:{}", parsed.worker_id, parsed.timestamp_ms)
    }

    #[test]
    fn guard_is_injectable_end_to_end() {
        let rocket = rocket::build()
            .manage(guard())
            .mount("/", routes![next_id_route, parse_route]);
        let client = Client::tracked(rocket).expect("rocket 应当能启动");

        let res = client.get("/next").dispatch();
        assert_eq!(res.status(), Status::Ok);
        let first: i64 = res.into_string().unwrap().parse().unwrap();
        assert!(first > 0);

        // 第二个请求：同一个实例严格单调 —— 拿到同一个守卫的证据
        let res = client.get("/next").dispatch();
        let second: i64 = res.into_string().unwrap().parse().unwrap();
        assert!(second > first, "两个请求不是同一个发号器: {second} <= {first}");

        // 刚发出来的 ID 能被同一个守卫反解
        let res = client.get(format!("/parse/{first}")).dispatch();
        assert_eq!(res.status(), Status::Ok);
        let parsed = res.into_string().unwrap();
        assert!(parsed.starts_with("0:"), "worker 0 的时间戳串，实际 {parsed:?}");
    }

    /// 忘了 `manage(...)` 时是 500，不是 panic。
    #[test]
    fn missing_managed_state_is_a_500() {
        let rocket = rocket::build().mount("/", routes![next_id_route]);
        let client = Client::tracked(rocket).expect("rocket 应当能启动");

        let res = client.get("/next").dispatch();
        assert_eq!(res.status(), Status::InternalServerError);
    }
}
