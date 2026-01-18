use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
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
use tokio_util::io::ReaderStream;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use axum_server::tls_rustls::RustlsConfig;
use walkdir::WalkDir;

#[derive(Clone)]
struct AppState {
    storage_dir: Arc<PathBuf>,
    auth_token: Arc<Option<String>>, // None => auth disabled
    in_flight: Arc<AtomicUsize>,
}

#[derive(Deserialize, Default)]
struct ServerConfig {
    listen_addr: Option<String>,
    storage_dir: Option<String>,
    auth_token: Option<String>,
    // v0.3.2
    tls_enabled: Option<bool>, // optional, default = false
    tls: Option<TlsConfig>,
}

#[derive(Deserialize)]
struct TlsConfig {
    cert_pem: String,
    key_pem: String,
}

#[derive(Serialize)]
struct HealthzResponse {
    app: &'static str,
    version: &'static str,
    status: &'static str,
    in_flight: usize,
}


fn default_config_path(file_name: &str) -> Result<PathBuf, Response> {
    let exe = std::env::current_exe().map_err(server_error)?;
    let dir = exe.parent().ok_or_else(|| server_error("failed to get exe dir"))?;
    Ok(dir.join(file_name))
}

fn load_config_optional() -> Result<(Option<ServerConfig>, PathBuf), Response> {
    let mut args = std::env::args().skip(1);
    let mut config_path: Option<PathBuf> = None;
    let mut config_explicit = false;

    while let Some(a) = args.next() {
        if a == "--config" {
            if let Some(p) = args.next() {
                config_path = Some(PathBuf::from(p));
                config_explicit = true;
            }
        }
    }

    let path = match config_path {
        Some(p) => p,
        None => default_config_path("sobj-server.json")?,
    };

    // If --config is explicitly specified, missing file is an error.
    // Otherwise (default path alongside sobj-server binary), missing file is allowed
    // and we fall back to built-in defaults. In that case, relative paths are
    // resolved from the sobj-server binary directory via this pseudo config path.
    if !path.exists() {
        if config_explicit {
            let msg = format!("failed to read config: file not found (path: {:?})", path);
            return Err(server_error(msg));
        }
        return Ok((None, path));
    }

    let txt = std::fs::read_to_string(&path).map_err(|e| {
        let msg = format!("failed to read config: {} (expected at: {:?})", e, path);
        server_error(msg)
    })?;
    let cfg: ServerConfig =
        serde_json::from_str(&txt).map_err(|e| server_error(format!("invalid json: {}", e)))?;
    Ok((Some(cfg), path))
}

#[tokio::main]
async fn main() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");

    tracing_subscriber::fmt()
        .with_env_filter("info,tower_http=info")
        .init();

    // ---- CLI TLS flags (v0.3.2) ----
    let mut cli_tls = false;
    let mut cli_no_tls = false;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--tls" => cli_tls = true,
            "--no-tls" => cli_no_tls = true,
            _ => {}
        }
    }
    if cli_tls && cli_no_tls {
        eprintln!("args error: both --tls and --no-tls are set");
        std::process::exit(2);
    }

    let (cfg_opt, cfg_path) = match load_config_optional() {
        Ok(v) => v,
        Err(r) => {
            eprintln!("config error: {:?}", r);
            std::process::exit(2);
        }
    };

    // Built-in defaults (config file is optional)
    let cfg = cfg_opt.unwrap_or_default();

    // v0.4 defaults
    let listen_addr = cfg.listen_addr.unwrap_or_else(|| "0.0.0.0:9999".into());
    let storage_dir_str = cfg.storage_dir.unwrap_or_else(|| "./data".into());

    // auth_token: omitted or empty => auth disabled
    let auth_token = cfg.auth_token.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });

    // storage_dir: relative => base is directory where sobj-server.json exists.
    // When config file is missing, cfg_path points to the default config path
    // alongside the sobj-server binary, so relative paths resolve from exe dir.
    let storage_dir = {
        let p = PathBuf::from(storage_dir_str);
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
        .route("/healthz", get(healthz))
        .route("/", get(list_objects))
        .route("/_copy", post(copy_object))
        .route("/_move", post(move_object))
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

    // ---- TLS enablement (CLI > JSON > default=false) ----
    let tls_enabled = if cli_tls {
        true
    } else if cli_no_tls {
        false
    } else {
        cfg.tls_enabled.unwrap_or(false)
    };

    if tls_enabled {
        let tls = cfg.tls.as_ref().unwrap_or_else(|| {
            eprintln!("config error: tls is enabled but tls section is missing");
            std::process::exit(2);
        });

        // resolve relative paths from directory where sobj-server.json exists
        let base = cfg_path.parent().unwrap_or_else(|| StdPath::new("."));
        let cert = {
            let p = PathBuf::from(&tls.cert_pem);
            if p.is_absolute() { p } else { base.join(p) }
        };
        let key = {
            let p = PathBuf::from(&tls.key_pem);
            if p.is_absolute() { p } else { base.join(p) }
        };

        tracing::info!("sobj-server listening on https://{}", addr);

        let tls = RustlsConfig::from_pem_file(cert, key)
            .await
            .unwrap_or_else(|e| {
                eprintln!("tls config error: {:?}", e);
                std::process::exit(2);
            });

        axum_server::bind_rustls(addr, tls)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    } else {
        tracing::info!("sobj-server listening on http://{}", addr);
        axum_server::bind(addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    }
}

/* ---------------- Auth ---------------- */

fn require_auth(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    let Some(expected) = state.auth_token.as_ref() else {
        return Ok(()); // auth disabled
    };

    let v = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if v == Some(expected.as_str()) {
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

/* ---------------- Health ---------------- */

async fn healthz(State(state): State<AppState>) -> Json<HealthzResponse> {
    Json(HealthzResponse {
        app: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        status: "ok",
        in_flight: state.in_flight.load(Ordering::Relaxed),
    })
}


/* ---------------- PUT ---------------- */

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

/* ---------------- GET (streaming) ---------------- */

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

    let meta = match fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"NotFound"}))).into_response();
        }
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

