use crate::{error::AppError, state::WebState};
use axum::{extract::State, Json};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct Request {
    operation: String,
    request: dbx_core::xxljob::XxlJobRequest,
}

pub async fn request(
    State(state): State<Arc<WebState>>,
    Json(body): Json<Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    dbx_core::xxljob::execute(&state.app, &body.operation, body.request).await.map(Json).map_err(AppError::from)
}
