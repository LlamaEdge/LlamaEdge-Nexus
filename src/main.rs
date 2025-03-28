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
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use config::Config;
use error::{ServerError, ServerResult};
use futures_util::StreamExt;
use info::ServerInfo;
use server::{Server, ServerGroup, ServerId, ServerKind};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::services::ServeDir;
use utils::LogLevel;

#[derive(Debug, Parser)]
#[command(version = env!("CARGO_PKG_VERSION"), about = "LlamaEdge Nexus - A gateway service for LLM backends")]
struct Cli {
    /// Path to the config file
    #[arg(long, default_value = "config.toml", value_parser = clap::value_parser!(PathBuf))]
    config: PathBuf,
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

    // socket address
    let addr = SocketAddr::from((
        config.server.host.parse::<IpAddr>().unwrap(),
        config.server.port,
    ));

    let app_state = Arc::new(AppState::new(config, ServerInfo::default()));

    let app = Router::new()
        .route("/v1/chat/completions", post(handler::chat_handler))
        // .route("/v1/completions", post(chat_handler))
        .route("/v1/embeddings", post(handler::embeddings_handler))
        .route(
            "/v1/audio/transcriptions",
            post(handler::audio_transcriptions_handler),
        )
        .route(
            "/v1/audio/translations",
            post(handler::audio_translations_handler),
        )
        .route("/v1/audio/speech", post(handler::audio_tts_handler))
        .route("/v1/images/generations", post(handler::image_handler))
        .route("/v1/images/edits", post(handler::image_handler))
        .route("/v1/create/rag", post(handler::create_rag_handler))
        .route("/v1/chunks", post(handler::chunks_handler))
        .route("/v1/models", get(handler::models_handler))
        .route("/v1/info", get(handler::info_handler))
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
        .nest_service(
            "/",
            ServeDir::new(&cli.web_ui).not_found_service(
                ServeDir::new(&cli.web_ui).append_index_html_on_directories(true),
            ),
        )
        .with_state(app_state.clone());

    // create a tcp listener
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

#[derive(Clone)]
struct AppState {
    config: Arc<RwLock<Config>>,
    server_group: Arc<RwLock<HashMap<ServerKind, ServerGroup>>>,
    server_info: Arc<RwLock<ServerInfo>>,
    models: Arc<RwLock<HashMap<ServerId, Vec<endpoints::models::Model>>>>,
}

impl AppState {
    fn new(config: Config, server_info: ServerInfo) -> Self {
        Self {
            server_group: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            server_info: Arc::new(RwLock::new(server_info)),
            models: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_downstream_server(&self, server: Server) -> ServerResult<()> {
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

        if found {
            // remove the server info from the server_info
            let mut server_info = self.server_info.write().await;
            server_info.servers.remove(server_id.as_ref());

            // remove the server from the models
            let mut models = self.models.write().await;
            models.remove(server_id.as_ref());
        }

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
