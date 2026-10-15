use std::io::{Read, Seek, SeekFrom};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    let dir = state.config.lock().output_dir.clone();
    let mut items = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("mp4") {
                continue;
            }
            let meta = e.metadata().ok();
            items.push(json!({
                "name": path.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
                "path": path.to_string_lossy(),
                "bytes": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                "modified": meta.and_then(|m| m.modified().ok()).and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
                }),
            }));
        }
    }
    items.sort_by(|a, b| {
        let am = a.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
        let bm = b.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
        bm.cmp(&am)
    });
    Json(json!({ "outputs": items }))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let safe = sanitize_name(&name)?;
    let path = state.config.lock().output_dir.join(&safe);
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "output not found".into()));
    }
    std::fs::remove_file(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let safe = sanitize_name(&name)?;
    let path = state.config.lock().output_dir.join(&safe);
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "output not found".into()));
    }
    let meta = std::fs::metadata(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total = meta.len();
    if total == 0 {
        return Err((StatusCode::NOT_FOUND, "empty output".into()));
    }

    let disposition = HeaderValue::from_str(&format!("inline; filename=\"{safe}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("inline"));

    if let Some(range_val) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        let Some((start, end)) = parse_bytes_range(range_val, total) else {
            let mut res = Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                .body(Body::empty())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            res.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
            return Ok(res);
        };
        let len = end - start + 1;
        let mut file = std::fs::File::open(&path)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        file.seek(SeekFrom::Start(start))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut res = Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, "video/mp4")
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, len)
            .header(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{total}"),
            )
            .header(header::CONTENT_DISPOSITION, disposition)
            .body(Body::from(buf))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // Ensure CORS-friendly caching does not hide new outputs.
        res.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, max-age=0, must-revalidate"),
        );
        return Ok(res);
    }

    let bytes =
        std::fs::read(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut res = bytes.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("video/mp4"),
    );
    res.headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    res.headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=0, must-revalidate"),
    );
    Ok(res)
}

/// Parse a single `bytes=` range. Returns inclusive `(start, end)`.
fn parse_bytes_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.trim();
    // Browsers usually send one range; take the first.
    let spec = spec.split(',').next()?.trim();
    if spec.is_empty() || total == 0 {
        return None;
    }
    let (start_s, end_s) = spec.split_once('-')?;
    if start_s.is_empty() {
        // suffix: bytes=-N
        let n: u64 = end_s.parse().ok()?;
        if n == 0 {
            return None;
        }
        let n = n.min(total);
        return Some((total - n, total - 1));
    }
    let start: u64 = start_s.parse().ok()?;
    if start >= total {
        return None;
    }
    let end = if end_s.is_empty() {
        total - 1
    } else {
        end_s.parse::<u64>().ok()?.min(total - 1)
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

fn sanitize_name(name: &str) -> Result<String, (StatusCode, String)> {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err((StatusCode::BAD_REQUEST, "invalid name".into()));
    }
    if !name.ends_with(".mp4") {
        return Err((StatusCode::BAD_REQUEST, "only .mp4 supported".into()));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_bytes_range;

    #[test]
    fn range_open_end() {
        assert_eq!(parse_bytes_range("bytes=0-", 1000), Some((0, 999)));
    }

    #[test]
    fn range_closed() {
        assert_eq!(parse_bytes_range("bytes=0-1", 1000), Some((0, 1)));
    }

    #[test]
    fn range_suffix() {
        assert_eq!(parse_bytes_range("bytes=-100", 1000), Some((900, 999)));
    }

    #[test]
    fn range_unsatisfiable() {
        assert_eq!(parse_bytes_range("bytes=1000-", 1000), None);
    }
}
