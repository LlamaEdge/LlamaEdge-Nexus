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

use axum::extract::Extension;
use std::sync::RwLock;

type SharedClient = Arc<Client<HttpConnector>>;

#[derive(Clone)]
struct AppState {
    client: SharedClient,
    chat_urls: Arc<RwLock<Vec<Uri>>>,
    image_urls: Arc<RwLock<Vec<Uri>>>,
}

impl AppState {
    fn new(client: SharedClient) -> Self {
        Self {
            client,
            chat_urls: Arc::new(RwLock::new(Vec::new())),
            image_urls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn add_url(&self, url_type: UrlType, url: &Uri) {
        match url_type {
            UrlType::Chat => self.chat_urls.write().unwrap().push(url.clone()),
            UrlType::Image => self.image_urls.write().unwrap().push(url.clone()),
        }
    }

    fn remove_url(&self, url_type: UrlType, url: &Uri) {
        match url_type {
            UrlType::Chat => self.chat_urls.write().unwrap().retain(|u| u != url),
            UrlType::Image => self.image_urls.write().unwrap().retain(|u| u != url),
        }
    }
}

enum UrlType {
    Chat,
    Image,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Create a shared HTTP client
    let client = Arc::new(Client::new());

    let app_state = AppState::new(client);

    // // Add initial URLs
    // app_state.add_url(UrlType::Chat, "http://localhost:12345".parse().unwrap());
    // app_state.add_url(UrlType::Chat, "http://localhost:12346".parse().unwrap());
    // app_state.add_url(UrlType::Image, "http://localhost:12306".parse().unwrap());
    // app_state.add_url(UrlType::Image, "http://localhost:12307".parse().unwrap());

    // Build our application with routes
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_handler))
        .route("/v1/image/generation", post(image_handler))
        .route("/admin/register/:type", post(add_url_handler))
        .route("/admin/unregister/:type", post(remove_url_handler))
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
        .read()
        .unwrap()
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
        .read()
        .unwrap()
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

    println!("dispatch the chat request to {}", new_uri);

    *req.uri_mut() = new_uri;

    // Forward the request to the downstream server
    match client.request(req).await {
        Ok(res) => Ok(res),
        Err(_) => Err(StatusCode::BAD_GATEWAY),
    }
}

async fn add_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    println!("In add_url_handler");
    println!("url_type: {}", url_type);
    println!("body: {}", &body);

    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "image" => UrlType::Image,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    state.add_url(url_type, &url);

    println!("registered {}", url);

    Ok(StatusCode::OK)
}

async fn remove_url_handler(
    State(state): State<AppState>,
    Path(url_type): Path<String>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    println!("In remove_url_handler");

    let url_type = match url_type.as_str() {
        "chat" => UrlType::Chat,
        "image" => UrlType::Image,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let url: Uri = body.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    state.remove_url(url_type, &url);

    println!("unregistered {}", url);

    Ok(StatusCode::OK)
}
