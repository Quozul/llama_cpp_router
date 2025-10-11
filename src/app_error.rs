use crate::services::backend_server_manager::EstimateError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

impl IntoResponse for EstimateError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
