use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{fs, io::AsyncWriteExt};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use walkdir::WalkDir;

#[derive(Clone)]
struct AppState {
    storage_dir: Arc<PathBuf>,
    auth_token: Arc<String>,
    in_flight: Arc<AtomicUsize>,
}

#[derive(Deserialize)]
struct ServerConfig {
    listen_addr: Option<String>,
    storage_dir: Option<String>,
    auth_token: Option<String>,
}

fn default_config_path(file_name: &str) -> Result<PathBuf, Response> {
    let exe = std::env::current_exe().map_err(server_error)?;
    let dir = exe.parent().ok_or_else(|| server_error("failed to get exe dir"))?;
    Ok(dir.join(file_name))
}

fn load_config() -> Result<(ServerConfig, PathBuf), Response> {
    let mut args = std::env::args().skip(1);
    let mut config_path: Option<PathBuf> = None;

    while let Some(a) = args.next() {
        if a == "--config" {
            if let Some(p) = args.next() {
                config_path = Some(PathBuf::from(p));
            }
        }
    }

    let path = match config_path {
        Some(p) => p,
        None => default_config_path("sobj-server.json")?,
    };

    let txt = std::fs::read_to_string(&path).map_err(|e| {
        let msg = format!("failed to read config: {} (expected at: {:?})", e, path);
        server_error(msg)
    })?;
    let cfg: ServerConfig =
        serde_json::from_str(&txt).map_err(|e| server_error(format!("invalid json: {}", e)))?;
    Ok((cfg, path))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,tower_http=info")
        .init();

    let (cfg, cfg_path) = match load_config() {
        Ok(v) => v,
        Err(r) => {
            eprintln!("config error: {:?}", r);
            std::process::exit(2);
        }
    };

    let listen_addr = cfg.listen_addr.unwrap_or_else(|| "0.0.0.0:8080".into());
    let auth_token = cfg.auth_token.unwrap_or_else(|| "Bearer devtoken".into());
    let storage_dir = cfg.storage_dir.unwrap_or_else(|| "./data".into());

    let storage_dir = {
        let p = PathBuf::from(storage_dir);
        if p.is_absolute() {
            p
        } else {
            cfg_path.parent().unwrap_or_else(|| StdPath::new(".")).join(p)
        }
    };

    let state = AppState {
        storage_dir: Arc::new(storage_dir),
        auth_token: Arc::new(auth_token),
        in_flight: Arc::new(AtomicUsize::new(0)),
    };

    let app = Router::new()
        .route("/", get(list_objects))
        .route(
            "/*key",
            put(put_object)
                .get(get_object)
                .delete(delete_object)
                .head(head_object),
        )
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state);

    let addr: SocketAddr = listen_addr.parse().expect("invalid listen_addr");
    tracing::info!("sobj-server listening on http://{}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

fn require_auth(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    let v = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if v == Some(state.auth_token.as_str()) {
        Ok(())
    } else {
        let body = Json(serde_json::json!({ "error": "Unauthorized" }));
        Err((StatusCode::UNAUTHORIZED, body).into_response())
    }
}

fn inc_in_flight(state: &AppState) -> usize {
    state.in_flight.fetch_add(1, Ordering::SeqCst) + 1
}
fn dec_in_flight(state: &AppState) -> usize {
    state.in_flight
        .fetch_sub(1, Ordering::SeqCst)
        .saturating_sub(1)
}

/* PUT */

#[derive(Serialize)]
struct PutResp {
    key: String,
    size: u64,
}

async fn put_object(
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

    let tmp_path = path.with_extension("uploading.tmp");
    let mut file = match fs::File::create(&tmp_path).await {
        Ok(f) => f,
        Err(e) => return server_error(e),
    };

    let mut stream = body.into_data_stream();
    let mut size: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return server_error(e),
        };
        size += chunk.len() as u64;
        if let Err(e) = file.write_all(&chunk).await {
            return server_error(e);
        }
    }
    if let Err(e) = file.flush().await {
        return server_error(e);
    }
    if let Err(e) = fs::rename(&tmp_path, &path).await {
        return server_error(e);
    }

    (StatusCode::CREATED, Json(PutResp { key, size })).into_response()
}

/* GET */

