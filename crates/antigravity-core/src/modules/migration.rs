use crate::{
    models::{Account, TokenData},
    modules::{account, vscode},
    utils::protobuf,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use std::{fs, path::PathBuf};

/// Scan and import V1 data
pub async fn import_from_v1() -> Result<Vec<Account>, String> {
    use crate::modules::oauth;
    let home = dirs::home_dir().ok_or("Failed to get home directory")?;
    // V1 data directory (confirmed unified across platforms per utils.py)
    let v1_dir = home.join(".antigravity-agent");
    let mut imported_accounts = Vec::new();
    // Try multiple possible filenames
    let index_files = vec![
        "antigravity_accounts.json", // Directly use string literal
        "accounts.json",
    ];
    let mut found_index = false;
    for index_filename in index_files {
        let v1_accounts_path = v1_dir.join(index_filename);
        if !v1_accounts_path.exists() {
            continue;
        }
        found_index = true;
        crate::modules::logger::log_info(&format!("Found V1 data: {:?}", v1_accounts_path));
        let content = match fs::read_to_string(&v1_accounts_path) {
            Ok(c) => c,
            Err(e) => {
                crate::modules::logger::log_warn(&format!("Failed to read index: {}", e));
                continue;
            }
        };

        let v1_index: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                crate::modules::logger::log_warn(&format!("Failed to parse index JSON: {}", e));
                continue;
            }
        };

        // Compatible with two formats: direct map, or containing \"accounts\" field
        let accounts_map = if let Some(map) = v1_index.as_object() {
            if let Some(accounts) = map.get("accounts").and_then(|v| v.as_object()) {
                accounts
            } else {
                map
            }
        } else {
            continue;
        };
        for (id, acc_info) in accounts_map {
            let email_placeholder = acc_info
                .get("email")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            // Skip non-account keys (e.g., "current_account_id")
            if !acc_info.is_object() {
                continue;
            }

            let backup_file_str = acc_info.get("backup_file").and_then(|v| v.as_str());
            let data_file_str = acc_info.get("data_file").and_then(|v| v.as_str());

            // Prefer backup_file, fallback to data_file
            let target_file = backup_file_str.or(data_file_str);

            if target_file.is_none() {
                crate::modules::logger::log_warn(&format!(
                    "Account {} ({}) missing data file path",
                    id, email_placeholder
                ));
                continue;
            }

            // target_file is guaranteed to be Some here (checked above)
            let mut backup_path = PathBuf::from(target_file.expect("checked is_none above"));

            // If relative path, try to join
            if !backup_path.exists() {
                backup_path = v1_dir.join(backup_path.file_name().unwrap_or_default());
            }

            // Try joining data/ or backups/ subdirectories
            if !backup_path.exists() {
                let file_name = backup_path.file_name().unwrap_or_default();
                let try_backups = v1_dir.join("backups").join(file_name);
                if try_backups.exists() {
                    backup_path = try_backups;
                } else {
                    let try_accounts = v1_dir.join("accounts").join(file_name);
                    if try_accounts.exists() {
                        backup_path = try_accounts;
                    }
                }
            }

            if !backup_path.exists() {
                crate::modules::logger::log_warn(&format!(
                    "Account {} ({}) backup file does not exist: {:?}",
                    id, email_placeholder, backup_path
                ));
                continue;
            }

            // Read backup file
            if let Ok(backup_content) = fs::read_to_string(&backup_path) {
                if let Ok(backup_json) = serde_json::from_str::<Value>(&backup_content) {
                    // Compatible with two formats:
                    // 1. V1 backup: jetskiStateSync.agentManagerInitState -> Protobuf
                    // 2. V2/Script data: JSON containing "token" field

                    let mut refresh_token_opt = None;

                    // Try format 2
                    if let Some(token_data) = backup_json.get("token") {
                        if let Some(rt) = token_data.get("refresh_token").and_then(|v| v.as_str()) {
                            refresh_token_opt = Some(rt.to_string());
                        }
                    }

                    // Try format 1
                    if refresh_token_opt.is_none() {
                        if let Some(state_b64) = backup_json
                            .get("jetskiStateSync.agentManagerInitState")
                            .and_then(|v| v.as_str())
                        {
                            // Parse Protobuf
                            if let Ok(blob) = general_purpose::STANDARD.decode(state_b64) {
                                if let Ok(Some(oauth_data)) = protobuf::find_field(&blob, 6) {
                                    if let Ok(Some(refresh_bytes)) =
                                        protobuf::find_field(&oauth_data, 3)
                                    {
                                        if let Ok(rt) = String::from_utf8(refresh_bytes) {
                                            refresh_token_opt = Some(rt);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(refresh_token) = refresh_token_opt {
                        crate::modules::logger::log_info(&format!(
                            "Importing account: {}",
                            email_placeholder
                        ));

                        let (email, access_token, expires_in) =
                            match oauth::refresh_access_token(&refresh_token).await {
                                Ok(token_resp) => {
                                    match oauth::get_user_info(&token_resp.access_token).await {
                                        Ok(user_info) => (
                                            user_info.email,
                                            token_resp.access_token,
                                            token_resp.expires_in,
                                        ),
                                        Err(_) => (
                                            email_placeholder.clone(),
                                            token_resp.access_token,
                                            token_resp.expires_in,
                                        ),
                                    }
                                }
                                Err(e) => {
                                    crate::modules::logger::log_warn(&format!(
                                        "Token refresh failed (may be expired): {}",
                                        e
                                    ));
                                    (
                                        email_placeholder.clone(),
                                        "imported_access_token".to_string(),
                                        0,
                                    )
                                }
                            };

                        let token_data = TokenData::new(
                            access_token,
                            refresh_token,
                            expires_in,
                            Some(email.clone()),
                            None, // project_id will be fetched when needed
                            None, // session_id
                        );

                        // get_user_info already fetched name at line 153, but we're outside the match, using None for safety
                        match account::upsert_account(email.clone(), None, token_data) {
                            Ok(acc) => {
                                crate::modules::logger::log_info(&format!(
                                    "Import successful: {}",
                                    email
                                ));
                                imported_accounts.push(acc);
                            }
                            Err(e) => crate::modules::logger::log_error(&format!(
                                "Import save failed {}: {}",
                                email, e
                            )),
                        }
                    } else {
                        crate::modules::logger::log_warn(&format!(
                            "Account {} data file does not contain Refresh Token",
                            email_placeholder
                        ));
                    }
                }
            }
        }
    }

    if !found_index {
        return Err("V1 account data file not found".to_string());
    }

    Ok(imported_accounts)
}

/// Import account from custom database path
pub async fn import_from_custom_db_path(path_str: String) -> Result<Account, String> {
    use crate::modules::oauth;

    let path = PathBuf::from(path_str);
    if !path.exists() {
        return Err(format!("File does not exist: {:?}", path));
    }

    let refresh_token = extract_refresh_token_from_file(&path)?;

    // 3. Use Refresh Token to get latest Access Token and user info
    crate::modules::logger::log_info("Using Refresh Token to get user info...");
    let token_resp = oauth::refresh_access_token(&refresh_token).await?;
    let user_info = oauth::get_user_info(&token_resp.access_token).await?;

    let email = user_info.email;

    crate::modules::logger::log_info(&format!("Successfully retrieved account info: {}", email));

    let token_data = TokenData::new(
        token_resp.access_token,
        refresh_token,
        token_resp.expires_in,
        Some(email.clone()),
        None, // project_id will be fetched when needed
        None, // session_id will be generated in token_manager
    );

    // 4. Add or update account
    account::upsert_account(email.clone(), user_info.name, token_data)
}

/// Import current logged-in account from default IDE database
pub async fn import_from_db() -> Result<Account, String> {
    let db_path = vscode::get_vscode_db_path()?;
    import_from_custom_db_path(db_path.to_string_lossy().to_string()).await
}

/// Extract Refresh Token from database (common logic)
pub fn extract_refresh_token_from_file(db_path: &PathBuf) -> Result<String, String> {
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    // Connect to database
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    // Read from ItemTable
    let current_data: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["jetskiStateSync.agentManagerInitState"],
            |row| row.get(0),
        )
        .map_err(|_| {
            "Login state data not found (jetskiStateSync.agentManagerInitState)".to_string()
        })?;

    // Base64 decode
    let blob = general_purpose::STANDARD
        .decode(&current_data)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    // 1. Find oauthTokenInfo (Field 6)
    let oauth_data = protobuf::find_field(&blob, 6)
        .map_err(|e| format!("Failed to parse Protobuf: {}", e))?
        .ok_or("OAuth data not found (Field 6)")?;
    // 2. Extract refresh_token (Field 3)
    let refresh_bytes = protobuf::find_field(&oauth_data, 3)
        .map_err(|e| format!("Failed to parse OAuth data: {}", e))?
        .ok_or("Data does not contain Refresh Token (Field 3)")?;
    String::from_utf8(refresh_bytes).map_err(|_| "Refresh Token is not UTF-8 encoded".to_string())
}

/// Get Refresh Token from default database (legacy compatibility)
pub fn get_refresh_token_from_db() -> Result<String, String> {
    let db_path = vscode::get_vscode_db_path()?;
    extract_refresh_token_from_file(&db_path)
}
