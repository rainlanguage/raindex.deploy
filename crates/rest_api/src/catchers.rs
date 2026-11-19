use crate::error::ApiError;

#[catch(400)]
fn bad_request() -> ApiError {
    ApiError::BadRequest("the request could not be understood".into())
}

#[catch(404)]
fn not_found() -> ApiError {
    ApiError::NotFound("route not found".into())
}

#[catch(422)]
fn unprocessable() -> ApiError {
    ApiError::BadRequest("invalid path or query parameters".into())
}

#[catch(429)]
fn rate_limited() -> ApiError {
    ApiError::RateLimited("too many requests; try again later".into())
}

#[catch(500)]
fn internal_error() -> ApiError {
    ApiError::Internal("internal server error".into())
}

pub fn catchers() -> Vec<rocket::Catcher> {
    catchers![
        bad_request,
        not_found,
        unprocessable,
        rate_limited,
        internal_error
    ]
}
