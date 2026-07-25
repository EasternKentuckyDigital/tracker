use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};

use crate::{
    db::Database,
    model::{MergeSummary, SyncRequest, SyncResponse, validate_sync_payload},
    tailscale::TailscalePeer,
};

const MAX_SYNC_BYTES: usize = 16 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 64 * 1024;
const MAX_HEALTH_BYTES: usize = 128;
const MAX_CONCURRENT_SYNCS: usize = 2;
const MAX_TOKEN_BYTES: usize = 4 * 1024;
const TRACKER_HEALTH_RESPONSE: &str = "tracker-sync-v1";

#[derive(Clone)]
struct ServerState {
    database_path: PathBuf,
    token: Option<Arc<str>>,
    sync_slots: Arc<Semaphore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachablePeer {
    pub name: String,
    pub url: String,
}

pub async fn serve(database_path: PathBuf, bind: SocketAddr, token: Option<String>) -> Result<()> {
    validate_token(token.as_deref())?;
    let state = ServerState {
        database_path,
        token: token.map(Into::into),
        sync_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_SYNCS)),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/sync", post(sync_handler))
        .layer(DefaultBodyLimit::max(MAX_SYNC_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("could not listen on {bind}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("sync server failed")
}

pub async fn sync_with_peer(
    database: &mut Database,
    peer: &str,
    token: Option<&str>,
) -> Result<MergeSummary> {
    validate_token(token)?;
    let endpoint = sync_endpoint(peer)?;
    let request = SyncRequest {
        device_id: database.device_id()?,
        tasks: database.all_tasks()?,
        entries: database.all_entries()?,
    };
    validate_sync_payload(&request.device_id, &request.tasks, &request.entries)
        .context("local database contains records that are unsafe to sync")?;
    let request_body =
        serde_json::to_vec(&request).context("could not serialize local sync records")?;
    if request_body.len() > MAX_SYNC_BYTES {
        bail!("local dataset is too large for the sync protocol limit");
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let request_builder = client
        .post(endpoint.clone())
        .header(CONTENT_TYPE, "application/json")
        .body(request_body);
    let response = with_auth(request_builder, token)
        .send()
        .await
        .with_context(|| format!("could not connect to {endpoint}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = read_limited_response(response, MAX_ERROR_BYTES)
            .await
            .ok()
            .and_then(|body| String::from_utf8(body).ok())
            .unwrap_or_default();
        bail!("peer rejected sync ({status}): {}", body.trim());
    }
    let body = read_limited_response(response, MAX_SYNC_BYTES)
        .await
        .context("peer returned an oversized sync response")?;
    let response: SyncResponse =
        serde_json::from_slice(&body).context("peer returned an invalid sync response")?;
    validate_sync_payload(&response.device_id, &response.tasks, &response.entries)
        .context("peer returned unsafe sync records")?;
    database.merge(&response.tasks, &response.entries)
}

pub async fn discover_tracker_peers(
    candidates: Vec<TailscalePeer>,
    port: u16,
    token: Option<&str>,
) -> Result<Vec<ReachablePeer>> {
    validate_token(token)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(750))
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let token = token.map(str::to_owned);
    let mut probes = JoinSet::new();

    for candidate in candidates {
        let client = client.clone();
        let token = token.clone();
        probes.spawn(async move {
            let url = format!("http://{}:{port}", candidate.ip);
            let request = client.get(format!("{url}/health"));
            let response = with_auth(request, token.as_deref()).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let body = read_limited_response(response, MAX_HEALTH_BYTES)
                .await
                .ok()?;
            (body.as_slice() == TRACKER_HEALTH_RESPONSE.as_bytes()).then_some(ReachablePeer {
                name: candidate.name,
                url,
            })
        });
    }

    let mut peers = Vec::new();
    while let Some(result) = probes.join_next().await {
        if let Ok(Some(peer)) = result {
            peers.push(peer);
        }
    }
    peers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(peers)
}

async fn health() -> &'static str {
    TRACKER_HEALTH_RESPONSE
}

async fn sync_handler(
    State(state): State<ServerState>,
    Json(request): Json<SyncRequest>,
) -> Result<Response, (StatusCode, String)> {
    let _permit = state.sync_slots.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "another sync is already in progress; retry shortly".to_owned(),
        )
    })?;
    validate_sync_payload(&request.device_id, &request.tasks, &request.entries).map_err(
        |error| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid sync payload: {error}"),
            )
        },
    )?;
    let mut database = Database::open(&state.database_path).map_err(internal_error)?;
    database
        .merge(&request.tasks, &request.entries)
        .map_err(internal_error)?;
    let response = SyncResponse {
        device_id: database.device_id().map_err(internal_error)?,
        tasks: database.all_tasks().map_err(internal_error)?,
        entries: database.all_entries().map_err(internal_error)?,
    };
    validate_sync_payload(&response.device_id, &response.tasks, &response.entries)
        .map_err(internal_error)?;
    let body = serde_json::to_vec(&response).map_err(|error| internal_error(error.into()))?;
    if body.len() > MAX_SYNC_BYTES {
        return Err((
            StatusCode::INSUFFICIENT_STORAGE,
            "local dataset is too large for the sync protocol limit".to_owned(),
        ));
    }
    Ok(([(CONTENT_TYPE, "application/json")], body).into_response())
}

