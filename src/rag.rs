use crate::{
    error::{ServerError, ServerResult},
    AppState,
};
use axum::{
    body::Body,
    extract::{Json, State},
    http::{HeaderMap, Response},
};
use chat_prompts::{error as ChatPromptsError, MergeRagContext, MergeRagContextPolicy};
use endpoints::{
    chat::{ChatCompletionRequest, ChatCompletionRequestMessage, ChatCompletionUserMessageContent},
    embeddings::{EmbeddingObject, EmbeddingRequest, EmbeddingsResponse, InputText},
    rag::{RagScoredPoint, RetrieveObject},
};
use qdrant::{Point, PointId, ScoredPoint};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, fmt, sync::Arc};
use text_splitter::{MarkdownSplitter, TextSplitter};

pub async fn chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut chat_request): Json<ChatCompletionRequest>,
) -> ServerResult<Response<Body>> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    info!(target: "stdout", "Received a new chat request - request_id: {}", request_id);

    // qdrant config
    let qdrant_config_vec =
        match get_qdrant_configs(State(state.clone()), &chat_request, &request_id).await {
            Ok(qdrant_config_vec) => qdrant_config_vec,
            Err(e) => {
                let err_msg = format!("Failed to get the VectorDB config: {}", e);
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                return Err(ServerError::Operation(err_msg));
            }
        };

    // retrieve context
    let retrieve_object_vec = retrieve_context_with_multiple_qdrant_configs(
        State(state.clone()),
        headers.clone(),
        &request_id,
        &chat_request,
        &qdrant_config_vec,
    )
    .await?;

    // log retrieve object
    debug!(target: "stdout", "request_id: {} - retrieve_object_vec:\n{}", request_id, serde_json::to_string_pretty(&retrieve_object_vec).unwrap());

    // extract the context from retrieved objects
    let mut context = String::new();
    for (idx, retrieve_object) in retrieve_object_vec.iter().enumerate() {
        match retrieve_object.points.as_ref() {
            Some(scored_points) => {
                match scored_points.is_empty() {
                    false => {
                        for (idx, point) in scored_points.iter().enumerate() {
                            // log
                            debug!(target: "stdout", "request_id: {} - Point-{}, score: {}, source: {}", request_id, idx, point.score, &point.source);

                            context.push_str(&point.source);
                            context.push_str("\n\n");
                        }
                    }
                    true => {
                        // log
                        warn!(target: "stdout", "No point retrieved from the collection `{}` (score < threshold {}) - request_id: {}", qdrant_config_vec[idx].collection_name, qdrant_config_vec[idx].score_threshold, request_id);
                    }
                }
            }
            None => {
                // log
                warn!(target: "stdout", "No point retrieved from the collection `{}` (score < threshold {}) - request_id: {}", qdrant_config_vec[idx].collection_name, qdrant_config_vec[idx].score_threshold, request_id);
            }
        }
    }
    debug!(target: "stdout", "request_id: {} - context:\n{}", request_id, context);

    // merge context into chat request
    if !context.is_empty() {
        if chat_request.messages.is_empty() {
            let err_msg = "Found empty chat messages";

            // log
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            return Err(ServerError::BadRequest(err_msg.to_string()));
        }

        // get the prompt template from the chat server
        let prompt_template = {
            let server_info = state.server_info.read().await;
            let chat_server = server_info
                .servers
                .iter()
                .find(|(_server_id, server)| server.chat_model.is_some());
            match chat_server {
                Some((_server_id, chat_server)) => {
                    let chat_model = chat_server.chat_model.as_ref().unwrap();
                    chat_model.prompt_template.unwrap()
                }
                None => {
                    let err_msg = "No chat server available";
                    error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
                    return Err(ServerError::Operation(err_msg.to_string()));
                }
            }
        };

        // get the rag policy
        let (rag_policy, rag_prompt) = {
            let config = state.config.read().await;
            (
                config.rag.rag_policy.to_owned(),
                config.rag.prompt.to_owned(),
            )
        };

        // insert rag context into chat request
        if let Err(e) = RagPromptBuilder::build(
            &mut chat_request.messages,
            &[context],
            prompt_template.has_system_prompt(),
            rag_policy,
            rag_prompt,
        ) {
            let err_msg = e.to_string();

            // log
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            return Err(ServerError::Operation(err_msg));
        }
    }

    // perform chat completion
    crate::handler::chat(State(state.clone()), headers, Json(chat_request)).await
}

