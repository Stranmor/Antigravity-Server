//! Antigravity Manager - Native Desktop UI
//!
//! This is the Slint-based native desktop application.

mod backend;

use antigravity_core::modules::logger;
use backend::BackendState;

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
    let (
        total_accounts,
        avg_gemini,
        avg_gemini_image,
        avg_claude,
        low_quota_count,
        current_email,
        current_name,
        current_last_used,
        all_count,
        pro_count,
        ultra_count,
        free_count,
        accounts_data,
    ) = {
        let mut state = runtime.block_on(backend.lock());
        if let Err(e) = state.load_accounts() {
            tracing::warn!("Failed to load accounts: {}", e);
            (0, 0, 0, 0, 0, String::new(), String::new(), String::new(), 0, 0, 0, 0, Vec::new())
        } else {
            let current = state.get_current_account();
            let current_id = state.current_account_id().map(|s| s.to_string());
            let email = current.map(|a| a.email.clone()).unwrap_or_default();
            let name = current.and_then(|a| a.name.clone()).unwrap_or_default();
            let last_used = current.map(|a| {
                chrono::DateTime::from_timestamp(a.last_used, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            }).unwrap_or_default();
            
            // Build accounts data for UI
            let accounts_data: Vec<AccountData> = state.accounts().iter().map(|a| {
                let is_current = current_id.as_ref().map(|id| &a.id == id).unwrap_or(false);
                let tier = BackendState::get_tier(a);
                let is_forbidden = a.quota.as_ref().map(|q| q.is_forbidden).unwrap_or(false);
                
                AccountData {
                    id: a.id.clone().into(),
                    email: a.email.clone().into(),
                    name: a.name.clone().unwrap_or_default().into(),
                    disabled: a.disabled,
                    disabled_reason: a.disabled_reason.clone().unwrap_or_default().into(),
                    proxy_disabled: a.proxy_disabled,
                    subscription_tier: tier.into(),
                    last_used: chrono::DateTime::from_timestamp(a.last_used, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                        .into(),
                    gemini_pro_quota: BackendState::get_model_quota(a, "gemini-3-pro") as i32,
                    gemini_flash_quota: BackendState::get_model_quota(a, "flash") as i32,
                    gemini_image_quota: BackendState::get_model_quota(a, "image") as i32,
                    claude_quota: BackendState::get_model_quota(a, "claude") as i32,
                    is_current,
                    is_forbidden,
                }
            }).collect();
            
            (
                state.account_count() as i32,
                state.avg_gemini_quota() as i32,
                state.avg_gemini_image_quota() as i32,
                state.avg_claude_quota() as i32,
                state.low_quota_count() as i32,
                email,
                name,
                last_used,
                state.account_count() as i32,
                state.pro_count() as i32,
                state.ultra_count() as i32,
                state.free_count() as i32,
                accounts_data,
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

    // Set AppState global data for AccountsPage
    let accounts_model = slint::ModelRc::new(slint::VecModel::from(accounts_data));
    app.global::<AppState>().set_accounts(accounts_model);
    app.global::<AppState>().set_all_count(all_count);
    app.global::<AppState>().set_pro_count(pro_count);
    app.global::<AppState>().set_ultra_count(ultra_count);
    app.global::<AppState>().set_free_count(free_count);
    app.global::<AppState>().set_total_items(total_accounts);
    app.global::<AppState>().set_total_pages(((total_accounts as f32) / 20.0).ceil().max(1.0) as i32);
    app.global::<AppState>().set_current_page(1);

    // Set up AppState callbacks
    app.global::<AppState>().on_add_account(|| {
        tracing::info!("AppState: Add account requested");
    });

    app.global::<AppState>().on_refresh_all(|| {
        tracing::info!("AppState: Refresh all requested");
    });

    app.global::<AppState>().on_export_selected(|| {
        tracing::info!("AppState: Export selected requested");
    });

    app.global::<AppState>().on_delete_selected(|| {
        tracing::info!("AppState: Delete selected requested");
    });

    app.global::<AppState>().on_toggle_proxy_batch(|enable| {
        tracing::info!("AppState: Toggle proxy batch: {}", enable);
    });

    app.global::<AppState>().on_search_changed(|query| {
        tracing::info!("AppState: Search: {}", query);
    });

    app.global::<AppState>().on_toggle_select(|id| {
        tracing::info!("AppState: Toggle select: {}", id);
    });

    app.global::<AppState>().on_toggle_all(|| {
        tracing::info!("AppState: Toggle all");
    });

    app.global::<AppState>().on_switch_account(|id| {
        tracing::info!("AppState: Switch to account: {}", id);
    });

    app.global::<AppState>().on_refresh_account(|id| {
        tracing::info!("AppState: Refresh account: {}", id);
    });

    app.global::<AppState>().on_view_details(|id| {
        tracing::info!("AppState: View details: {}", id);
    });

    app.global::<AppState>().on_export_account(|id| {
        tracing::info!("AppState: Export account: {}", id);
    });

    app.global::<AppState>().on_delete_account(|id| {
        tracing::info!("AppState: Delete account: {}", id);
    });

    app.global::<AppState>().on_toggle_proxy(|id| {
        tracing::info!("AppState: Toggle proxy: {}", id);
    });

    // Set up dashboard callbacks
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
    });

    app.on_switch_account(|| {
        tracing::info!("Switch account requested");
    });

    tracing::info!("Application window created, running event loop...");

    if let Err(e) = app.run() {
        tracing::error!("Application error: {}", e);
        std::process::exit(1);
    }
}
