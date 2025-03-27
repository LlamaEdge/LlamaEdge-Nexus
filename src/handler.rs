use crate::{
    error::{self, ServerError, ServerResult},
    info::{ApiServer, ModelConfig},
    server::{RoutingPolicy, Server, ServerIdToRemove, ServerKind},
    AppState, SharedClient, UrlType,
};
use axum::{
    body::Body,
    extract::{Json, Path, State},
    http::{HeaderMap, Method, Request, Response, StatusCode, Uri},
    response::IntoResponse,
};
use endpoints::{
    chat::ChatCompletionRequest,
    embeddings::{EmbeddingRequest, EmbeddingsResponse},
    models::ListModelsResponse,
};
use reqwest::Client;
use std::sync::Arc;

pub(crate) async fn embeddings_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> ServerResult<Response<Body>> {
    // Get request ID from headers
    let request_id = headers
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    info!(target: "stdout", "Received a new embeddings request - request_id: {}", request_id);

    // get the embeddings server
    let servers = state.server_group.read().await;
    let embeddings_servers = match servers.get(&ServerKind::embeddings) {
        Some(servers) => servers,
        None => {
            let err_msg = "No embeddings server available";
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            return Err(ServerError::Operation(err_msg.to_string()));
        }
    };

    let embeddings_server_base_url = match embeddings_servers.next().await {
        Ok(url) => url,
        Err(e) => {
            let err_msg = format!("Failed to get the embeddings server: {}", e);
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            return Err(ServerError::Operation(err_msg));
        }
    };
    let embeddings_service_url = format!("{}v1/embeddings", embeddings_server_base_url);
    info!(target: "stdout", "Forward the embeddings request to {} - request_id: {}", embeddings_service_url, request_id);

    // parse the content-type header
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            let err_msg = "Missing Content-Type header".to_string();
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            ServerError::Operation(err_msg)
        })?;
    let content_type = content_type.to_string();
    info!(target: "stdout", "Request content type: {} - request_id: {}", content_type, request_id);

    // Create request client
    let response = reqwest::Client::new()
        .post(embeddings_service_url)
        .header("Content-Type", content_type)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            let err_msg = format!(
                "Failed to forward the request to the downstream server: {}",
                e
            );
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            ServerError::Operation(err_msg)
        })?;

    let status = response.status();

    // Handle response body reading with cancellation
    let bytes = response.bytes().await.map_err(|e| {
        let err_msg = format!("Failed to get the full response as bytes: {}", e);
        error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
        ServerError::Operation(err_msg)
    })?;

    match Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(bytes))
    {
        Ok(response) => {
            info!(target: "stdout", "Embeddings request completed successfully - request_id: {}", request_id);
            Ok(response)
        }
        Err(e) => {
            let err_msg = format!("Failed to create the response: {}", e);
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            Err(ServerError::Operation(err_msg))
        }
    }
}

pub mod admin {
    use super::*;

    pub async fn register_downstream_server_handler(
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
        Json(mut server): Json<Server>,
    ) -> ServerResult<Response<Body>> {
        // Get request ID from headers
        let request_id = headers
            .get("x-request-id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let server_url = server.url.clone();
        let server_kind = server.kind;
        let server_id = server.id.clone();

        // verify the server
        if server_kind.contains(ServerKind::chat)
            || server_kind.contains(ServerKind::embeddings)
            || server_kind.contains(ServerKind::image)
            || server_kind.contains(ServerKind::transcribe)
            || server_kind.contains(ServerKind::translate)
            || server_kind.contains(ServerKind::tts)
        {
            verify_server(
                State(state.clone()),
                &request_id,
                &server_id,
                &server_url,
                &server_kind,
            )
            .await?;
        }

        // register the server
        state.register_downstream_server(server).await?;
        info!(
            "Registered successfully. Assigned Server Id: {} - request_id: {}",
            server_id, request_id
        );

        // create a response with status code 200. Content-Type is JSON
        let json_body = serde_json::json!({
            "id": server_id,
            "url": server_url,
            "kind": server_kind
        });

        let response = axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(json_body.to_string()))
            .map_err(|e| {
                let err_msg = format!("Failed to create response: {}", e);
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                ServerError::Operation(err_msg)
            })?;

        Ok(response)
    }

