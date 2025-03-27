use crate::{
    error::{self, ServerError, ServerResult},
    info::{ApiServer, ModelConfig},
    server::{Server, ServerIdToRemove, ServerKind},
    AppState, RoutingPolicy, SharedClient, UrlType,
};
use axum::{
    body::Body,
    extract::{Json, Path, State},
    http::{HeaderMap, Method, Request, Response, StatusCode, Uri},
    response::IntoResponse,
};
use reqwest::Client;
use std::sync::Arc;

pub(crate) async fn chat_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling chat request");

    let chat_url = match state.chat_urls.read().await.next().await {
        Ok(url) => url,
        Err(e) => {
            let err_msg = e.to_string();
            info!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    proxy_request(state.client, req, chat_url).await
}

pub(crate) async fn rag_chat_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling rag request");

    let rag_url = match state.rag_urls.read().await.next().await {
        Ok(url) => url,
        Err(e) => {
            let err_msg = e.to_string();
            info!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    proxy_request(state.client, req, rag_url).await
}

pub(crate) async fn audio_whisper_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling audio whisper request");

    let audio_url = match state.audio_urls.read().await.next().await {
        Ok(url) => url,
        Err(e) => {
            let err_msg = e.to_string();
            info!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    proxy_request(state.client, req, audio_url).await
}

pub(crate) async fn image_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling image request");

    let image_url = match state.image_urls.read().await.next().await {
        Ok(url) => url,
        Err(e) => {
            let err_msg = e.to_string();
            info!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    proxy_request(state.client, req, image_url).await
}

pub(crate) async fn proxy_request(
    _client: SharedClient, // We'll keep this parameter for now to maintain compatibility
    req: Request<Body>,
    downstream_server_socket_addr: Uri,
) -> Result<Response<Body>, StatusCode> {
    // Change the request URL to the downstream server
    let endpoint = match req.uri().path_and_query().map(|x| x.to_string()) {
        Some(endpoint) => endpoint,
        None => {
            let err_msg = "failed to parse the endpoint from the request uri";
            error!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(err_msg));
        }
    };
    info!(target: "stdout", "endpoint: {}", &endpoint);

    let mut server_socket_addr = downstream_server_socket_addr.to_string();
    server_socket_addr = server_socket_addr.trim_end_matches('/').to_string();

    let new_uri = match format!("{}{}", server_socket_addr, endpoint).parse::<Uri>() {
        Ok(url) => url,
        Err(e) => {
            let err_msg = format!("failed to parse the downstream server URL: {}", e);
            error!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    info!(target: "stdout", "dispatch the chat request to {}", new_uri);

    // Convert axum headers to reqwest headers
    let mut headers = HeaderMap::new();
    for (key, value) in req.headers() {
        headers.insert(key.clone(), value.clone());
    }

    let method = req.method().clone();

    // Get the request body
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(e) => {
            let err_msg = format!("failed to read request body: {}", e);
            error!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    // Create reqwest client
    let client = Client::new();

    // Build the request
    let request_builder = client
        .request(method, new_uri.to_string())
        .headers(headers)
        .body(body);

    // Send the request
    match request_builder.send().await {
        Ok(res) => {
            let status = res.status();
            let headers = res.headers().clone();
            let body = match res.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    let err_msg = format!("failed to read response body: {}", e);
                    error!(target: "stdout", "{}", &err_msg);
                    return Ok(error::internal_server_error(&err_msg));
                }
            };

            // Build the response
            let mut response = Response::builder().status(status);

            // Add headers
            for (key, value) in headers {
                if let Some(key) = key {
                    response = response.header(key, value);
                }
            }

            // Set the body
            match response.body(Body::from(body)) {
                Ok(res) => Ok(res.map(|b| b.into())),
                Err(e) => {
                    let err_msg = format!("failed to build response: {}", e);
                    error!(target: "stdout", "{}", &err_msg);
                    Ok(error::internal_server_error(&err_msg))
                }
            }
        }
        Err(e) => {
            let err_msg = format!(
                "failed to forward the request to the downstream server: {}",
                e
            );
            error!(target: "stdout", "{}", &err_msg);
            Ok(error::internal_server_error(&err_msg))
        }
    }
}

pub(crate) async fn add_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<Response<Body>, StatusCode> {
    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "whisper" => UrlType::AudioWhisper,
        "image" => UrlType::Image,
        "rag" => UrlType::Rag,
        _ => {
            let err_msg = format!("invalid url type: {}", url_type);
            error!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    if let Err(e) = state.add_url(&url_type, &url).await {
        let err_msg = e.to_string();
        info!(target: "stdout", "{}", &err_msg);
        return Ok(error::internal_server_error(&err_msg));
    }

    info!(target: "stdout", "registered new downstream server, type: {}, url: {}", url_type, url);

    // create a response with status code 200. Content-Type is JSON
    let json_body = serde_json::json!({
        "message": "URL registered successfully",
        "url": url.to_string()
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(json_body.to_string()))
        .unwrap();

    Ok(response)
}

pub(crate) async fn remove_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<Response<Body>, StatusCode> {
    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "whisper" => UrlType::AudioWhisper,
        "image" => UrlType::Image,
        "rag" => UrlType::Rag,
        _ => {
            let err_msg = format!("invalid url type: {}", url_type);
            error!(target: "stdout", "{}", &err_msg);
            return Ok(error::internal_server_error(&err_msg));
        }
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    if let Err(e) = state.remove_url(&url_type, &url).await {
        let err_msg = e.to_string();
        error!(target: "stdout", "{}", &err_msg);
        return Ok(error::internal_server_error(&err_msg));
    }

    info!(target: "stdout", "unregistered downstream server, type: {}, url: {}", url_type, url);

    // create a response with status code 200. Content-Type is JSON
    let json_body = serde_json::json!({
        "message": "URL unregistered successfully",
        "url": url.to_string()
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(json_body.to_string()))
        .unwrap();

    Ok(response)
}

pub(crate) async fn list_downstream_servers_handler(
    State(state): State<AppState>,
) -> Result<Response<Body>, StatusCode> {
    let servers = state.list_downstream_servers().await;

    // create a response with status code 200. Content-Type is JSON
    let json_body = serde_json::json!({
        "chat": servers.get("chat").unwrap(),
        "whisper": servers.get("whisper").unwrap(),
        "image": servers.get("image").unwrap(),
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(json_body.to_string()))
        .unwrap();

    Ok(response)
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

        let servers = state.list_downstream_servers_new().await?;

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
