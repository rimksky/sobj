use axum::response::Response;
use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};

use crate::error::invalid_key;

pub fn normalize_key(raw: &str) -> Result<String, Response> {
    let decoded = percent_decode_str(raw).decode_utf8_lossy().to_string();

    if decoded.is_empty() {
        return Err(invalid_key());
    }
    if decoded.starts_with('/') || decoded.contains("..") {
        return Err(invalid_key());
    }
    Ok(decoded)
}

pub fn key_to_path(root: &Path, key: &str) -> Result<PathBuf, Response> {
    let p = root.join(key);
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(invalid_key());
    }
    Ok(p)
}

pub fn path_to_key(rel: &Path) -> String {
    rel.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_valid() {
        assert!(normalize_key("foo").is_ok());
        assert!(normalize_key("foo/bar.txt").is_ok());
        assert!(normalize_key("a/b/c.json").is_ok());
    }

    #[test]
    fn normalize_key_url_encoded() {
        let k = normalize_key("hello%20world").unwrap();
        assert_eq!(k, "hello world");
    }

    #[test]
    fn normalize_key_empty() {
        assert!(normalize_key("").is_err());
    }

    #[test]
    fn normalize_key_leading_slash() {
        assert!(normalize_key("/foo").is_err());
        assert!(normalize_key("/").is_err());
    }

    #[test]
    fn normalize_key_dotdot() {
        assert!(normalize_key("../etc/passwd").is_err());
        assert!(normalize_key("foo/../bar").is_err());
        assert!(normalize_key("foo/..").is_err());
        assert!(normalize_key("..").is_err());
    }

    #[test]
    fn key_to_path_safe() {
        let root = Path::new("/storage");
        let p = key_to_path(root, "foo/bar.txt").unwrap();
        assert_eq!(p, PathBuf::from("/storage/foo/bar.txt"));
    }

    #[test]
    fn key_to_path_traversal_blocked() {
        let root = Path::new("/storage");
        assert!(key_to_path(root, "foo/../../../etc/passwd").is_err());
    }
}
