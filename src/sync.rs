use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, task::JoinSet};

use crate::{
    db::Database,
    model::{MergeSummary, SyncRequest, SyncResponse},
    tailscale::TailscalePeer,
};

const MAX_SYNC_BYTES: usize = 16 * 1024 * 1024;
const TRACKER_HEALTH_RESPONSE: &str = "tracker-sync-v1";

#[derive(Clone)]
struct ServerState {
    database_path: PathBuf,
    token: Option<Arc<str>>,
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
    let endpoint = format!("{}/v1/sync", peer.trim_end_matches('/'));
    let request = SyncRequest {
        device_id: database.device_id()?,
        tasks: database.all_tasks()?,
        entries: database.all_entries()?,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let request_builder = client.post(&endpoint).json(&request);
    let response = with_auth(request_builder, token)
        .send()
        .await
        .with_context(|| format!("could not connect to {endpoint}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("peer rejected sync ({status}): {}", body.trim());
    }
    let response: SyncResponse = response
        .json()
        .await
        .context("peer returned an invalid sync response")?;
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
            let body = response.text().await.ok()?;
            (body.trim() == TRACKER_HEALTH_RESPONSE).then_some(ReachablePeer {
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
) -> Result<Json<SyncResponse>, (StatusCode, String)> {
    let mut database = Database::open(&state.database_path).map_err(internal_error)?;
    database
        .merge(&request.tasks, &request.entries)
        .map_err(internal_error)?;
    let response = SyncResponse {
        device_id: database.device_id().map_err(internal_error)?,
        tasks: database.all_tasks().map_err(internal_error)?,
        entries: database.all_entries().map_err(internal_error)?,
    };
    Ok(Json(response))
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
    Ok(())
}

fn with_auth(request: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    }
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
