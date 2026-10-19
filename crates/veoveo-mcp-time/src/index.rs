//! Operational translation of collection query failures.
#[cfg(feature = "mcp")]
use rmcp::ErrorData as McpError;
#[cfg(feature = "mcp")]
pub(crate) fn query_error(error: anyhow::Error) -> McpError {
    if matches!(
        error.downcast_ref::<veoveo_types::PagingError>(),
        Some(veoveo_types::PagingError::Cursor(_) | veoveo_types::PagingError::Limit)
    ) || matches!(
        error.downcast_ref::<crate::persistence::PersistenceError>(),
        Some(crate::persistence::PersistenceError::InvalidTimeField { .. })
    ) {
        McpError::invalid_params("invalid Time collection cursor", None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}
