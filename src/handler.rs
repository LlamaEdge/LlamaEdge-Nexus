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

    let chat_url = state.chat_urls.read().unwrap().next();

    if let Err(e) = chat_url {
        let err_msg = e.to_string();
        info!(target: "stdout", "{}", &err_msg);
        return Ok(error::internal_server_error(&err_msg));
    }

    proxy_request(state.client, req, chat_url.unwrap()).await
}

pub(crate) async fn audio_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling audio request");

    let audio_url = state.audio_urls.read().unwrap().next();

    if let Err(e) = audio_url {
        let err_msg = e.to_string();
        info!(target: "stdout", "{}", &err_msg);
        return Ok(error::internal_server_error(&err_msg));
    }

    proxy_request(state.client, req, audio_url.unwrap()).await
}

pub(crate) async fn image_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!(target: "stdout", "handling image request");

    let image_url = state.image_urls.read().unwrap().next();

    if let Err(e) = image_url {
        let err_msg = e.to_string();
        info!(target: "stdout", "{}", &err_msg);
        return Ok(error::internal_server_error(&err_msg));
    }

    proxy_request(state.client, req, image_url.unwrap()).await
}

pub(crate) async fn proxy_request(
    client: SharedClient,
    mut req: Request<Body>,
    downstream_url: Uri,
) -> Result<Response<Body>, StatusCode> {
    // Change the request URL to the downstream server
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|x| x.to_string())
        .unwrap_or_default();
    println!("path_and_query: {}", path_and_query);

    let mut downstream_url_s = downstream_url.to_string();
    downstream_url_s = downstream_url_s.trim_end_matches('/').to_string();
    println!("downstream_url_s: {}", downstream_url_s);

    let new_uri = format!("{}{}", downstream_url_s, path_and_query)
        .parse()
        .unwrap();

    println!("dispatch the chat request to {}", new_uri);

    *req.uri_mut() = new_uri;

    // Forward the request to the downstream server
    match client.request(req).await {
        Ok(res) => Ok(res),
        Err(_) => Err(StatusCode::BAD_GATEWAY),
    }
}

pub(crate) async fn add_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    println!("In add_url_handler");
    println!("url_type: {}", url_type);
    println!("body: {}", &body);

    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "audio" => UrlType::Audio,
        "image" => UrlType::Image,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    state.add_url(url_type, &url);

    println!("registered {}", url);

    Ok(StatusCode::OK)
}

pub(crate) async fn remove_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    println!("In remove_url_handler");

    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "audio" => UrlType::Audio,
        "image" => UrlType::Image,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    state.remove_url(url_type, &url);

    println!("unregistered {}", url);

    Ok(StatusCode::OK)
}
