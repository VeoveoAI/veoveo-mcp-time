mod error;
mod handlers;

use std::sync::Arc;

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{mcp::requirements, state::TimeApplication};

/// Authority, calendar, epoch and clock-policy administration under
/// `{mount}/admin`. The shared host serves the document routes beside these. Every
/// route requires Time's administrative permission.
pub fn router(state: Arc<TimeApplication>) -> Router {
    Router::new()
        .route(
            "/sources",
            get(handlers::list_sources).post(handlers::create_source),
        )
        .route(
            "/sources/{source_id}",
            get(handlers::get_source).put(handlers::replace_source),
        )
        .route(
            "/acquisitions",
            get(handlers::list_acquisitions).post(handlers::create_acquisition),
        )
        .route(
            "/acquisitions/{acquisition_id}",
            get(handlers::get_acquisition),
        )
        .route(
            "/acquisitions/{acquisition_id}/cancel",
            post(handlers::cancel_acquisition),
        )
        .route("/releases", get(handlers::list_releases))
        .route("/releases/{release_id}", get(handlers::get_release))
        .route(
            "/releases/{release_id}/activate",
            post(handlers::activate_release),
        )
        .route(
            "/active-authorities",
            get(handlers::list_active_authorities),
        )
        .route(
            "/calendars",
            get(handlers::list_calendars).post(handlers::create_calendar),
        )
        .route(
            "/calendars/{calendar_id}/versions/{version}",
            get(handlers::get_calendar),
        )
        .route(
            "/epochs",
            get(handlers::list_epochs).post(handlers::create_epoch),
        )
        .route("/epochs/{epoch_id}", get(handlers::get_epoch))
        .route(
            "/clock-policy",
            get(handlers::get_clock_policy).put(handlers::replace_clock_policy),
        )
        .with_state(state)
        .layer(middleware::from_fn(authorize))
}

/// Admits an administrative request only when the Gateway granted Time's
/// administrative permission for it.
async fn authorize(request: Request, next: Next) -> axum::response::Response {
    let allowed = request
        .extensions()
        .get::<GatewayInternalIdentity>()
        .is_some_and(requirements::administers);
    if !allowed {
        return error::ApiError::forbidden(format!(
            "Missing permission `{}`.",
            requirements::ADMINISTRATION.name()
        ))
        .into_response();
    }
    next.run(request).await
}