async fn get_qdrant_configs(
    State(state): State<Arc<AppState>>,
    chat_request: &ChatCompletionRequest,
    request_id: impl AsRef<str>,
) -> Result<Vec<QdrantConfig>, ServerError> {
    let request_id = request_id.as_ref();

    match (
        chat_request.vdb_server_url.as_deref(),
        chat_request.vdb_collection_name.as_deref(),
        chat_request.limit.as_deref(),
        chat_request.score_threshold.as_deref(),
    ) {
        (Some(url), Some(collection_name), Some(limit), Some(score_threshold)) => {
            // check if the length of collection name, limit, score_threshold are same
            if collection_name.len() != limit.len()
                || collection_name.len() != score_threshold.len()
            {
                let err_msg =
                    "The number of elements of `collection name`, `limit`, `score_threshold` in the request should be same.";

                // log
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

                return Err(ServerError::Operation(err_msg.into()));
            }

            info!(target: "stdout", "Use the VectorDB settings from the request - request_id: {}", request_id);

            let collection_name_str = collection_name.join(",");
            let limit_str = limit
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<String>>()
                .join(",");
            let score_threshold_str = score_threshold
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(",");
            info!(target: "stdout", "qdrant url: {}, collection name: {}, limit: {}, score threshold: {} - request_id: {}", url, collection_name_str, limit_str, score_threshold_str, request_id);

            let mut qdrant_config_vec = vec![];
            for (idx, col_name) in collection_name.iter().enumerate() {
                qdrant_config_vec.push(QdrantConfig {
                    url: url.to_string(),
                    collection_name: col_name.to_string(),
                    limit: limit[idx],
                    score_threshold: score_threshold[idx],
                });
            }

            Ok(qdrant_config_vec)
        }
        (None, None, None, None) => {
            info!(target: "stdout", "Use the default VectorDB settings - request_id: {}", request_id);

            let vdb_config = &state.config.read().await.rag.vector_db;
            let mut qdrant_config_vec = vec![];
            for cname in vdb_config.collection_name.iter() {
                qdrant_config_vec.push(QdrantConfig {
                    url: vdb_config.url.clone(),
                    collection_name: cname.clone(),
                    limit: vdb_config.limit,
                    score_threshold: vdb_config.score_threshold,
                });
            }

            Ok(qdrant_config_vec)
        }
        _ => {
            let err_msg = "The VectorDB settings in the request are not correct. The `url_vdb_server`, `collection_name`, `limit`, `score_threshold` fields in the request should be provided. The number of elements of `collection name`, `limit`, `score_threshold` should be same.";

            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            Err(ServerError::Operation(err_msg.into()))
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub collection_name: String,
    pub limit: u64,
    pub score_threshold: f32,
}
impl fmt::Display for QdrantConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "url: {}, collection_name: {}, limit: {}, score_threshold: {}",
            self.url, self.collection_name, self.limit, self.score_threshold
        )
    }
}

async fn retrieve_context_with_multiple_qdrant_configs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request_id: impl AsRef<str>,
    chat_request: &ChatCompletionRequest,
    qdrant_config_vec: &[QdrantConfig],
) -> Result<Vec<RetrieveObject>, ServerError> {
    let mut retrieve_object_vec: Vec<RetrieveObject> = Vec::new();
    let mut set: HashSet<String> = HashSet::new();
    for qdrant_config in qdrant_config_vec {
        let mut retrieve_object = retrieve_context_with_single_qdrant_config(
            State(state.clone()),
            headers.clone(),
            request_id.as_ref(),
            chat_request,
            qdrant_config,
        )
        .await?;

        if let Some(points) = retrieve_object.points.as_mut() {
            if !points.is_empty() {
                // find the duplicate points
                let mut idx_removed = vec![];
                for (idx, point) in points.iter().enumerate() {
                    if set.contains(&point.source) {
                        idx_removed.push(idx);
                    } else {
                        set.insert(point.source.clone());
                    }
                }

                // remove the duplicate points
                if !idx_removed.is_empty() {
                    let num = idx_removed.len();

                    for idx in idx_removed.iter().rev() {
                        points.remove(*idx);
                    }

                    info!(target: "stdout", "removed duplicated {} point(s) retrieved from the collection `{}` - request_id: {}", num, qdrant_config.collection_name, request_id.as_ref());
                }

                if !points.is_empty() {
                    retrieve_object_vec.push(retrieve_object);
                }
            }
        }
    }

    Ok(retrieve_object_vec)
}

