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

fn error_page(title: &str, message: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>OAuth Error</title></head>
<body style="font-family:sans-serif;text-align:center;padding:50px">
<h1 style="color:red">{}</h1><p>{}</p></body></html>"#,
        title, message
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Get OAuth redirect URI based on port and optional host override.
pub fn get_oauth_redirect_uri_with_port(port: u16) -> String {
    std::env::var("ANTIGRAVITY_OAUTH_HOST").map_or_else(
        |_| format!("http://127.0.0.1:{port}/api/oauth/callback"),
        |host| format!("{host}/api/oauth/callback"),
    )
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

#[derive(Deserialize)]
pub struct OAuthLoginRequest {
    /// Optional proxy URL to assign to the account being authorized.
    /// When set, ALL requests during OAuth (code exchange, user info) go through this proxy.
    pub proxy_url: Option<String>,
}

// ============ Handlers ============

pub async fn get_oauth_url(State(state): State<AppState>) -> Json<OAuthUrlResponse> {
    let port = state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);
    let oauth_state = state.generate_oauth_state(None);
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state)
        .unwrap_or_else(|e| format!("Error: {e}"));

    Json(OAuthUrlResponse { url, redirect_uri, state: oauth_state })
}

pub async fn start_oauth_login(
    State(state): State<AppState>,
    Json(payload): Json<OAuthLoginRequest>,
) -> Json<OAuthLoginResponse> {
    let port = state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    // Resolve proxy_url: explicit from request > auto-assigned from pool > None
    let proxy_url = if payload.proxy_url.is_some() {
        payload.proxy_url
    } else {
        let pool = &state.inner.proxy_config.read().await.account_proxy_pool;
        antigravity_core::modules::proxy_pool::assign_proxy_from_pool(pool, None)
    };

    if let Some(ref purl) = proxy_url {
        tracing::info!("OAuth login: auto-assigned proxy {}", purl);
    }

    let oauth_state = state.generate_oauth_state(proxy_url);
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state)
        .unwrap_or_else(|e| format!("Error: {e}"));

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
    // Validate state and extract proxy_url
    let proxy_url = if let Some(s) = &query.state {
        match app_state.validate_oauth_state(s) {
            Some(proxy_url) => proxy_url,
            None => {
                return Html(error_page(
                    "❌ Invalid State Token",
                    "CSRF validation failed. Please try again.",
                ))
                .into_response();
            },
        }
    } else {
        return Html(error_page(
            "❌ Missing State Token",
            "No state parameter received. Please try again.",
        ))
        .into_response();
    };

    if let Some(error) = query.error {
        return Html(error_page(
            "❌ Authorization Failed",
            &format!("Error: {}", escape_html(&error)),
        ))
        .into_response();
    }

    let Some(code) = query.code else {
        return Html(error_page(
            "❌ Missing Authorization Code",
            "No authorization code received.",
        ))
        .into_response();
    };

    let port = app_state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    // Use per-account proxy for code exchange to prevent IP leak
    let token_res =
        match oauth::exchange_code_with_proxy(&code, &redirect_uri, proxy_url.as_deref()).await {
            Ok(t) => t,
            Err(e) => {
                return Html(error_page(
                    "❌ Token Exchange Failed",
                    &format!("Error: {}", escape_html(&e)),
                ))
                .into_response()
            },
        };

    let Some(refresh_token) = token_res.refresh_token else {
        return Html(
            r#"<!DOCTYPE html>
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
            </html>"#
                .to_string(),
        )
        .into_response();
    };

    // Use per-account proxy for user info lookup
    let user_info = match oauth::get_user_info_with_proxy(
        &token_res.access_token,
        proxy_url.as_deref(),
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            return Html(error_page(
                "❌ Failed to Get User Info",
                &format!("Error: {}", escape_html(&e)),
            ))
            .into_response()
        },
    };

    let token_data = TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );

    let upsert_email = user_info.email.clone();
    let upsert_name = user_info.get_display_name();
    let upsert_token = token_data.clone();
    match tokio::task::spawn_blocking(move || {
        account::upsert_account(upsert_email, upsert_name, upsert_token)
    })
    .await
    {
        Ok(Ok(mut acc)) => {
            if let Some(repo) = app_state.repository() {
                if let Err(e) = repo
                    .upsert_account(
                        user_info.email.clone(),
                        user_info.get_display_name(),
                        token_data,
                    )
                    .await
                {
                    tracing::warn!("Failed to upsert account to DB: {}", e);
                }
                if let Some(ref purl) = proxy_url {
                    if let Err(e) = repo.update_proxy_url(&acc.id, Some(purl.as_str())).await {
                        tracing::warn!(
                            "Failed to persist proxy_url to DB for {}: {}",
                            acc.email,
                            e
                        );
                    }
                }
            }
            // Persist proxy_url to JSON file so both storage paths are covered
            if let Some(ref purl) = proxy_url {
                acc.proxy_url = Some(purl.clone());
                let acc_clone = acc.clone();
                if let Err(e) =
                    tokio::task::spawn_blocking(move || account::save_account(&acc_clone))
                        .await
                        .unwrap_or_else(|e| Err(format!("spawn_blocking panicked: {e}")))
                {
                    tracing::warn!("Failed to persist proxy_url to JSON for {}: {}", acc.email, e);
                }
            }
            if let Err(e) = app_state.reload_accounts().await {
                tracing::warn!("Failed to reload accounts after OAuth callback: {}", e);
            }

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
        },
        Ok(Err(e)) => {
            Html(error_page("❌ Failed to Save Account", &format!("Error: {}", escape_html(&e))))
                .into_response()
        },
        Err(e) => Html(error_page(
            "❌ Failed to Save Account",
            &format!("Error: {}", escape_html(&e.to_string())),
        ))
        .into_response(),
    }
}

