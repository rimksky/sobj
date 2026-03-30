use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use serde::Serialize;
use std::{net::SocketAddr, sync::atomic::Ordering};
use tokio::{fs, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::{
    auth::{dec_in_flight, inc_in_flight, require_auth},
    error::{not_found, server_error},
    key::{key_to_path, normalize_key},
    state::AppState,
};

/* ---------------- PUT ---------------- */

#[derive(Serialize)]
struct PutResp {
    key: String,
    size: u64,
}

pub async fn put_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Body,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="PUT", key=%key, "request");

    let resp = put_object_impl(state.clone(), headers, key, body).await;

    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn put_object_impl(state: AppState, headers: HeaderMap, key: String, body: Body) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }

    let key = match normalize_key(&key) {
        Ok(k) => k,
        Err(r) => return r,
    };

    let path = match key_to_path(&state.storage_dir, &key) {
        Ok(p) => p,
        Err(r) => return r,
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return server_error(e);
        }
    }

    // 並行 PUT が同じキーに書き込む際に tmp ファイルが衝突しないよう
    // upload_counter でユニークな ID を付与する
    let upload_id = state.upload_counter.fetch_add(1, Ordering::Relaxed);
    let tmp_path = path.with_extension(format!("{}.uploading.tmp", upload_id));

    let mut file = match fs::File::create(&tmp_path).await {
        Ok(f) => f,
        Err(e) => return server_error(e),
    };

    let mut stream = body.into_data_stream();
    let mut size: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = fs::remove_file(&tmp_path).await;
                return server_error(e);
            }
        };
        size += chunk.len() as u64;
        if let Err(e) = file.write_all(&chunk).await {
            let _ = fs::remove_file(&tmp_path).await;
            return server_error(e);
        }
    }
    if let Err(e) = file.flush().await {
        let _ = fs::remove_file(&tmp_path).await;
        return server_error(e);
    }
    if let Err(e) = fs::rename(&tmp_path, &path).await {
        let _ = fs::remove_file(&tmp_path).await;
        return server_error(e);
    }

    (StatusCode::CREATED, Json(PutResp { key, size })).into_response()
}

/* ---------------- GET (ストリーミング) ---------------- */

pub async fn get_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="GET", key=%key, "request");

    let resp = get_object_impl(state.clone(), headers, key).await;

    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn get_object_impl(state: AppState, headers: HeaderMap, key: String) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }

    let key = match normalize_key(&key) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let path = match key_to_path(&state.storage_dir, &key) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let meta = match fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return not_found(),
        Err(e) => return server_error(e),
    };

    let file = match fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => return server_error(e),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let ct = mime_guess::from_path(&key).first_or_octet_stream();

    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_str(ct.as_ref()).unwrap(),
    );
    resp.headers_mut().insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&meta.len().to_string()).unwrap(),
    );
    resp
}

/* ---------------- HEAD ---------------- */

pub async fn head_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="HEAD", key=%key, "request");

    let resp = head_object_impl(state.clone(), headers, key).await;

    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn head_object_impl(state: AppState, headers: HeaderMap, key: String) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }

    let key = match normalize_key(&key) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let path = match key_to_path(&state.storage_dir, &key) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let meta = match fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return not_found(),
        Err(e) => return server_error(e),
    };

    let mut resp = Response::new(Body::empty());
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&meta.len().to_string()).unwrap(),
    );
    resp
}

/* ---------------- DELETE ---------------- */

pub async fn delete_object(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="DELETE", key=%key, "request");

    let resp = delete_object_impl(state.clone(), headers, key).await;

    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn delete_object_impl(state: AppState, headers: HeaderMap, key: String) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }

    let key = match normalize_key(&key) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let path = match key_to_path(&state.storage_dir, &key) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let _ = fs::remove_file(&path).await;
    StatusCode::NO_CONTENT.into_response()
}
