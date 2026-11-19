#[macro_use]
extern crate rocket;

mod cache;
mod catchers;
mod config;
mod error;
mod fairings;
mod provider;
mod routes;
mod service;
mod telemetry;

use cache::MarketSnapshotCache;
use config::Config;
use provider::RaindexProvider;
use rocket_cors::{AllowedHeaders, AllowedMethods, AllowedOrigins, CorsOptions};
use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub struct AppState {
    source: Arc<dyn service::MarketDataSource>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::health::health,
        routes::health::detailed_health,
        routes::markets::tickers,
        routes::markets::orderbook,
        routes::markets::markets,
    ),
    components(schemas(
        error::ApiErrorCode,
        error::ApiErrorDetail,
        error::ApiErrorResponse,
        routes::health::HealthResponse,
        routes::health::DetailedHealthResponse,
        routes::markets::TickerResponse,
        routes::markets::OrderbookResponse,
    )),
    tags(
        (name = "Markets", description = "Public ticker and orderbook compatibility endpoints"),
        (name = "Raindex", description = "Raindex market data"),
        (name = "Health", description = "Service and indexer health")
    ),
    info(
        title = "Raindex Market Data API",
        description = "Public cached market data backed by the Raindex local indexer",
    )
)]
struct ApiDoc;

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Provider(#[from] provider::ProviderError),
    #[error("failed to warm market cache: {0}")]
    CacheWarm(String),
    #[error("invalid CORS method: {0}")]
    CorsMethod(String),
    #[error(transparent)]
    Cors(#[from] rocket_cors::Error),
    #[error("Rocket failed: {0}")]
    Rocket(String),
    #[error("failed to initialize logging: {0}")]
    Logging(String),
}

fn cors() -> Result<rocket_cors::Cors, StartupError> {
    let methods = ["Get", "Options"]
        .into_iter()
        .map(|method| {
            rocket_cors::Method::from_str(method)
                .map_err(|_| StartupError::CorsMethod(method.to_string()))
        })
        .collect::<Result<AllowedMethods, _>>()?;
    Ok(CorsOptions {
        allowed_origins: AllowedOrigins::all(),
        allowed_methods: methods,
        allowed_headers: AllowedHeaders::all(),
        expose_headers: HashSet::from([
            "X-Request-Id".to_string(),
            "Retry-After".to_string(),
            "X-RateLimit-Limit".to_string(),
            "X-RateLimit-Remaining".to_string(),
            "X-RateLimit-Reset".to_string(),
        ]),
        ..Default::default()
    }
    .to_cors()?)
}

fn rocket_figment(trusted_proxy_ip_header: Option<&str>) -> rocket::figment::Figment {
    let figment = rocket::Config::figment();
    match trusted_proxy_ip_header {
        Some(header) => figment.merge((rocket::Config::IP_HEADER, header)),
        None => figment.merge((rocket::Config::IP_HEADER, false)),
    }
}

async fn build(config: Config) -> Result<rocket::Rocket<rocket::Build>, StartupError> {
    let provider =
        Arc::new(RaindexProvider::load(&config.registry_url, config.local_db_path.clone()).await?);
    let cache = MarketSnapshotCache::new(
        Duration::from_secs(config.cache_ttl_seconds),
        config.snapshot_recent_trades_limit,
    );
    let market_data = Arc::new(service::CachedMarketData::new(provider, cache));
    tracing::info!(
        timeout_seconds = config.local_db_ready_timeout_seconds,
        "waiting for local market index readiness"
    );
    market_data
        .wait_for_local_index(Duration::from_secs(config.local_db_ready_timeout_seconds))
        .await
        .map_err(|error| StartupError::CacheWarm(error.to_string()))?;
    tracing::info!("warming market snapshot cache");
    market_data
        .warm()
        .await
        .map_err(|error| StartupError::CacheWarm(error.to_string()))?;
    tracing::info!("market snapshot cache is ready");
    market_data
        .clone()
        .start_background_refresh(Duration::from_secs(config.cache_ttl_seconds));
    let state = AppState {
        source: market_data,
    };
    Ok(
        rocket::custom(rocket_figment(config.trusted_proxy_ip_header.as_deref()))
            .manage(state)
            .manage(fairings::RateLimiter::new(
                config.rate_limit_global_rpm,
                config.rate_limit_per_ip_rpm,
            ))
            .mount("/", routes::health::routes())
            .mount("/", routes::markets::compatibility_routes())
            .mount("/v1", routes::markets::raindex_routes())
            .mount(
                "/",
                SwaggerUi::new("/swagger/<_..>").url("/api-doc/openapi.json", ApiDoc::openapi()),
            )
            .register("/", catchers::catchers())
            .attach(fairings::RequestLogger)
            .attach(fairings::RateLimitHeaders)
            .attach(cors()?),
    )
}

#[rocket::main]
async fn main() -> Result<(), StartupError> {
    let config = Config::from_env()?;
    let _logging_guard = telemetry::init(&config.log_dir)
        .map_err(|error| StartupError::Logging(error.to_string()))?;
    build(config)
        .await?
        .launch()
        .await
        .map_err(|error| StartupError::Rocket(error.to_string()))?;
    Ok(())
}
