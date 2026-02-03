use crate::utils::protobuf;
use base64::{engine::general_purpose, Engine as _};
use std::path::PathBuf;

/// Extract Refresh Token from database (common logic).
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

/// Get Refresh Token from default database (legacy compatibility).
pub fn get_refresh_token_from_db() -> Result<String, String> {
    let db_path = crate::modules::vscode::get_vscode_db_path()?;
    extract_refresh_token_from_file(&db_path)
}
