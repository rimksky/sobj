use axum::http::HeaderMap;
use axum::response::Response;
use std::sync::atomic::Ordering;

use crate::{error::unauthorized, state::AppState};

pub fn require_auth(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    let Some(expected) = state.auth_token.as_ref() else {
        return Ok(()); // 認証無効
    };

    let v = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if v == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

pub fn inc_in_flight(state: &AppState) -> usize {
    state.in_flight.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn dec_in_flight(state: &AppState) -> usize {
    state.in_flight
        .fetch_sub(1, Ordering::SeqCst)
        .saturating_sub(1)
}
