use crate::error::ApiError;
use rocket::{
    fairing::{Fairing, Info, Kind},
    http::{Header, Status},
    request::{FromRequest, Outcome},
    Data, Request, Response,
};
use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const WINDOW: Duration = Duration::from_secs(60);
const REQUEST_ID_HEADER: &str = "X-Request-Id";
const PER_IP_CLEANUP_EVERY: u64 = 1_024;

struct RequestMeta {
    started: Instant,
    request_id: String,
    span: tracing::Span,
}

fn fallback_meta() -> RequestMeta {
    RequestMeta {
        started: Instant::now(),
        request_id: "unknown".into(),
        span: tracing::Span::none(),
    }
}

pub fn request_span_for(request: &Request<'_>) -> tracing::Span {
    request.local_cache(fallback_meta).span.clone()
}

pub fn request_id_for(request: &Request<'_>) -> String {
    request.local_cache(fallback_meta).request_id.clone()
}

pub struct RequestLogger;

#[rocket::async_trait]
impl Fairing for RequestLogger {
    fn info(&self) -> Info {
        Info {
            name: "Request logger",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _data: &mut Data<'_>) {
        let request_id = request
            .headers()
            .get_one(REQUEST_ID_HEADER)
            .map(str::trim)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value.is_ascii()
                    && !value.chars().any(char::is_control)
            })
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let span = tracing::info_span!(
            "request",
            method = %request.method(),
            uri = %request.uri(),
            request_id = %request_id
        );
        span.in_scope(|| tracing::info!("request started"));
        request.local_cache(|| RequestMeta {
            started: Instant::now(),
            request_id,
            span,
        });
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let meta = request.local_cache(fallback_meta);
        let duration_ms = meta.started.elapsed().as_secs_f64() * 1_000.0;
        meta.span.in_scope(|| {
            tracing::info!(
                status = response.status().code,
                duration_ms,
                "request completed"
            )
        });
        response.set_header(Header::new(REQUEST_ID_HEADER, meta.request_id.clone()));
    }
}

#[derive(Clone)]
pub struct RateLimitInfo {
    limit: u64,
    remaining: u64,
    reset: u64,
    allowed: bool,
}

struct CachedRateLimitInfo(Mutex<Option<RateLimitInfo>>);

pub struct RateLimiter {
    global_rpm: u64,
    per_ip_rpm: u64,
    state: Mutex<RateLimitState>,
    per_ip_check_count: AtomicU64,
}

#[derive(Default)]
struct RateLimitState {
    global: VecDeque<Instant>,
    per_ip: HashMap<IpAddr, VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(global_rpm: u64, per_ip_rpm: u64) -> Self {
        Self {
            global_rpm,
            per_ip_rpm,
            state: Mutex::new(RateLimitState::default()),
            per_ip_check_count: AtomicU64::new(0),
        }
    }

    fn check(&self, ip: IpAddr) -> Result<RateLimitInfo, ApiError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ApiError::Internal("rate limiter unavailable".into()))?;
        let now = Instant::now();
        let cutoff = now - WINDOW;
        prune(&mut state.global, cutoff);
        let check_count = self.per_ip_check_count.fetch_add(1, Ordering::Relaxed) + 1;
        if check_count.is_multiple_of(PER_IP_CLEANUP_EVERY) {
            state.per_ip.retain(|_, window| {
                prune(window, cutoff);
                !window.is_empty()
            });
        }

        let global_allowed = self.global_rpm == 0 || state.global.len() < self.global_rpm as usize;
        let per_ip = state.per_ip.entry(ip).or_default();
        prune(per_ip, cutoff);
        let per_ip_allowed = self.per_ip_rpm == 0 || per_ip.len() < self.per_ip_rpm as usize;
        if !global_allowed || !per_ip_allowed {
            let (limit, reset) = if !per_ip_allowed {
                (self.per_ip_rpm, reset_at(per_ip, now))
            } else {
                (self.global_rpm, reset_at(&state.global, now))
            };
            return Ok(RateLimitInfo {
                limit,
                remaining: 0,
                reset,
                allowed: false,
            });
        }

        if self.global_rpm > 0 {
            state.global.push_back(now);
        }
        if self.per_ip_rpm > 0 {
            state.per_ip.entry(ip).or_default().push_back(now);
        }
        let (limit, remaining, reset) = if self.per_ip_rpm > 0 {
            let window = state.per_ip.get(&ip);
            (
                self.per_ip_rpm,
                self.per_ip_rpm
                    .saturating_sub(window.map_or(0, |window| window.len() as u64)),
                window.map_or_else(|| unix_now() + 60, |window| reset_at(window, now)),
            )
        } else {
            (
                self.global_rpm,
                self.global_rpm.saturating_sub(state.global.len() as u64),
                reset_at(&state.global, now),
            )
        };
        Ok(RateLimitInfo {
            limit,
            remaining,
            reset,
            allowed: true,
        })
    }
}

