//! HTTP route handlers for session CRUD operations.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use antikythera_domain::session::Session;

use crate::StorageEngine;

/// Build the session API router with all endpoints.
pub fn api_routes(engine: Arc<Mutex<StorageEngine>>) -> Router {
    Router::new()
        .route("/sessions", axum::routing::get(list_sessions))
        .route("/sessions/{id}", axum::routing::get(get_session))
        .route("/sessions/{id}", axum::routing::post(save_session))
        .route("/sessions/{id}", axum::routing::delete(delete_session))
        .route("/health", axum::routing::get(health_check))
        .with_state(engine)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn get_session(
    State(engine): State<Arc<Mutex<StorageEngine>>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    let mut engine = engine.lock().await;
    match engine.load(&id).await {
        Ok(Some(data)) => {
            let session: Session =
                serde_json::from_slice(&data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(session))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn save_session(
    State(engine): State<Arc<Mutex<StorageEngine>>>,
    Path(id): Path<String>,
    Json(session): Json<Session>,
) -> Result<StatusCode, StatusCode> {
    let data = serde_json::to_vec(&session).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut engine = engine.lock().await;
    engine
        .save(&id, data)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn delete_session(
    State(engine): State<Arc<Mutex<StorageEngine>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut engine = engine.lock().await;
    engine
        .delete(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_sessions(
    State(engine): State<Arc<Mutex<StorageEngine>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let engine = engine.lock().await;
    match engine.list().await {
        Ok(ids) => Ok(Json(serde_json::json!({ "sessions": ids }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
