#[macro_use]
extern crate log;

use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{Request, Response, StatusCode, Uri},
    routing::post,
    Router,
};
use hyper::{client::HttpConnector, Client};
use serde_json::Value;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

type SharedClient = Arc<Client<HttpConnector>>;

#[derive(Clone)]
struct AppState {
    client: SharedClient,
    chat_urls: Vec<Uri>,
    image_urls: Vec<Uri>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Create a shared HTTP client
    let client = Arc::new(Client::new());

    let app_state = AppState {
        client,
        chat_urls: vec![
            "http://localhost:12345".parse().unwrap(),
            "http://localhost:12346".parse().unwrap(),
        ],
        image_urls: vec![
            "http://localhost:12306".parse().unwrap(),
            "http://localhost:12307".parse().unwrap(),
        ],
    };

    // Build our application with routes
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_handler))
        .route("/v1/image/generation", post(image_handler))
        .with_state(app_state);

    // Run it
    let addr = "0.0.0.0:12123";
    let tcp_listener = TcpListener::bind(addr).await.unwrap();
    println!("LlamaEdge listening on {}", addr);
    if let Err(e) = axum::Server::from_tcp(tcp_listener.into_std().unwrap())
        .unwrap()
        .serve(app.into_make_service())
        .await
    {
        println!("server error: {}", e);
    }
}

async fn chat_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    println!("In chat_handler");

    // Choose a chat URL (for now, just use the first one)
    let chat_url = state
        .chat_urls
        .first()
        .cloned()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    proxy_request(state.client, req, chat_url).await
}

async fn image_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    println!("In image_handler");

    // Choose an image URL (for now, just use the first one)
    let image_url = state
        .image_urls
        .first()
        .cloned()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    proxy_request(state.client, req, image_url).await
}

async fn proxy_request(
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

    println!("new_uri: {}", new_uri);

    *req.uri_mut() = new_uri;

    // Forward the request to the downstream server
    match client.request(req).await {
        Ok(res) => Ok(res),
        Err(_) => Err(StatusCode::BAD_GATEWAY),
    }
}