fn prune(window: &mut VecDeque<Instant>, cutoff: Instant) {
    while window.front().is_some_and(|instant| *instant < cutoff) {
        window.pop_front();
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn reset_at(window: &VecDeque<Instant>, now: Instant) -> u64 {
    window.front().map_or_else(
        || unix_now() + WINDOW.as_secs(),
        |oldest| unix_now() + (*oldest + WINDOW).saturating_duration_since(now).as_secs(),
    )
}

pub struct PublicRateLimit;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PublicRateLimit {
    type Error = ApiError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(limiter) = request.rocket().state::<RateLimiter>() else {
            return Outcome::Error((
                Status::InternalServerError,
                ApiError::Internal("rate limiter unavailable".into()),
            ));
        };
        let ip = request
            .client_ip()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        match limiter.check(ip) {
            Ok(info) if info.allowed => {
                let cache = request.local_cache(|| CachedRateLimitInfo(Mutex::new(None)));
                if let Ok(mut value) = cache.0.lock() {
                    *value = Some(info);
                }
                Outcome::Success(Self)
            }
            Ok(info) => {
                let cache = request.local_cache(|| CachedRateLimitInfo(Mutex::new(None)));
                if let Ok(mut value) = cache.0.lock() {
                    *value = Some(info);
                }
                Outcome::Error((
                    Status::TooManyRequests,
                    ApiError::RateLimited("too many requests; try again later".into()),
                ))
            }
            Err(error) => Outcome::Error((Status::InternalServerError, error)),
        }
    }
}

pub struct RateLimitHeaders;

#[rocket::async_trait]
impl Fairing for RateLimitHeaders {
    fn info(&self) -> Info {
        Info {
            name: "Rate limit headers",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let cache = request.local_cache(|| CachedRateLimitInfo(Mutex::new(None)));
        if let Ok(value) = cache.0.lock() {
            if let Some(info) = value.as_ref() {
                response.set_header(Header::new("X-RateLimit-Limit", info.limit.to_string()));
                response.set_header(Header::new(
                    "X-RateLimit-Remaining",
                    info.remaining.to_string(),
                ));
                response.set_header(Header::new("X-RateLimit-Reset", info.reset.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_enforces_per_ip_limit() {
        let limiter = RateLimiter::new(100, 2);
        let ip = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        assert!(limiter.check(ip).expect("first").allowed);
        assert!(limiter.check(ip).expect("second").allowed);
        assert!(!limiter.check(ip).expect("blocked").allowed);
    }

    #[test]
    fn limiter_amortizes_stale_ip_cleanup() {
        let limiter = RateLimiter::new(0, 1);
        let stale = Instant::now() - Duration::from_secs(61);
        {
            let mut state = limiter.state.lock().expect("rate-limit state");
            (1..=5).for_each(|last_octet| {
                state.per_ip.insert(
                    IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, last_octet)),
                    VecDeque::from([stale]),
                );
            });
        }
        let active_ip = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        (0..PER_IP_CLEANUP_EVERY).for_each(|_| {
            let _ = limiter.check(active_ip).expect("rate-limit check");
        });
        let state = limiter.state.lock().expect("rate-limit state");
        assert_eq!(state.per_ip.len(), 1);
        assert!(state.per_ip.contains_key(&active_ip));
    }
}
