//! Antigravity Manager - Native Desktop UI
//!
//! This is the Slint-based native desktop application.
//! It replaces the Tauri WebView-based frontend with pure Rust rendering.

mod backend;

use antigravity_core::modules::logger;

slint::include_modules!();

fn main() {
    // Initialize logging
    logger::init_logger();
    tracing::info!("Antigravity Manager starting...");

    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    let _guard = runtime.enter();

    // Create backend
    let backend = backend::create_backend();

    // Load accounts
    let (total_accounts, avg_gemini, avg_gemini_image, avg_claude, low_quota_count, current_email, current_name, current_last_used) = {
        let mut state = runtime.block_on(backend.lock());
        if let Err(e) = state.load_accounts() {
            tracing::warn!("Failed to load accounts: {}", e);
            (0, 0, 0, 0, 0, String::new(), String::new(), String::new())
        } else {
            let current = state.get_current_account();
            let email = current.map(|a| a.email.clone()).unwrap_or_default();
            let name = current.and_then(|a| a.name.clone()).unwrap_or_default();
            let last_used = current.map(|a| {
                chrono::DateTime::from_timestamp(a.last_used, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            }).unwrap_or_default();
            
            (
                state.account_count() as i32,
                state.avg_gemini_quota() as i32,
                state.avg_gemini_image_quota() as i32,
                state.avg_claude_quota() as i32,
                state.low_quota_count() as i32,
                email,
                name,
                last_used,
            )
        }
    };

    tracing::info!(
        "Stats: {} accounts, {}% avg Gemini, {}% avg Claude, {} low quota",
        total_accounts, avg_gemini, avg_claude, low_quota_count
    );

    // Create and run the main window
    let app = match AppWindow::new() {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("Failed to create application window: {}", e);
            std::process::exit(1);
        }
    };

    // Set dashboard stats
    app.set_stats(DashboardStats {
        total_accounts,
        avg_gemini,
        avg_gemini_image,
        avg_claude,
        low_quota_count,
    });

    // Set current account info
    let has_account = !current_email.is_empty();
    app.set_current_account(CurrentAccountInfo {
        email: current_email.into(),
        name: if current_name.is_empty() { 
            "U".into() 
        } else { 
            current_name.chars().next().unwrap_or('U').to_string().into() 
        },
        last_used: current_last_used.into(),
        has_account,
    });

    // Set up callbacks
    let backend_clone = backend.clone();
    let runtime_handle = runtime.handle().clone();
    app.on_refresh_accounts(move || {
        tracing::info!("Refresh accounts requested");
        let backend = backend_clone.clone();
        runtime_handle.spawn(async move {
            let mut state = backend.lock().await;
            if let Err(e) = state.load_accounts() {
                tracing::error!("Failed to refresh accounts: {}", e);
            } else {
                tracing::info!("Accounts refreshed: {}", state.account_count());
            }
        });
    });

    app.on_add_account(|| {
        tracing::info!("Add account requested");
        // TODO: Implement account add dialog
    });

    app.on_switch_account(|| {
        tracing::info!("Switch account requested");
        // TODO: Implement account switch dialog
    });

    tracing::info!("Application window created, running event loop...");

    if let Err(e) = app.run() {
        tracing::error!("Application error: {}", e);
        std::process::exit(1);
    }
}
