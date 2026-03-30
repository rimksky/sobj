use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Arc,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub storage_dir: Arc<PathBuf>,
    pub auth_token: Arc<Option<String>>, // None => 認証無効
    pub in_flight: Arc<AtomicUsize>,
    pub upload_counter: Arc<AtomicU64>, // PUT 並行書き込み用ユニーク ID
}

#[derive(Deserialize, Default)]
pub struct ServerConfig {
    pub listen_addr: Option<String>,
    pub storage_dir: Option<String>,
    pub auth_token: Option<String>,
    pub tls_enabled: Option<bool>,
    pub tls: Option<TlsConfig>,
}

#[derive(Deserialize)]
pub struct TlsConfig {
    pub cert_pem: String,
    pub key_pem: String,
}

#[derive(Serialize)]
pub struct HealthzResponse {
    pub app: &'static str,
    pub version: &'static str,
    pub status: &'static str,
    pub in_flight: usize,
}