async fn authenticate(
    State(state): State<ServerState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(token) = &state.token else {
        return Ok(next.run(request).await);
    };
    let expected = format!("Bearer {token}");
    let supplied = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let matches = supplied.len() == expected.len()
        && bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()));
    if !matches {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

fn validate_token(token: Option<&str>) -> Result<()> {
    if token.is_some_and(|token| token.len() < 32) {
        bail!("TRACKER_SYNC_TOKEN must contain at least 32 bytes");
    }
    if token.is_some_and(|token| token.len() > MAX_TOKEN_BYTES) {
        bail!("TRACKER_SYNC_TOKEN must not exceed {MAX_TOKEN_BYTES} bytes");
    }
    if token.is_some_and(|token| token.chars().any(char::is_control)) {
        bail!("TRACKER_SYNC_TOKEN must not contain control characters");
    }
    Ok(())
}

fn with_auth(request: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    }
}

fn sync_endpoint(peer: &str) -> Result<reqwest::Url> {
    let mut endpoint = reqwest::Url::parse(peer).context("peer must be an absolute URL")?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        bail!("peer URL must use http or https");
    }
    if endpoint.host_str().is_none() {
        bail!("peer URL must include a host");
    }
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        bail!("peer URL must not contain credentials");
    }
    if endpoint.query().is_some() || endpoint.fragment().is_some() {
        bail!("peer URL must not contain a query or fragment");
    }
    if !matches!(endpoint.path(), "" | "/") {
        bail!("peer URL must contain only an origin, without a path");
    }
    endpoint.set_path("/v1/sync");
    Ok(endpoint)
}

async fn read_limited_response(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|content_length| content_length > limit as u64)
    {
        bail!("response exceeds the {limit}-byte limit");
    }

    let mut body = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0)
            .min(limit),
    );
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > limit {
            bail!("response exceeds the {limit}-byte limit");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    eprintln!("sync request failed: {error:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal sync error".to_owned(),
    )
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_a_versioned_sync_endpoint() {
        assert_eq!(
            sync_endpoint("https://peer.example:7789").unwrap().as_str(),
            "https://peer.example:7789/v1/sync"
        );
    }

    #[test]
    fn rejects_credentialed_or_ambiguous_peer_urls() {
        assert!(sync_endpoint("file:///tmp/tracker").is_err());
        assert!(sync_endpoint("http://token@peer.example").is_err());
        assert!(sync_endpoint("http://peer.example/redirect").is_err());
        assert!(sync_endpoint("http://peer.example?next=internal").is_err());
    }

    #[test]
    fn rejects_unsafe_tokens() {
        assert!(validate_token(Some("short")).is_err());
        assert!(validate_token(Some(&"x".repeat(MAX_TOKEN_BYTES + 1))).is_err());
        assert!(validate_token(Some(&format!("{}\n", "x".repeat(32)))).is_err());
        assert!(validate_token(Some(&"x".repeat(32))).is_ok());
    }
}