/* ---------------- DELETE ---------------- */

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

/* ---------------- LIST ---------------- */

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

    (StatusCode::OK, Json(ListResp { prefix, delimiter, items, common_prefixes, next_cursor })).into_response()
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

        items.push(ListItem { key, size, last_modified: lm });
    }

    Ok((items, common.into_iter().collect(), None))
}

/* ---------------- COPY / MOVE ---------------- */

#[derive(Deserialize)]
struct CopyMoveReq {
    src: String,
    dst: String,
    overwrite: Option<bool>,
}

#[derive(Serialize)]
struct CopyMoveResp {
    src: String,
    dst: String,
    size: u64,
}

async fn copy_object(
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"NotFound"}))).into_response();
        }
        Err(e) => return server_error(e),
    };

    if !overwrite {
        if fs::metadata(&dst_path).await.is_ok() {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error":"Conflict","message":"destination exists"}))).into_response();
        }
    }

    if let Some(parent) = dst_path.parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            return server_error(e);
        }
    }

    match fs::copy(&src_path, &dst_path).await {
        Ok(_) => (StatusCode::OK, Json(CopyMoveResp{src: src_key, dst: dst_key, size: src_meta.len()})).into_response(),
        Err(e) => server_error(e),
    }
}

async fn move_object(
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"NotFound"}))).into_response();
        }
        Err(e) => return server_error(e),
    };

    if !overwrite {
        if fs::metadata(&dst_path).await.is_ok() {
            return (StatusCode::CONFLICT, Json(serde_json::json!({"error":"Conflict","message":"destination exists"}))).into_response();
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
        Ok(_) => (StatusCode::OK, Json(CopyMoveResp{src: src_key, dst: dst_key, size: src_meta.len()})).into_response(),
        Err(_) => match fs::copy(&src_path, &dst_path).await {
            Ok(_) => {
                let _ = fs::remove_file(&src_path).await;
                (StatusCode::OK, Json(CopyMoveResp{src: src_key, dst: dst_key, size: src_meta.len()})).into_response()
            }
            Err(e) => server_error(e),
        },
    }
}

/* ---------------- Key safety ---------------- */

fn normalize_key(raw: &str) -> Result<String, Response> {
    let decoded = percent_decode_str(raw).decode_utf8_lossy().to_string();

    if decoded.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"InvalidKey"}))).into_response());
    }
    if decoded.starts_with('/') || decoded.contains("..") {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"InvalidKey"}))).into_response());
    }
    Ok(decoded)
}

fn key_to_path(root: &StdPath, key: &str) -> Result<PathBuf, Response> {
    let p = root.join(key.replace('\\', "/"));
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"InvalidKey"}))).into_response());
    }
    Ok(p)
}

fn path_to_key(rel: &StdPath) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn server_error<E: std::fmt::Display>(e: E) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"InternalError","message":e.to_string()}))).into_response()
}
