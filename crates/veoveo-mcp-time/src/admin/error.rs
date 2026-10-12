use axum::{
    http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};

use crate::contract::{AdminProblem, AdminProblemType, TimeCorrelationId};

/// RFC 9457 media type for problem details.
const PROBLEM_JSON: &str = "application/problem+json";

#[derive(Debug)]
pub(super) struct ApiError {
    status: StatusCode,
    body: AdminProblem,
}

impl ApiError {
    pub fn bad_request(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            AdminProblemType::InvalidRequest,
            message.to_string(),
            false,
        )
    }
    pub fn not_found(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            AdminProblemType::NotFound,
            message.to_string(),
            false,
        )
    }
    pub fn conflict(message: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            AdminProblemType::VersionConflict,
            message.to_string(),
            false,
        )
    }
    pub fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!("Time administrative operation failed: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            AdminProblemType::InternalError,
            "Time administrative operation failed",
            true,
        )
    }
    fn new(
        status: StatusCode,
        problem_type: AdminProblemType,
        detail: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            status,
            body: AdminProblem {
                problem_type,
                title: problem_type.title().to_owned(),
                status: status.as_u16(),
                detail: detail.into(),
                retryable,
                correlation_id: TimeCorrelationId::new(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&self.body).expect("problem details serialize");
        let mut response = (self.status, body).into_response();
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        response
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("unknown") || message.contains("disappeared") {
            Self::not_found(message)
        } else if message.contains("conflict") || message.contains("record_version") {
            Self::conflict(message)
        } else if message.contains("invalid")
            || message.contains("must")
            || message.contains("disabled")
            || message.contains("digest")
        {
            Self::bad_request(message)
        } else {
            Self::internal(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn errors_are_problem_details() {
        let response = ApiError::not_found("Missing source.").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[CONTENT_TYPE], PROBLEM_JSON);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["type"], "https://veoveo.ai/problems/not-found");
        assert_eq!(problem["title"], "Not found");
        assert_eq!(problem["status"], 404);
        assert_eq!(problem["detail"], "Missing source.");
        assert_eq!(problem["retryable"], false);
        assert!(problem["correlationId"].is_string());
    }
}
