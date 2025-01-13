use crate::{error, AppState, RoutingPolicy, SharedClient, UrlType};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, Response, StatusCode, Uri},
};

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
    client: SharedClient,
    mut req: Request<Body>,
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

    *req.uri_mut() = new_uri;

    // Forward the request to the downstream server
    match client.request(req).await {
        Ok(res) => Ok(res),
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