async fn retrieve_context_with_single_qdrant_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request_id: impl AsRef<str>,
    chat_request: &ChatCompletionRequest,
    qdrant_config: &QdrantConfig,
) -> Result<RetrieveObject, ServerError> {
    let request_id = request_id.as_ref();

    info!(target: "stdout", "Computing embeddings for user query - request_id: {}", request_id);

    // get the context window from config
    let config_ctx_window = state.config.read().await.rag.context_window;

    // get context_window: chat_request.context_window prioritized CONTEXT_WINDOW
    let context_window = chat_request
        .context_window
        .or(Some(config_ctx_window))
        .unwrap_or(1);
    info!(target: "stdout", "Context window: {} - request_id: {}", context_window, request_id);

    // compute embeddings for user query
    let embedding_response = match chat_request.messages.is_empty() {
        true => {
            let err_msg = "Found empty chat messages";

            // log
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            return Err(ServerError::BadRequest(err_msg.to_string()));
        }
        false => {
            // get the last `n` user messages in the context window.
            // `n` is determined by the `context_window` in the chat request.
            let mut last_n_user_messages = Vec::new();
            for (idx, message) in chat_request.messages.iter().rev().enumerate() {
                if let ChatCompletionRequestMessage::User(user_message) = message {
                    if let ChatCompletionUserMessageContent::Text(text) = user_message.content() {
                        if !text.ends_with("<server-health>") {
                            last_n_user_messages.push(text.clone());
                        } else if idx == 0 {
                            let content = text.trim_end_matches("<server-health>").to_string();
                            last_n_user_messages.push(content);
                            break;
                        }
                    }
                }

                if last_n_user_messages.len() == context_window as usize {
                    break;
                }
            }

            // join the user messages in the context window into a single string
            let query_text = if !last_n_user_messages.is_empty() {
                info!(target: "stdout", "Found the latest {} user message(s) - request_id: {}", last_n_user_messages.len(), request_id);

                last_n_user_messages.reverse();
                last_n_user_messages.join("\n")
            } else {
                let error_msg = "No user messages found.";

                // log
                error!(target: "stdout", "{} - request_id: {}", error_msg, request_id);

                return Err(ServerError::BadRequest(error_msg.to_string()));
            };

            // log
            info!(target: "stdout", "Query text for the context retrieval: {} - request_id: {}", query_text, request_id);

            // create a embedding request
            let embedding_request = EmbeddingRequest {
                model: None,
                input: InputText::String(query_text),
                encoding_format: None,
                user: chat_request.user.clone(),
                vdb_server_url: None,
                vdb_collection_name: None,
                vdb_api_key: None,
            };

            // compute embeddings for query
            let response = crate::handler::embeddings_handler(
                State(state.clone()),
                headers.clone(),
                Json(embedding_request),
            )
            .await?;

            // parse the response
            let bytes = hyper::body::to_bytes(response.into_body())
                .await
                .map_err(|e| {
                    let err_msg = format!("Failed to parse embeddings response: {}", e);

                    // log
                    error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

                    ServerError::Operation(err_msg)
                })?;

            // parse the response
            serde_json::from_slice::<EmbeddingsResponse>(&bytes).map_err(|e| {
                let err_msg = format!("Failed to parse embeddings response: {}", e);

                // log
                error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

                ServerError::Operation(err_msg)
            })?
        }
    };

    let query_embedding: Vec<f32> = match embedding_response.data.first() {
        Some(embedding) => embedding.embedding.iter().map(|x| *x as f32).collect(),
        None => {
            let err_msg = "No embeddings returned";

            // log
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            return Err(ServerError::Operation(err_msg.to_string()));
        }
    };

    // get vdb_api_key if it is provided in the request, otherwise get it from the environment variable `VDB_API_KEY`
    let vdb_api_key = chat_request
        .vdb_api_key
        .clone()
        .or_else(|| std::env::var("VDB_API_KEY").ok());

    // perform the context retrieval
    let mut retrieve_object: RetrieveObject = match retrieve_context(
        query_embedding.as_slice(),
        &qdrant_config.url,
        &qdrant_config.collection_name,
        qdrant_config.limit as usize,
        Some(qdrant_config.score_threshold),
        vdb_api_key,
        request_id,
    )
    .await
    {
        Ok(search_result) => search_result,
        Err(e) => {
            let err_msg = format!("No point retrieved. {}", e);

            // log
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

            return Err(ServerError::Operation(err_msg));
        }
    };
    if retrieve_object.points.is_none() {
        retrieve_object.points = Some(Vec::new());
    }

    info!(target: "stdout", "Retrieved {} point(s) from the collection `{}` - request_id: {}", retrieve_object.points.as_ref().unwrap().len(), qdrant_config.collection_name, request_id);

    Ok(retrieve_object)
}

