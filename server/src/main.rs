mod auth;
mod error;
mod handlers;
mod key;
mod state;

use axum::{
    routing::{get, put},
    Router,
};
use std::{
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Arc,
    },
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use axum_server::tls_rustls::RustlsConfig;

use crate::{
    error::server_error,
    handlers::{
        health::health,
        list::list_objects,
        object::{delete_object, get_object, head_object, put_object},
    },
    state::{AppState, ServerConfig},
};

/* ---------------- 設定ファイル読み込み ---------------- */

fn default_config_path(file_name: &str) -> Result<PathBuf, axum::response::Response> {
    let exe = std::env::current_exe().map_err(server_error)?;
    let dir = exe.parent().ok_or_else(|| server_error("failed to get exe dir"))?;
    Ok(dir.join(file_name))
}

fn load_config_optional()
    -> Result<(Option<ServerConfig>, PathBuf), axum::response::Response>
{
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

    // --config で明示指定の場合、ファイルが存在しなければエラー
    // デフォルトパスの場合は省略可能（バイナリ隣に設定不要）
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

/* ---------------- エントリーポイント ---------------- */

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");

    tracing_subscriber::fmt()
        .with_env_filter("info,tower_http=info")
        .init();

    // ---- CLI TLS フラグ ----
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

    let cfg = cfg_opt.unwrap_or_default();

    let listen_addr = cfg.listen_addr.unwrap_or_else(|| "0.0.0.0:9999".into());
    let storage_dir_str = cfg.storage_dir.unwrap_or_else(|| "./data".into());

    // auth_token: 未指定または空文字 => 認証無効
    let auth_token = cfg.auth_token.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });

    // storage_dir: 相対パスは sobj-server.json があるディレクトリ基準で解決
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
        upload_counter: Arc::new(AtomicU64::new(0)),
    };

    let app = Router::new()
        .route("/health", get(health))
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

    // ---- TLS 有効化（CLI > JSON > デフォルト=false） ----
    let tls_enabled = if cli_tls {
        true
    } else if cli_no_tls {
        false
    } else {
        cfg.tls_enabled.unwrap_or(false)
    };

    if tls_enabled {
        let tls_cfg = cfg.tls.as_ref().unwrap_or_else(|| {
            eprintln!("config error: tls is enabled but tls section is missing");
            std::process::exit(2);
        });

        // 証明書パスを sobj-server.json のディレクトリ基準で解決
        let base = cfg_path.parent().unwrap_or_else(|| StdPath::new("."));
        let cert = {
            let p = PathBuf::from(&tls_cfg.cert_pem);
            if p.is_absolute() { p } else { base.join(p) }
        };
        let key = {
            let p = PathBuf::from(&tls_cfg.key_pem);
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
