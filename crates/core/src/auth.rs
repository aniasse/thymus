use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use crate::api::CoreState;

const SESSION_COOKIE: &str = "thymus_session";

pub fn extract_token(req: &Request<Body>) -> Option<String> {
    // Try Authorization: Bearer <token>
    if let Some(auth) = req.headers().get(header::AUTHORIZATION)
        && let Ok(value) = auth.to_str()
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }

    // Try session cookie
    if let Some(cookie_header) = req.headers().get(header::COOKIE)
        && let Ok(cookies) = cookie_header.to_str()
    {
        let prefix = format!("{SESSION_COOKIE}=");
        for cookie in cookies.split(';') {
            if let Some(token) = cookie.trim().strip_prefix(&prefix) {
                return Some(token.to_string());
            }
        }
    }

    None
}

fn is_authenticated(state: &CoreState, req: &Request<Body>) -> bool {
    let Some(ref expected) = state.token else {
        return true; // No token configured = open access
    };

    extract_token(req).is_some_and(|t| &t == expected)
}

pub async fn require_auth(
    State(state): State<CoreState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Always-open endpoints
    if path == "/login"
        || path == "/api/health"
        || path == "/api/login"
        || path.starts_with("/static")
    {
        return next.run(req).await;
    }

    if is_authenticated(&state, &req) {
        return next.run(req).await;
    }

    // API paths → 401, browser page navs → redirect to login
    if path.starts_with("/api/") {
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

pub fn session_cookie(token: &str) -> String {
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400")
}