async fn retrieve_context(
    query_embedding: &[f32],
    vdb_server_url: impl AsRef<str>,
    vdb_collection_name: impl AsRef<str>,
    limit: usize,
    score_threshold: Option<f32>,
    vdb_api_key: Option<String>,
    request_id: impl AsRef<str>,
) -> Result<RetrieveObject, ServerError> {
    let request_id = request_id.as_ref();

    info!(target: "stdout", "Retrieve context from {}/collections/{}, max number of result to return: {}, score threshold: {} - request_id: {}", vdb_server_url.as_ref(), vdb_collection_name.as_ref(), limit, score_threshold.unwrap_or_default(), request_id);

    // create a Qdrant client
    let mut qdrant_client = qdrant::Qdrant::new_with_url(vdb_server_url.as_ref().to_string());

    // set the API key if provided
    if let Some(key) = vdb_api_key.as_deref() {
        if !key.is_empty() {
            debug!(target: "stdout", "Set the API key for the VectorDB server - request_id: {}", request_id);
            qdrant_client.set_api_key(key);
        }
    }

    info!(target: "stdout", "Search similar points from the qdrant instance - request_id: {}", request_id);

    // search for similar points
    let scored_points = qdrant_client
        .search_points(
            vdb_collection_name.as_ref(),
            query_embedding.to_vec(),
            limit as u64,
            score_threshold,
        )
        .await
        .map_err(|e| {
            let err_msg = format!(
                "Failed to search similar points from the qdrant instance: {}",
                e
            );
            error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);
            ServerError::Operation(err_msg)
        })?;

    info!(target: "stdout", "Try to remove duplicated points - request_id: {}", request_id);

    // remove duplicates, which have the same source
    let mut seen = HashSet::new();
    let unique_scored_points: Vec<ScoredPoint> = scored_points
        .into_iter()
        .filter(|point| {
            seen.insert(
                point
                    .payload
                    .as_ref()
                    .unwrap()
                    .get("source")
                    .unwrap()
                    .to_string(),
            )
        })
        .collect();

    debug!(target: "stdout", "Found {} unique scored points - request_id: {}", unique_scored_points.len(), request_id);

    let ro = match unique_scored_points.is_empty() {
        true => RetrieveObject {
            points: None,
            limit,
            score_threshold: score_threshold.unwrap_or(0.0),
        },
        false => {
            let mut points: Vec<RagScoredPoint> = vec![];
            for point in unique_scored_points.iter() {
                if let Some(payload) = &point.payload {
                    if let Some(source) = payload.get("source").and_then(Value::as_str) {
                        points.push(RagScoredPoint {
                            source: source.to_string(),
                            score: point.score,
                        })
                    }

                    // For debugging purpose, log the optional search field if it exists
                    if let Some(search) = payload.get("search").and_then(Value::as_str) {
                        info!(target: "stdout", "search: {} - request_id: {}", search, request_id);
                    }
                }
            }

            RetrieveObject {
                points: Some(points),
                limit,
                score_threshold: score_threshold.unwrap_or(0.0),
            }
        }
    };

    Ok(ro)
}

