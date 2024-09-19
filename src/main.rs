#[macro_use]
extern crate log;

use async_trait::async_trait;
use axum::extract::Extension;
use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{Request, Response, StatusCode, Uri},
    routing::post,
    Router,
};
use hyper::{client::HttpConnector, Client};
use serde_json::Value;
use std::fmt::{self, write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

type SharedClient = Arc<Client<HttpConnector>>;

#[async_trait]
pub trait RoutingPolicy {
    fn next(&self) -> Uri;
}

#[derive(Debug)]
struct Server {
    url: Uri,
    connections: AtomicUsize,
}
impl Server {
    fn new(url: Uri) -> Self {
        Self {
            url,
            connections: AtomicUsize::new(0),
        }
    }
}

#[derive(Debug, Default)]
struct Services {
    servers: RwLock<Vec<Server>>,
}
impl Services {
    fn push(&mut self, url: Uri) {
        let server = Server::new(url);
        self.servers.write().unwrap().push(server)
    }
}
impl RoutingPolicy for Services {
    fn next(&self) -> Uri {
        let servers = self.servers.read().unwrap();
        let server = servers
            .iter()
            .min_by(|s1, s2| {
                s1.connections
                    .load(Ordering::Relaxed)
                    .cmp(&s2.connections.load(Ordering::Relaxed))
            })
            .unwrap();

        server.connections.fetch_add(1, Ordering::Relaxed);
        server.url.clone()
    }
}

#[derive(Clone)]
struct AppState {
    client: SharedClient,
    chat_urls: Arc<RwLock<Services>>,
    image_urls: Arc<RwLock<Services>>,
}

impl AppState {
    fn new(client: SharedClient) -> Self {
        Self {
            client,
            chat_urls: Arc::new(RwLock::new(Services::default())),
            image_urls: Arc::new(RwLock::new(Services::default())),
        }
    }

    fn add_url(&self, url_type: UrlType, url: &Uri) {
        match url_type {
            UrlType::Chat => self.chat_urls.write().unwrap().push(url.clone()),
            UrlType::Image => self.image_urls.write().unwrap().push(url.clone()),
            // UrlType::Chat => self.chat_urls.write().unwrap().push(url.clone()),
            // UrlType::Image => self.image_urls.write().unwrap().push(url.clone()),
        }
    }

    fn remove_url(&self, url_type: UrlType, url: &Uri) {
        let services = match &url_type {
            UrlType::Chat => &self.chat_urls,
            UrlType::Image => &self.image_urls,
        };

        let services = services.write().unwrap();
        services
            .servers
            .write()
            .unwrap()
            .retain(|server| &server.url != url);

        // Optionally, log the removal
        println!("Removed {} URL: {}", url_type, url);
    }
}

#[derive(Debug)]
enum UrlType {
    Chat,
    Image,
}
impl fmt::Display for UrlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlType::Chat => write!(f, "Chat"),
            UrlType::Image => write!(f, "Image"),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Create a shared HTTP client
    let client = Arc::new(Client::new());

    let app_state = AppState::new(client);

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

    // TODO use round-robin policy to decide which chat url to dispatch to
    let chat_url = state.chat_urls.read().unwrap().next();

    proxy_request(state.client, req, chat_url).await
}

async fn image_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    println!("In image_handler");

    let image_url = state.image_urls.read().unwrap().next();

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
