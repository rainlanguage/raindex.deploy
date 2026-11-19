use crate::fairings::{request_id_for, request_span_for};
use rocket::{
    http::{Header, Status},
    response::Responder,
    serde::json::Json,
    Request, Response,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiErrorCode {
    BadRequest,
    NotFound,
    RateLimited,
    UpstreamUnavailable,
    InternalError,
}

impl ApiErrorCode {
    fn status(self) -> Status {
        match self {
            Self::BadRequest => Status::BadRequest,
            Self::NotFound => Status::NotFound,
            Self::RateLimited => Status::TooManyRequests,
            Self::UpstreamUnavailable => Status::ServiceUnavailable,
            Self::InternalError => Status::InternalServerError,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorDetail {
    pub code: ApiErrorCode,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorResponse {
    pub request_id: String,
    pub error: ApiErrorDetail,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("upstream unavailable: {0}")]
    UpstreamUnavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    fn code(&self) -> ApiErrorCode {
        match self {
            Self::BadRequest(_) => ApiErrorCode::BadRequest,
            Self::NotFound(_) => ApiErrorCode::NotFound,
            Self::RateLimited(_) => ApiErrorCode::RateLimited,
            Self::UpstreamUnavailable(_) => ApiErrorCode::UpstreamUnavailable,
            Self::Internal(_) => ApiErrorCode::InternalError,
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::NotFound(message)
            | Self::RateLimited(message)
            | Self::UpstreamUnavailable(message)
            | Self::Internal(message) => message.clone(),
        }
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        let code = self.code();
        let status = code.status();
        request_span_for(request).in_scope(|| {
            if status.code >= 500 {
                tracing::error!(error = %self, "request failed");
            } else {
                tracing::warn!(error = %self, "request failed");
            }
        });
        let body = ApiErrorResponse {
            request_id: request_id_for(request),
            error: ApiErrorDetail {
                code,
                message: self.public_message(),
            },
        };
        let response = Json(body).respond_to(request)?;
        let mut response = Response::build_from(response).status(status).finalize();
        if matches!(code, ApiErrorCode::RateLimited) {
            response.set_header(Header::new("Retry-After", "60"));
        }
        Ok(response)
    }
}