#[derive(Debug, Default)]
struct RagPromptBuilder;
impl MergeRagContext for RagPromptBuilder {
    fn build(
        messages: &mut Vec<endpoints::chat::ChatCompletionRequestMessage>,
        context: &[String],
        has_system_prompt: bool,
        policy: MergeRagContextPolicy,
        rag_prompt: Option<String>,
    ) -> ChatPromptsError::Result<()> {
        if messages.is_empty() {
            error!(target: "stdout", "Found empty messages in the chat request.");

            return Err(ChatPromptsError::PromptError::NoMessages);
        }

        if context.is_empty() {
            let err_msg = "No context provided.";

            // log
            error!(target: "stdout", "{}", &err_msg);

            return Err(ChatPromptsError::PromptError::Operation(err_msg.into()));
        }

        // check rag policy
        let mut policy = policy;
        if policy == MergeRagContextPolicy::SystemMessage && !has_system_prompt {
            // log
            info!(target: "stdout", "The chat model does not support system message. Switch the currect rag policy to `last-user-message`");

            policy = MergeRagContextPolicy::LastUserMessage;
        }
        info!(target: "stdout", "rag_policy: {}", &policy);

        let context = context[0].trim_end();
        info!(target: "stdout", "context:\n{}", context);

        match policy {
            MergeRagContextPolicy::SystemMessage => {
                match &messages[0] {
                    ChatCompletionRequestMessage::System(message) => {
                        let system_message = {
                            match rag_prompt {
                                Some(global_rag_prompt) => {
                                    // compose new system message content
                                    let content = format!(
                                        "{system_message}\n{rag_prompt}\n{context}",
                                        system_message = message.content().trim(),
                                        rag_prompt = global_rag_prompt.to_owned(),
                                        context = context
                                    );

                                    // create system message
                                    ChatCompletionRequestMessage::new_system_message(
                                        content,
                                        message.name().cloned(),
                                    )
                                }
                                None => {
                                    // compose new system message content
                                    let content = format!(
                                        "{system_message}\n{context}",
                                        system_message = message.content().trim(),
                                        context = context
                                    );

                                    // create system message
                                    ChatCompletionRequestMessage::new_system_message(
                                        content,
                                        message.name().cloned(),
                                    )
                                }
                            }
                        };

                        // replace the original system message
                        messages[0] = system_message;
                    }
                    _ => {
                        let system_message = match rag_prompt {
                            Some(global_rag_prompt) => {
                                // compose new system message content
                                let content = format!(
                                    "{rag_prompt}\n{context}",
                                    rag_prompt = global_rag_prompt.to_owned(),
                                    context = context
                                );

                                // create system message
                                ChatCompletionRequestMessage::new_system_message(content, None)
                            }
                            None => {
                                // create system message
                                ChatCompletionRequestMessage::new_system_message(
                                    context.to_string(),
                                    None,
                                )
                            }
                        };

                        // insert system message
                        messages.insert(0, system_message);
                    }
                }
            }
            MergeRagContextPolicy::LastUserMessage => {
                info!(target: "stdout", "Merge RAG context into last user message.");

                let len = messages.len();
                match &messages.last() {
                    Some(ChatCompletionRequestMessage::User(message)) => {
                        if let ChatCompletionUserMessageContent::Text(content) = message.content() {
                            // compose new user message content
                            let content = format!(
                                    "{context}\nAnswer the question based on the pieces of context above. The question is:\n{user_message}",
                                    context = context,
                                    user_message = content.trim(),
                                );

                            let content = ChatCompletionUserMessageContent::Text(content);

                            // create user message
                            let user_message = ChatCompletionRequestMessage::new_user_message(
                                content,
                                message.name().cloned(),
                            );
                            // replace the original user message
                            messages[len - 1] = user_message;
                        }
                    }
                    _ => {
                        let err_msg =
                            "The last message in the chat request should be a user message.";

                        // log
                        error!(target: "stdout", "{}", &err_msg);

                        return Err(ChatPromptsError::PromptError::BadMessages(err_msg.into()));
                    }
                }
            }
        }

        Ok(())
    }
}