async fn get_object(
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

    let data = match fs::read(&path).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"NotFound"})),
            )
                .into_response();
        }
        Err(e) => return server_error(e),
    };

    let ct = mime_guess::from_path(&key).first_or_octet_stream();

    let mut resp = Response::new(Body::from(data));
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_str(ct.as_ref()).unwrap(),
    );
    resp
}

/* HEAD */

async fn head_object(
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response()
        }
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

/* DELETE */

async fn delete_object(
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

/* LIST */

#[derive(Deserialize)]
struct ListQuery {
    prefix: Option<String>,
    delimiter: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Serialize)]
struct ListItem {
    key: String,
    size: u64,
    last_modified: Option<String>,
}

#[derive(Serialize)]
struct ListResp {
    prefix: String,
    delimiter: Option<String>,
    items: Vec<ListItem>,
    common_prefixes: Vec<String>,
    next_cursor: Option<String>,
}

async fn list_objects(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    let in_flight = inc_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, method="LIST", "request");

    let resp = list_objects_impl(state.clone(), headers, q).await;

    let in_flight = dec_in_flight(&state);
    tracing::info!(peer=%peer, in_flight=%in_flight, status=%resp.status(), "done");
    resp
}

async fn list_objects_impl(state: AppState, headers: HeaderMap, q: ListQuery) -> Response {
    if let Err(r) = require_auth(&headers, &state) {
        return r;
    }

    let prefix = q.prefix.unwrap_or_default();
    let delimiter = q.delimiter.filter(|d| !d.is_empty());
    let limit = q.limit.unwrap_or(1000).min(10_000);
    let cursor = q.cursor.unwrap_or_default();

    let (items, common_prefixes, next_cursor) =
        match list_impl(&state.storage_dir, &prefix, delimiter.as_deref(), &cursor, limit).await {
            Ok(v) => v,
            Err(r) => return r,
        };

    (
        StatusCode::OK,
        Json(ListResp {
            prefix,
            delimiter,
            items,
            common_prefixes,
            next_cursor,
        }),
    )
        .into_response()
}

async fn list_impl(
    root: &StdPath,
    prefix: &str,
    delimiter: Option<&str>,
    cursor: &str,
    limit: usize,
) -> Result<(Vec<ListItem>, Vec<String>, Option<String>), Response> {
    let mut keys: Vec<(String, u64, Option<String>)> = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = match entry.path().strip_prefix(root) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let key = path_to_key(rel);

        if !prefix.is_empty() && !key.starts_with(prefix) {
            continue;
        }
        if !cursor.is_empty() && key.as_str() <= cursor {
            continue;
        }

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        let lm = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let dt: time::OffsetDateTime = t.into();
                dt.format(&time::format_description::well_known::Rfc3339).ok()
            });

        keys.push((key, size, lm));
    }

    keys.sort_by(|a, b| a.0.cmp(&b.0));

    let mut items: Vec<ListItem> = Vec::new();
    let mut common: BTreeSet<String> = BTreeSet::new();

    for (key, size, lm) in keys.into_iter() {
        if items.len() >= limit {
            let next = items.last().map(|it| it.key.clone());
            return Ok((items, common.into_iter().collect(), next));
        }

        if let Some(d) = delimiter {
            let rest = &key[prefix.len()..];
            if let Some(pos) = rest.find(d) {
                let cp = format!("{}{}", prefix, &rest[..pos + d.len()]);
                common.insert(cp);
                continue;
            }
        }

        items.push(ListItem {
            key,
            size,
            last_modified: lm,
        });
    }

    Ok((items, common.into_iter().collect(), None))
}

fn normalize_key(raw: &str) -> Result<String, Response> {
    let decoded = percent_decode_str(raw).decode_utf8_lossy().to_string();

    if decoded.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"InvalidKey"})),
        )
            .into_response());
    }
    if decoded.starts_with('/') || decoded.contains("..") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"InvalidKey"})),
        )
            .into_response());
    }
    Ok(decoded)
}

fn key_to_path(root: &StdPath, key: &str) -> Result<PathBuf, Response> {
    let p = root.join(key.replace('\\', "/"));
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"InvalidKey"})),
        )
            .into_response());
    }
    Ok(p)
}

fn path_to_key(rel: &StdPath) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn server_error<E: std::fmt::Display>(e: E) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error":"InternalError","message":e.to_string()})),
    )
        .into_response()
}
