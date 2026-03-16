use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;

use crate::models::{Session, User};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    pub token: String,
}

pub async fn api_auth(
    State(pool): State<PgPool>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let session = Session::find_by_token(&pool, &token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let user = User::find_by_id(&pool, session.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.status == 0 {
        return Err(StatusCode::FORBIDDEN);
    }

    req.extensions_mut().insert(AuthUser {
        user,
        token,
    });

    Ok(next.run(req).await)
}

pub async fn console_auth(
    State(pool): State<PgPool>,
    mut req: Request,
    next: Next,
) -> Response {
    let redirect = || axum::response::Redirect::to("/console/login").into_response();

    let cookie_header = req
        .headers()
        .get("Cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let session_id = match parse_cookie(cookie_header, "session") {
        Some(id) => id,
        None => return redirect(),
    };

    let session = match Session::find_by_token(&pool, &session_id).await {
        Ok(Some(s)) => s,
        _ => return redirect(),
    };

    let user = match User::find_by_id(&pool, session.user_id).await {
        Ok(Some(u)) if u.status != 0 => u,
        _ => return redirect(),
    };

    req.extensions_mut().insert(AuthUser {
        user,
        token: session_id,
    });

    next.run(req).await
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header
        .split(';')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, '=');
            let key = parts.next()?.trim();
            let val = parts.next()?.trim();
            if key == name { Some(val.to_string()) } else { None }
        })
        .next()
}
