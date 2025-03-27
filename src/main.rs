#[macro_use]
extern crate log;

mod config;
mod error;
mod handler;
mod info;
mod rag;
mod server;
mod utils;

use anyhow::Result;
use async_trait::async_trait;
use axum::{http::Uri, routing::post, Router};
use clap::Parser;
use config::Config;
use error::{ServerError, ServerResult};
use futures_util::StreamExt;
use hyper::{client::HttpConnector, Client};
use info::ServerInfo;
use server::{ServerGroup, ServerKind};
use std::{
    collections::HashMap,
    fmt,
    net::SocketAddr,
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{net::TcpListener, sync::RwLock};
use utils::LogLevel;

type SharedClient = Arc<Client<HttpConnector>>;

// default port of LlamaEdge Proxy Server
const DEFAULT_PORT: &str = "8080";

#[derive(Debug, Parser)]
#[command(version = env!("CARGO_PKG_VERSION"), about = "LlamaEdge Nexus - A gateway service for LLM backends")]
struct Cli {
    /// Path to the config file
    #[arg(long, default_value = "config.toml", value_parser = clap::value_parser!(PathBuf))]
    config: PathBuf,
    /// Socket address of llama-proxy-server instance. For example, `0.0.0.0:8080`.
    #[arg(long, default_value = None, value_parser = clap::value_parser!(SocketAddr), group = "socket_address_group")]
    socket_addr: Option<SocketAddr>,
    /// Socket address of llama-proxy-server instance
    #[arg(long, default_value = DEFAULT_PORT, value_parser = clap::value_parser!(u16), group = "socket_address_group")]
    port: u16,
    /// Use rag-api-server instances as downstream server instead of llama-api-server instances
    #[arg(long)]
    rag: bool,
    /// Root path for the Web UI files
    #[arg(long, default_value = "chatbot-ui")]
    web_ui: PathBuf,
}

#[allow(clippy::needless_return)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ServerError> {
    // get the environment variable `RUST_LOG`
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default().to_lowercase();
    let (_, log_level) = match rust_log.is_empty() {
        true => ("stdout", LogLevel::Info),
        false => match rust_log.split_once("=") {
            Some((target, level)) => (target, level.parse().unwrap_or(LogLevel::Info)),
            None => ("stdout", rust_log.parse().unwrap_or(LogLevel::Info)),
        },
    };

    // set global logger
    wasi_logger::Logger::install().expect("failed to install wasi_logger::Logger");
    log::set_max_level(log_level.into());

    // parse the command line arguments
    let cli = Cli::parse();

    // log the version of the server
    info!(target: "stdout", "version: {}", env!("CARGO_PKG_VERSION"));

    // Create a shared HTTP client
    let client = Arc::new(Client::new());

    // Load the config based on the command
    let config = match Config::load(&cli.config) {
        Ok(mut config) => {
            if cli.rag {
                config.rag.enable = true;
                info!(target: "stdout", "RAG is enabled");
            }

            config
        }
        Err(e) => {
            let err_msg = format!("Failed to load config: {}", e);
            error!(target: "stdout", "{}", err_msg);
            return Err(ServerError::FailedToLoadConfig(err_msg));
        }
    };

    let app_state = Arc::new(AppState::new(client, config, ServerInfo::default()));

    let app = Router::new()
        .route("/v1/chat/completions", post(handler::chat_handler))
        // .route("/v1/completions", post(chat_handler))
        .route("/v1/models", post(handler::embeddings_handler))
        // .route("/v1/embeddings", post(chat_handler))
        // .route("/v1/files", post(chat_handler))
        // .route("/v1/chunks", post(chat_handler))
        // .route("/v1/audio/transcriptions", post(audio_whisper_handler))
        // .route("/v1/audio/translations", post(audio_whisper_handler))
        // .route("/v1/images/generations", post(image_handler))
        // .route("/v1/images/edits", post(image_handler))
        // .route("/admin/register/:type", post(add_url_handler))
        // .route("/admin/unregister/:type", post(remove_url_handler))
        .route(
            "/admin/servers/register",
            post(handler::admin::register_downstream_server_handler),
        )
        .route(
            "/admin/servers/unregister",
            post(handler::admin::remove_downstream_server_handler),
        )
        .route(
            "/admin/servers",
            post(handler::admin::list_downstream_servers_handler),
        )
        // .nest_service(
        //     "/",
        //     ServeDir::new(&cli.web_ui).not_found_service(
        //         ServeDir::new(&cli.web_ui).append_index_html_on_directories(true),
        //     ),
        // )
        .with_state(app_state);

    // socket address
    let addr = match cli.socket_addr {
        Some(addr) => addr,
        None => SocketAddr::from(([0, 0, 0, 0], cli.port)),
    };
    let tcp_listener = TcpListener::bind(addr).await.unwrap();
    info!(target: "stdout", "Listening on {}", addr);

    // run
    match axum::Server::from_tcp(tcp_listener.into_std().unwrap())
        .unwrap()
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(ServerError::Operation(e.to_string())),
    }
}

/// Represents a LlamaEdge API server
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