// Segment the given text into chunks
pub(crate) fn chunk_text(
    text: impl AsRef<str>,
    ty: impl AsRef<str>,
    chunk_capacity: usize,
    request_id: impl AsRef<str>,
) -> Result<Vec<String>, ServerError> {
    let request_id = request_id.as_ref();

    if ty.as_ref().to_lowercase().as_str() != "txt" && ty.as_ref().to_lowercase().as_str() != "md" {
        let err_msg = "Failed to upload the target file. Only files with 'txt' and 'md' extensions are supported.";

        error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

        return Err(ServerError::Operation(err_msg.into()));
    }

    match ty.as_ref().to_lowercase().as_str() {
        "txt" => {
            info!(target: "stdout", "Chunk the plain text contents - request_id: {}", request_id);

            // create a text splitter
            let splitter = TextSplitter::new(chunk_capacity);

            let chunks = splitter
                .chunks(text.as_ref())
                .map(|s| s.to_string())
                .collect::<Vec<_>>();

            info!(target: "stdout", "{} chunks - request_id: {}", chunks.len(), request_id);

            Ok(chunks)
        }
        "md" => {
            info!(target: "stdout", "Chunk the markdown contents - request_id: {}", request_id);

            // create a markdown splitter
            let splitter = MarkdownSplitter::new(chunk_capacity);

            let chunks = splitter
                .chunks(text.as_ref())
                .map(|s| s.to_string())
                .collect::<Vec<_>>();

            info!(target: "stdout", "Number of chunks: {} - request_id: {}", chunks.len(), request_id);

            Ok(chunks)
        }
        _ => {
            let err_msg =
                "Failed to upload the target file. Only text and markdown files are supported.";

            error!(target: "stdout", "{}", err_msg);

            Err(ServerError::Operation(err_msg.into()))
        }
    }
}

pub(crate) async fn qdrant_create_collection(
    qdrant_client: &qdrant::Qdrant,
    collection_name: impl AsRef<str>,
    dim: usize,
    request_id: impl AsRef<str>,
) -> Result<(), ServerError> {
    let request_id = request_id.as_ref();

    info!(target: "stdout", "Create a collection `{}` of {} dimensions - request_id: {}", collection_name.as_ref(), dim, request_id);

    if let Err(e) = qdrant_client
        .create_collection(collection_name.as_ref(), dim as u32)
        .await
    {
        let err_msg = e.to_string();

        error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

        return Err(ServerError::Operation(err_msg));
    }

    Ok(())
}

pub(crate) async fn qdrant_persist_embeddings(
    qdrant_client: &qdrant::Qdrant,
    collection_name: impl AsRef<str>,
    embeddings: &[EmbeddingObject],
    chunks: &[String],
    request_id: impl AsRef<str>,
) -> Result<(), ServerError> {
    let request_id = request_id.as_ref();

    info!(target: "stdout", "Persist embeddings to the Qdrant instance - request_id: {}", request_id);

    let mut points = Vec::<Point>::new();
    for embedding in embeddings {
        // convert the embedding to a vector
        let vector: Vec<_> = embedding.embedding.iter().map(|x| *x as f32).collect();

        // create a payload
        let payload = serde_json::json!({"source": chunks[embedding.index as usize]})
            .as_object()
            .map(|m| m.to_owned());

        // create a point
        let p = Point {
            id: PointId::Num(embedding.index),
            vector,
            payload,
        };

        points.push(p);
    }

    info!(target: "stdout", "{} points to be upserted - request_id: {}", points.len(), request_id);

    if let Err(e) = qdrant_client
        .upsert_points(collection_name.as_ref(), points)
        .await
    {
        let err_msg = format!("{}", e);

        error!(target: "stdout", "{} - request_id: {}", err_msg, request_id);

        return Err(ServerError::Operation(err_msg));
    }

    Ok(())
}
