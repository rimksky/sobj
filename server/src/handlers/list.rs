use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::Path as StdPath};
use walkdir::WalkDir;

use crate::{
    auth::{dec_in_flight, inc_in_flight, require_auth},
    key::path_to_key,
    state::AppState,
};

/* ---------------- LIST ---------------- */

#[derive(Deserialize)]
pub struct ListQuery {
    prefix: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Serialize)]
pub struct ListItem {
    pub key: String,
    pub size: u64,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
struct ListResp {
    prefix: String,
    items: Vec<ListItem>,
    next_cursor: Option<String>,
}

pub async fn list_objects(
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
    let limit = q.limit.unwrap_or(1000).min(10_000);
    let cursor = q.cursor.unwrap_or_default();

    let (items, next_cursor) =
        match list_impl(&state.storage_dir, &prefix, &cursor, limit).await {
            Ok(v) => v,
            Err(r) => return r,
        };

    (StatusCode::OK, Json(ListResp { prefix, items, next_cursor })).into_response()
}

pub async fn list_impl(
    root: &StdPath,
    prefix: &str,
    cursor: &str,
    limit: usize,
) -> Result<(Vec<ListItem>, Option<String>), Response> {
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

        // アップロード中の一時ファイルをスキップ
        if key.ends_with(".uploading.tmp") {
            continue;
        }

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
    for (key, size, lm) in keys {
        if items.len() >= limit {
            let next = items.last().map(|it| it.key.clone());
            return Ok((items, next));
        }
        items.push(ListItem { key, size, last_modified: lm });
    }

    Ok((items, None))
}

/* ---------------- テスト ---------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn basic() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b.txt"), b"world").unwrap();

        let (items, next) = list_impl(root, "", "", 100).await.unwrap();
        assert_eq!(items.len(), 2);
        assert!(next.is_none());
        assert_eq!(items[0].key, "a.txt");
        assert_eq!(items[1].key, "sub/b.txt");
    }

    #[tokio::test]
    async fn prefix_filter() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        std::fs::write(root.join("b.txt"), b"b").unwrap();

        let (items, _) = list_impl(root, "a", "", 100).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "a.txt");
    }

    #[tokio::test]
    async fn limit_and_cursor() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        for i in 0..5u8 {
            std::fs::write(root.join(format!("{}.txt", i)), b"x").unwrap();
        }

        let (items, next) = list_impl(root, "", "", 3).await.unwrap();
        assert_eq!(items.len(), 3);
        assert!(next.is_some());

        let cursor = next.unwrap();
        let (items2, next2) = list_impl(root, "", &cursor, 3).await.unwrap();
        assert_eq!(items2.len(), 2);
        assert!(next2.is_none());
    }

    #[tokio::test]
    async fn skips_uploading_tmp() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        std::fs::write(root.join("a.0.uploading.tmp"), b"tmp").unwrap();
        std::fs::write(root.join("a.42.uploading.tmp"), b"tmp").unwrap();

        let (items, _) = list_impl(root, "", "", 100).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "a.txt");
    }

    #[tokio::test]
    async fn size_reported() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(root.join("data.bin"), b"hello").unwrap();

        let (items, _) = list_impl(root, "", "", 100).await.unwrap();
        assert_eq!(items[0].size, 5);
    }
}