#[derive(Debug)]
struct Services {
    servers: RwLock<Vec<Server>>,
    ty: UrlType,
}
impl Services {
    fn new(ty: UrlType) -> Self {
        Self {
            servers: RwLock::new(Vec::new()),
            ty,
        }
    }

    fn ty(&self) -> UrlType {
        self.ty.clone()
    }

    async fn push(&mut self, url: Uri) {
        let server = Server::new(url);
        self.servers.write().await.push(server)
    }
}

#[derive(Clone)]
struct AppState {
    client: SharedClient,
    chat_urls: Arc<RwLock<Services>>,
    audio_urls: Arc<RwLock<Services>>,
    image_urls: Arc<RwLock<Services>>,
    rag_urls: Arc<RwLock<Services>>,
    config: Arc<RwLock<Config>>,
    server_group: Arc<RwLock<HashMap<ServerKind, ServerGroup>>>,
    server_info: Arc<RwLock<ServerInfo>>,
}

impl AppState {
    fn new(client: SharedClient, config: Config, server_info: ServerInfo) -> Self {
        Self {
            client,
            chat_urls: Arc::new(RwLock::new(Services::new(UrlType::Chat))),
            audio_urls: Arc::new(RwLock::new(Services::new(UrlType::AudioWhisper))),
            image_urls: Arc::new(RwLock::new(Services::new(UrlType::Image))),
            rag_urls: Arc::new(RwLock::new(Services::new(UrlType::Rag))),
            server_group: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            server_info: Arc::new(RwLock::new(server_info)),
        }
    }

    pub async fn register_downstream_server(
        &self,
        server: crate::server::Server,
    ) -> ServerResult<()> {
        if server.kind.contains(ServerKind::chat) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::chat)
                .or_insert(ServerGroup::new(ServerKind::chat))
                .register(server.clone())
                .await?;
        }
        if server.kind.contains(ServerKind::embeddings) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::embeddings)
                .or_insert(ServerGroup::new(ServerKind::embeddings))
                .register(server.clone())
                .await?;
        }
        if server.kind.contains(ServerKind::image) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::image)
                .or_insert(ServerGroup::new(ServerKind::image))
                .register(server.clone())
                .await?;
        }
        if server.kind.contains(ServerKind::tts) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::tts)
                .or_insert(ServerGroup::new(ServerKind::tts))
                .register(server.clone())
                .await?;
        }
        if server.kind.contains(ServerKind::translate) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::translate)
                .or_insert(ServerGroup::new(ServerKind::translate))
                .register(server.clone())
                .await?;
        }
        if server.kind.contains(ServerKind::transcribe) {
            self.server_group
                .write()
                .await
                .entry(ServerKind::transcribe)
                .or_insert(ServerGroup::new(ServerKind::transcribe))
                .register(server.clone())
                .await?;
        }

        Ok(())
    }

    pub async fn unregister_downstream_server(
        &self,
        server_id: impl AsRef<str>,
    ) -> ServerResult<()> {
        let mut found = false;

        // unregister the server from the servers
        {
            // parse server kind from server id
            let kinds = server_id
                .as_ref()
                .split("-server-")
                .next()
                .unwrap()
                .split("-")
                .collect::<Vec<&str>>();

            let group_map = self.server_group.read().await;

            for kind in kinds {
                let kind = ServerKind::from_str(kind).unwrap();
                if let Some(group) = group_map.get(&kind) {
                    group.unregister(server_id.as_ref()).await?;
                    info!(target: "stdout", "Unregistered {} server: {}", &kind, server_id.as_ref());

                    if !found {
                        found = true;
                    }
                }
            }
        }

        // if found {
        //     // remove the server info from the server_info
        //     let mut server_info = self.server_info.write().await;
        //     server_info.servers.remove(server_id.as_ref());

        //     // remove the server from the models
        //     let mut models = self.models.write().await;
        //     models.remove(server_id.as_ref());
        // }

        if !found {
            return Err(ServerError::Operation(format!(
                "Server {} not found",
                server_id.as_ref()
            )));
        }

        Ok(())
    }

    pub(crate) async fn list_downstream_servers(
        &self,
    ) -> ServerResult<HashMap<ServerKind, Vec<crate::server::Server>>> {
        let servers = self.server_group.read().await;

        let mut server_groups = HashMap::new();
        for (kind, group) in servers.iter() {
            if !group.is_empty().await {
                let servers = group.servers.read().await;

                // Create a new Vec with cloned Server instances using async stream
                let server_vec = futures_util::stream::iter(servers.iter())
                    .then(|server_lock| async move {
                        let server = server_lock.read().await;
                        server.clone()
                    })
                    .collect::<Vec<_>>()
                    .await;

                server_groups.insert(*kind, server_vec);
            }
        }

        Ok(server_groups)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum UrlType {
    AudioWhisper,
    Chat,
    Image,
    Rag,
}
impl fmt::Display for UrlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlType::Chat => write!(f, "chat"),
            UrlType::AudioWhisper => write!(f, "whisper"),
            UrlType::Image => write!(f, "image"),
            UrlType::Rag => write!(f, "rag"),
        }
    }
}
