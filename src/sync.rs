use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

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
use tokio::net::TcpListener;

use crate::{
    db::Database,
    model::{MergeSummary, SyncRequest, SyncResponse},
};

const MAX_SYNC_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct ServerState {
    database_path: PathBuf,
    token: Arc<str>,
}

pub async fn serve(database_path: PathBuf, bind: SocketAddr, token: String) -> Result<()> {
    validate_token(&token)?;
    let state = ServerState {
        database_path,
        token: token.into(),
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
    token: &str,
) -> Result<MergeSummary> {
    validate_token(token)?;
    let endpoint = format!("{}/v1/sync", peer.trim_end_matches('/'));
    let request = SyncRequest {
        device_id: database.device_id()?,
        tasks: database.all_tasks()?,
        entries: database.all_entries()?,
    };
    let response = reqwest::Client::new()
        .post(&endpoint)
        .bearer_auth(token)
        .json(&request)
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

async fn health() -> &'static str {
    "ok"
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
    let expected = format!("Bearer {}", state.token);
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

fn validate_token(token: &str) -> Result<()> {
    if token.len() < 32 {
        bail!("TRACKER_SYNC_TOKEN must contain at least 32 bytes");
    }
    Ok(())
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

pub fn database_exists(path: &Path) -> bool {
    path.is_file()
}
