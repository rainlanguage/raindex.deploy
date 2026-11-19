use crate::{error::ApiError, fairings::PublicRateLimit, AppState};
use rocket::{serde::json::Json, State};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetailedHealthResponse {
    pub status: &'static str,
    pub sync_configured: bool,
    pub sync_healthy: bool,
    pub network_count: usize,
    pub orderbook_count: usize,
    pub snapshot_ready: bool,
    pub snapshot_last_success_at: Option<u64>,
    pub snapshot_refresh_healthy: bool,
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = HealthResponse)))]
#[get("/health")]
pub fn health(_limit: PublicRateLimit) -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[utoipa::path(get, path = "/health/detailed", responses((status = 200, body = DetailedHealthResponse)))]
#[get("/health/detailed")]
pub async fn detailed_health(
    _limit: PublicRateLimit,
    state: &State<AppState>,
) -> Result<Json<DetailedHealthResponse>, ApiError> {
    let snapshot = state.source.health().await.map_err(|error| {
        tracing::error!(error = %error, "detailed health check failed");
        ApiError::Internal("health data is temporarily unavailable".into())
    })?;
    Ok(Json(DetailedHealthResponse {
        status: if snapshot.healthy { "ok" } else { "degraded" },
        sync_configured: snapshot.configured,
        sync_healthy: snapshot.sync_healthy,
        network_count: snapshot.network_count,
        orderbook_count: snapshot.orderbook_count,
        snapshot_ready: snapshot.snapshot_ready,
        snapshot_last_success_at: snapshot.snapshot_last_success_at,
        snapshot_refresh_healthy: snapshot.snapshot_refresh_healthy,
    }))
}

pub fn routes() -> Vec<rocket::Route> {
    routes![health, detailed_health]
}
