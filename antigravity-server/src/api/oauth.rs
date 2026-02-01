//! OAuth Flow Handlers
//!
//! Headless OAuth flow for Google account authorization.

use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

use antigravity_core::modules::{account, oauth};
use antigravity_types::models::TokenData;

use crate::state::AppState;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Get OAuth redirect URI based on port and optional host override.
pub fn get_oauth_redirect_uri_with_port(port: u16) -> String {
    if let Ok(host) = std::env::var("ANTIGRAVITY_OAUTH_HOST") {
        format!("{}/api/oauth/callback", host)
    } else {
        format!("http://127.0.0.1:{}/api/oauth/callback", port)
    }
}

// ============ Response Types ============

#[derive(Serialize)]
pub struct OAuthUrlResponse {
    pub url: String,
    pub redirect_uri: String,
    pub state: String,
}

#[derive(Serialize)]
pub struct OAuthLoginResponse {
    pub url: String,
    pub message: String,
    pub state: String,
}

#[derive(Serialize)]
pub struct SubmitCodeResponse {
    pub success: bool,
    pub message: String,
    pub email: Option<String>,
}

// ============ Request Types ============

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
    pub state: Option<String>,
}

#[derive(Deserialize)]
pub struct SubmitCodeRequest {
    pub code: String,
    pub state: String,
}

// ============ Handlers ============

pub async fn get_oauth_url(State(state): State<AppState>) -> Json<OAuthUrlResponse> {
    let port = state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);
    let oauth_state = state.generate_oauth_state();
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state);

    Json(OAuthUrlResponse {
        url,
        redirect_uri,
        state: oauth_state,
    })
}

pub async fn start_oauth_login(State(state): State<AppState>) -> Json<OAuthLoginResponse> {
    let port = state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);
    let oauth_state = state.generate_oauth_state();
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state);

    Json(OAuthLoginResponse {
        url,
        message: "Open this URL in your browser to authorize".to_string(),
        state: oauth_state,
    })
}

pub async fn handle_oauth_callback(
    State(app_state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
    if let Some(s) = &query.state {
        if !app_state.validate_oauth_state(s) {
            return Html(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Invalid State Token</h1>
                    <p>CSRF validation failed. Please try again.</p>
                </body>
                </html>"#
                    .to_string(),
            )
            .into_response();
        }
    } else {
        return Html(
            r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: red;">❌ Missing State Token</h1>
                <p>No state parameter received. Please try again.</p>
            </body>
            </html>"#
                .to_string(),
        )
        .into_response();
    }

    if let Some(error) = query.error {
        return Html(format!(
            r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: red;">❌ Authorization Failed</h1>
                <p>Error: {}</p>
                <p>Please close this window and try again.</p>
            </body>
            </html>"#,
            escape_html(&error)
        ))
        .into_response();
    }

    let Some(code) = query.code else {
        return Html(
            r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: red;">❌ Missing Authorization Code</h1>
                <p>No authorization code received.</p>
            </body>
            </html>"#
                .to_string(),
        )
        .into_response();
    };

    let port = app_state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    let token_res = match oauth::exchange_code(&code, &redirect_uri).await {
        Ok(t) => t,
        Err(e) => {
            return Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Token Exchange Failed</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
                escape_html(&e)
            ))
            .into_response();
        }
    };

    let Some(refresh_token) = token_res.refresh_token else {
        return Html(r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: orange;">⚠️ No Refresh Token</h1>
                <p>Google didn't return a refresh token.</p>
                <p>This usually happens if you've authorized this app before.</p>
                <p><strong>Solution:</strong></p>
                <ol style="text-align: left; display: inline-block;">
                    <li>Go to <a href="https://myaccount.google.com/permissions" target="_blank">Google Account Permissions</a></li>
                    <li>Find and revoke "Antigravity Tools"</li>
                    <li>Try authorization again</li>
                </ol>
            </body>
            </html>"#.to_string()).into_response();
    };

    let user_info = match oauth::get_user_info(&token_res.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Failed to Get User Info</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
                escape_html(&e)
            ))
            .into_response();
        }
    };

    let token_data = TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );

    match account::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    ) {
        Ok(acc) => {
            let _ = app_state.reload_accounts().await;

            Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <meta charset="utf-8">
                    <title>Authorization Successful</title>
                </head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: green;">✅ Authorization Successful!</h1>
                    <p>Account added: <strong>{}</strong></p>
                    <p>You can close this window and return to the app.</p>
                    <script>setTimeout(function() {{ window.close(); }}, 3000);</script>
                </body>
                </html>"#,
                escape_html(&acc.email)
            ))
            .into_response()
        }
        Err(e) => Html(format!(
            r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Failed to Save Account</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
            escape_html(&e)
        ))
        .into_response(),
    }
}

pub async fn submit_oauth_code(
    State(app_state): State<AppState>,
    Json(payload): Json<SubmitCodeRequest>,
) -> Result<Json<SubmitCodeResponse>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    if !app_state.validate_oauth_state(&payload.state) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid or expired OAuth state".to_string(),
        ));
    }

    let port = app_state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    let token_res = oauth::exchange_code(&payload.code, &redirect_uri)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to exchange code: {}", e),
            )
        })?;

    let user_info = oauth::get_user_info(&token_res.access_token)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get user info: {}", e),
            )
        })?;

    let token_data = TokenData::new(
        token_res.access_token,
        token_res.refresh_token.ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "No refresh token in response".to_string(),
            )
        })?,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );

    account::upsert_account(user_info.email.clone(), user_info.name.clone(), token_data).map_err(
        |e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to add account: {}", e),
            )
        },
    )?;

    let _ = app_state.reload_accounts().await;

    Ok(Json(SubmitCodeResponse {
        success: true,
        message: format!("Account {} added successfully", user_info.email),
        email: Some(user_info.email),
    }))
}
