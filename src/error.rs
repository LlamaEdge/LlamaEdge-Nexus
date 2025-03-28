use crate::dual_error;
use axum::{http::StatusCode, response::IntoResponse, Json};
use hyper::{Body, Response};
use thiserror::Error;

#[allow(dead_code)]
pub(crate) fn not_implemented() -> Response<Body> {
    // log error
    dual_error!("501 Not Implemented");

    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "*")
        .header("Access-Control-Allow-Headers", "*")
        .status(hyper::StatusCode::NOT_IMPLEMENTED)
        .body(Body::from("501 Not Implemented"))
        .unwrap()
}

#[allow(dead_code)]
pub(crate) fn internal_server_error(msg: impl AsRef<str>) -> Response<Body> {
    let err_msg = match msg.as_ref().is_empty() {
        true => "500 Internal Server Error".to_string(),
        false => format!("500 Internal Server Error: {}", msg.as_ref()),
    };

    // log error
    dual_error!("{}", &err_msg);

    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "*")
        .header("Access-Control-Allow-Headers", "*")
        .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(err_msg))
        .unwrap()
}

#[allow(dead_code)]
pub(crate) fn bad_request(msg: impl AsRef<str>) -> Response<Body> {
    let err_msg = match msg.as_ref().is_empty() {
        true => "400 Bad Request".to_string(),
        false => format!("400 Bad Request: {}", msg.as_ref()),
    };

    // log error
    dual_error!("{}", &err_msg);

    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "*")
        .header("Access-Control-Allow-Headers", "*")
        .status(hyper::StatusCode::BAD_REQUEST)
        .body(Body::from(err_msg))
        .unwrap()
}

#[allow(dead_code)]
pub(crate) fn invalid_endpoint(msg: impl AsRef<str>) -> Response<Body> {
    let err_msg = match msg.as_ref().is_empty() {
        true => "404 The requested service endpoint is not found".to_string(),
        false => format!(
            "404 The requested service endpoint is not found: {}",
            msg.as_ref()
        ),
    };

    // log error
    dual_error!("{}", &err_msg);

    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "*")
        .header("Access-Control-Allow-Headers", "*")
        .status(hyper::StatusCode::NOT_FOUND)
        .body(Body::from(err_msg))
        .unwrap()
}

#[allow(dead_code)]
#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    #[error("Not found available server. Please register a server via the `/admin/register/{0}` endpoint.")]
    NotFoundServer(String),
    /// Error returned while parsing socket address failed
    #[error("Failed to parse socket address: {0}")]
    SocketAddr(String),
    /// Error returned while parsing CLI options failed
    #[error("{0}")]
    ArgumentError(String),
    /// Generic error returned while performing an operation
    #[error("{0}")]
    Operation(String),
    #[error("Invalid server kind: {0}")]
    InvalidServerKind(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Failed to load config: {0}")]
    FailedToLoadConfig(String),
}
impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        let (status, err_response) = match &self {
            ServerError::SocketAddr(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ServerError::ArgumentError(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ServerError::Operation(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ServerError::NotFoundServer(e) => (StatusCode::NOT_FOUND, e.to_string()),
            ServerError::InvalidServerKind(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ServerError::BadRequest(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ServerError::FailedToLoadConfig(e) => (StatusCode::BAD_REQUEST, e.to_string()),
        };

        (status, Json(err_response)).into_response()
    }
}

pub type ServerResult<T> = std::result::Result<T, ServerError>;