    // verify the server and get the server info and model list
    async fn verify_server(
        State(state): State<Arc<AppState>>,
        request_id: impl AsRef<str>,
        server_id: impl AsRef<str>,
        server_url: impl AsRef<str>,
        server_kind: &ServerKind,
    ) -> ServerResult<()> {
        let request_id = request_id.as_ref();
        let server_url = server_url.as_ref();
        let server_id = server_id.as_ref();

        let client = reqwest::Client::new();

        let server_info_url = format!("{}/v1/info", server_url);
        let response = client.get(&server_info_url).send().await.map_err(|e| {
            let err_msg = format!(
                "Failed to verify the {} downstream server: {}",
                server_kind, e
            );
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            ServerError::Operation(err_msg)
        })?;

        if !response.status().is_success() {
            let err_msg = format!(
                "Failed to verify the {} downstream server: {}",
                server_kind,
                response.status()
            );
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            return Err(ServerError::Operation(err_msg));
        }

        let mut api_server = response.json::<ApiServer>().await.map_err(|e| {
            let err_msg = format!("Failed to parse the server info: {}", e);
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            ServerError::Operation(err_msg)
        })?;
        api_server.server_id = Some(server_id.to_string());

        info!(target: "stdout", "server kind: {}", server_kind.to_string());
        info!(target: "stdout", "api server: {:?}", api_server);

        // verify the server kind
        {
            if server_kind.contains(ServerKind::chat) && api_server.chat_model.is_none() {
                let err_msg = "You are trying to register a chat server. However, the server does not support `chat`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
            if server_kind.contains(ServerKind::embeddings) && api_server.embedding_model.is_none()
            {
                let err_msg = "You are trying to register an embedding server. However, the server does not support `embeddings`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
            if server_kind.contains(ServerKind::image) && api_server.image_model.is_none() {
                let err_msg = "You are trying to register an image server. However, the server does not support `image`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
            if server_kind.contains(ServerKind::tts) && api_server.tts_model.is_none() {
                let err_msg = "You are trying to register a TTS server. However, the server does not support `tts`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
            if server_kind.contains(ServerKind::translate) && api_server.translate_model.is_none() {
                let err_msg = "You are trying to register a translation server. However, the server does not support `translate`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
            if server_kind.contains(ServerKind::transcribe) && api_server.transcribe_model.is_none()
            {
                let err_msg = "You are trying to register a transcription server. However, the server does not support `transcribe`. Please check the server kind.";
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg.to_string()));
            }
        }

        // // update the server info
        // let server_info = &mut state.server_info.write().await;
        // server_info
        //     .servers
        //     .insert(server_id.to_string(), api_server);

        // // get the models from the downstream server
        // let list_models_url = format!("{}/v1/models", server_url);
        // let list_models_response = client.get(&list_models_url).send().await.map_err(|e| {
        //     let err_msg = format!("Failed to get the models from the downstream server: {}", e);
        //     error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
        //     ServerError::Operation(err_msg)
        // })?;

        // let list_models_response = list_models_response
        //     .json::<ListModelsResponse>()
        //     .await
        //     .map_err(|e| {
        //         let err_msg = format!("Failed to parse the models: {}", e);
        //         error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
        //         ServerError::Operation(err_msg)
        //     })?;

        // // update the models
        // let mut models = state.models.write().await;
        // models.insert(server_id.to_string(), list_models_response.data);

        Ok(())
    }

    pub async fn remove_downstream_server_handler(
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
        Json(server_id): Json<ServerIdToRemove>,
    ) -> ServerResult<Response<Body>> {
        // Get request ID from headers
        let request_id = headers
            .get("x-request-id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        state
            .unregister_downstream_server(&server_id.server_id)
            .await?;

        // create a response with status code 200. Content-Type is JSON
        let json_body = serde_json::json!({
            "message": "Server unregistered successfully.",
            "id": server_id.server_id,
        });

        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(json_body.to_string()))
            .map_err(|e| {
                let err_msg = format!("Failed to create response: {}", e);
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                ServerError::Operation(err_msg)
            })?;

        Ok(response)
    }

    pub async fn list_downstream_servers_handler(
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
    ) -> ServerResult<Response<Body>> {
        // Get request ID from headers
        let request_id = headers
            .get("x-request-id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let servers = state.list_downstream_servers().await?;

        // compute the total number of servers
        let total_servers = servers.values().fold(0, |acc, servers| acc + servers.len());
        info!(target: "stdout", "Found {} downstream servers - request_id: {}", total_servers, request_id);

        let json_body = serde_json::to_string(&servers).unwrap();

        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(json_body))
            .map_err(|e| {
                let err_msg = format!("Failed to create response: {}", e);
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                ServerError::Operation(err_msg)
            })?;

        Ok(response)
    }
}
