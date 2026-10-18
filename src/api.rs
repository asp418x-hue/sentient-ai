use crate::liblink_struct::CothinkSystem;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_TASKS_PER_DISPATCH: usize = 256;
const MAX_CONCURRENT_AGENTS: usize = 32;
const MAX_CONTEXT_BYTES: usize = 1_048_576;

static NEXT_ORCHESTRATOR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct ApiState {
    pub native: Option<Arc<CothinkSystem>>,
}

#[derive(Debug, Deserialize)]
pub struct DispatchRequest {
    pub task_ids: Vec<usize>,
    pub context: String,
    pub max_concurrent: Option<usize>,
    pub time_dilation_factor: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

#[derive(Debug)]
struct ApiFailure(StatusCode, String);

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        (self.0, Json(ApiError { error: self.1 })).into_response()
    }
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/dispatch", post(dispatch))
        .with_state(state)
}

async fn health() -> &'static str {
    "Sentient-AI Core Active"
}

async fn dispatch(
    State(state): State<ApiState>,
    Json(request): Json<DispatchRequest>,
) -> Result<Json<Value>, ApiFailure> {
    let (max_concurrent, time_dilation_factor) = validate_request(&request)?;
    let native = state.native.ok_or_else(|| {
        ApiFailure(
            StatusCode::SERVICE_UNAVAILABLE,
            "native cothink dispatch is not configured".to_string(),
        )
    })?;
    let orchestrator_id = NEXT_ORCHESTRATOR_ID.fetch_add(1, Ordering::Relaxed);
    let task_ids = request.task_ids;
    let context = request.context;

    let response = tokio::task::spawn_blocking(move || {
        native.dispatch_agents(
            orchestrator_id,
            &task_ids,
            &context,
            max_concurrent,
            time_dilation_factor,
        )
    })
    .await
    .map_err(|error| {
        ApiFailure(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("dispatch worker failed: {error}"),
        )
    })?
    .map_err(|error| ApiFailure(StatusCode::BAD_GATEWAY, error))?;

    Ok(Json(response))
}

fn validate_request(request: &DispatchRequest) -> Result<(usize, f64), ApiFailure> {
    if request.task_ids.is_empty() {
        return Err(ApiFailure(
            StatusCode::BAD_REQUEST,
            "task_ids must contain at least one task".to_string(),
        ));
    }
    if request.task_ids.len() > MAX_TASKS_PER_DISPATCH {
        return Err(ApiFailure(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("task_ids may contain at most {MAX_TASKS_PER_DISPATCH} entries"),
        ));
    }
    if request.context.len() > MAX_CONTEXT_BYTES {
        return Err(ApiFailure(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("context may not exceed {MAX_CONTEXT_BYTES} bytes"),
        ));
    }

    let max_concurrent = request.max_concurrent.unwrap_or(8);
    if !(1..=MAX_CONCURRENT_AGENTS).contains(&max_concurrent) {
        return Err(ApiFailure(
            StatusCode::BAD_REQUEST,
            format!("max_concurrent must be between 1 and {MAX_CONCURRENT_AGENTS}"),
        ));
    }

    let time_dilation_factor = request.time_dilation_factor.unwrap_or(1.0);
    if !time_dilation_factor.is_finite() || !(0.01..=100.0).contains(&time_dilation_factor) {
        return Err(ApiFailure(
            StatusCode::BAD_REQUEST,
            "time_dilation_factor must be between 0.01 and 100".to_string(),
        ));
    }

    let mut task_ids = request.task_ids.clone();
    task_ids.sort_unstable();
    task_ids.dedup();
    if task_ids.len() != request.task_ids.len() {
        return Err(ApiFailure(
            StatusCode::BAD_REQUEST,
            "task_ids must be unique".to_string(),
        ));
    }

    Ok((max_concurrent, time_dilation_factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DispatchRequest {
        DispatchRequest {
            task_ids: vec![1, 2],
            context: "inspect the requested input".to_string(),
            max_concurrent: None,
            time_dilation_factor: None,
        }
    }

    #[test]
    fn accepts_dispatch_with_default_limits_and_normal_time() {
        assert_eq!(validate_request(&request()).unwrap(), (8, 1.0));
    }

    #[test]
    fn accepts_contracted_and_dilated_macro_time() {
        let mut contracted = request();
        contracted.time_dilation_factor = Some(0.5);
        assert_eq!(validate_request(&contracted).unwrap().1, 0.5);

        let mut dilated = request();
        dilated.time_dilation_factor = Some(4.0);
        assert_eq!(validate_request(&dilated).unwrap().1, 4.0);
    }

    #[test]
    fn rejects_empty_duplicate_and_oversized_dispatches() {
        let mut empty = request();
        empty.task_ids.clear();
        assert_eq!(
            validate_request(&empty).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );

        let mut duplicate = request();
        duplicate.task_ids = vec![1, 1];
        assert_eq!(
            validate_request(&duplicate).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );

        let mut oversized = request();
        oversized.context = "x".repeat(MAX_CONTEXT_BYTES + 1);
        assert_eq!(
            validate_request(&oversized).unwrap_err().0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
