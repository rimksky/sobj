use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::fs;

use crate::{
    auth::{dec_in_flight, inc_in_flight, require_auth},
    error::{conflict, not_found, server_error},
    key::{key_to_path, normalize_key},
    state::AppState,
};

#[derive(Deserialize)]
pub struct CopyMoveReq {
    pub src: String,
    pub dst: String,
    pub overwrite: Option<bool>,
}

#[derive(Serialize)]
struct CopyMoveResp {
    src: String,
    dst: String,
    size: u64,
}

/* ---------------- COPY ---------------- */

pub async fn copy_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<CopyMoveReq>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="COPY", src=%req.src, dst=%req.dst, "request");
    let resp = copy_object_impl(state.clone(), headers, req).await;
    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn copy_object_impl(state: AppState, headers: HeaderMap, req: CopyMoveReq) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }
    let overwrite = req.overwrite.unwrap_or(true);

    let src_key = match normalize_key(&req.src) { Ok(k) => k, Err(r) => return r };
    let dst_key = match normalize_key(&req.dst) { Ok(k) => k, Err(r) => return r };

    let src_path = match key_to_path(&state.storage_dir, &src_key) { Ok(p) => p, Err(r) => return r };
    let dst_path = match key_to_path(&state.storage_dir, &dst_key) { Ok(p) => p, Err(r) => return r };

    let src_meta = match fs::metadata(&src_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return not_found(),
        Err(e) => return server_error(e),
    };

    if !overwrite && fs::metadata(&dst_path).await.is_ok() {
        return conflict("destination exists");
    }

    if let Some(parent) = dst_path.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return server_error(e);
        }
    }

    match fs::copy(&src_path, &dst_path).await {
        Ok(_) => (StatusCode::OK, Json(CopyMoveResp {
            src: src_key,
            dst: dst_key,
            size: src_meta.len(),
        })).into_response(),
        Err(e) => server_error(e),
    }
}

/* ---------------- MOVE ---------------- */

pub async fn move_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<CopyMoveReq>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="MOVE", src=%req.src, dst=%req.dst, "request");
    let resp = move_object_impl(state.clone(), headers, req).await;
    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn move_object_impl(state: AppState, headers: HeaderMap, req: CopyMoveReq) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }
    let overwrite = req.overwrite.unwrap_or(true);

    let src_key = match normalize_key(&req.src) { Ok(k) => k, Err(r) => return r };
    let dst_key = match normalize_key(&req.dst) { Ok(k) => k, Err(r) => return r };

    let src_path = match key_to_path(&state.storage_dir, &src_key) { Ok(p) => p, Err(r) => return r };
    let dst_path = match key_to_path(&state.storage_dir, &dst_key) { Ok(p) => p, Err(r) => return r };

    let src_meta = match fs::metadata(&src_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return not_found(),
        Err(e) => return server_error(e),
    };

    if !overwrite {
        if fs::metadata(&dst_path).await.is_ok() {
            return conflict("destination exists");
        }
    } else {
        let _ = fs::remove_file(&dst_path).await;
    }

    if let Some(parent) = dst_path.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return server_error(e);
        }
    }

    match fs::rename(&src_path, &dst_path).await {
        Ok(_) => (StatusCode::OK, Json(CopyMoveResp {
            src: src_key,
            dst: dst_key,
            size: src_meta.len(),
        })).into_response(),
        Err(_) => match fs::copy(&src_path, &dst_path).await {
            Ok(_) => {
                let _ = fs::remove_file(&src_path).await;
                (StatusCode::OK, Json(CopyMoveResp {
                    src: src_key,
                    dst: dst_key,
                    size: src_meta.len(),
                })).into_response()
            }
            Err(e) => server_error(e),
        },
    }
}
