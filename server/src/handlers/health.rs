use axum::{extract::State, Json};
use std::sync::atomic::Ordering;

use crate::state::{AppState, HealthzResponse};

pub async fn health(State(state): State<AppState>) -> Json<HealthzResponse> {
    Json(HealthzResponse {
        app: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        status: "ok",
        in_flight: state.in_flight.load(Ordering::Relaxed),
    })
}