pub async fn submit_oauth_code(
    State(app_state): State<AppState>,
    Json(payload): Json<SubmitCodeRequest>,
) -> Result<Json<SubmitCodeResponse>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    let proxy_url = match app_state.validate_oauth_state(&payload.state) {
        Some(proxy_url) => proxy_url,
        None => {
            return Err((StatusCode::BAD_REQUEST, "Invalid or expired OAuth state".to_string()))
        },
    };

    let port = app_state.get_bound_port();
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    // Use per-account proxy for code exchange
    let token_res =
        oauth::exchange_code_with_proxy(&payload.code, &redirect_uri, proxy_url.as_deref())
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to exchange code: {}", e)))?;

    // Use per-account proxy for user info
    let user_info = oauth::get_user_info_with_proxy(&token_res.access_token, proxy_url.as_deref())
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get user info: {}", e))
        })?;

    let token_data = TokenData::new(
        token_res.access_token,
        token_res
            .refresh_token
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "No refresh token in response".to_string()))?,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        None,
    );

    let upsert_email = user_info.email.clone();
    let upsert_name = user_info.name.clone();
    let upsert_token = token_data.clone();
    let acc = tokio::task::spawn_blocking(move || {
        account::upsert_account(upsert_email, upsert_name, upsert_token)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}")))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add account: {}", e)))?;

    if let Some(repo) = app_state.repository() {
        if let Err(e) =
            repo.upsert_account(user_info.email.clone(), user_info.name.clone(), token_data).await
        {
            tracing::warn!("Failed to upsert account to DB: {}", e);
        }
        if let Some(ref purl) = proxy_url {
            if let Err(e) = repo.update_proxy_url(&acc.id, Some(purl.as_str())).await {
                tracing::warn!("Failed to persist proxy_url to DB for {}: {}", acc.email, e);
            }
        }
    }
    // Persist proxy_url to JSON file so both storage paths are covered
    if let Some(ref purl) = proxy_url {
        let mut acc_with_proxy = acc;
        acc_with_proxy.proxy_url = Some(purl.clone());
        if let Err(e) = tokio::task::spawn_blocking(move || account::save_account(&acc_with_proxy))
            .await
            .unwrap_or_else(|e| Err(format!("spawn_blocking panicked: {e}")))
        {
            tracing::warn!("Failed to persist proxy_url to JSON: {}", e);
        }
    }

    if let Err(e) = app_state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after OAuth code submit: {}", e);
    }

    Ok(Json(SubmitCodeResponse {
        success: true,
        message: format!("Account {} added successfully", user_info.email),
        email: Some(user_info.email),
    }))
}
