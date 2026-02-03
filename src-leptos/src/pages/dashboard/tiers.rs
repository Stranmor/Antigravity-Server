//! Tier breakdown and quick actions sections

use crate::api_models::DashboardStats;
use leptos::prelude::*;

#[component]
pub fn TierSection(stats: Memo<DashboardStats>) -> impl IntoView {
    view! {
        <section class="tier-section">
            <h2>"Account Tiers"</h2>
            <div class="tier-grid">
                <div class="tier-card tier-card--ultra">
                    <span class="tier-count">{move || stats.get().ultra_count}</span>
                    <span class="tier-label">"Ultra"</span>
                </div>
                <div class="tier-card tier-card--pro">
                    <span class="tier-count">{move || stats.get().pro_count}</span>
                    <span class="tier-label">"Pro"</span>
                </div>
                <div class="tier-card tier-card--free">
                    <span class="tier-count">{move || stats.get().free_count}</span>
                    <span class="tier-label">"Free"</span>
                </div>
                <div class="tier-card tier-card--warning">
                    <span class="tier-count">{move || stats.get().low_quota_count}</span>
                    <span class="tier-label">"Low Quota"</span>
                </div>
            </div>
        </section>
    }
}

#[component]
pub fn QuickActionsSection() -> impl IntoView {
    view! {
        <section class="quick-actions">
            <h2>"Quick Actions"</h2>
            <div class="action-grid">
                <a href="/accounts" class="action-card">
                    <span class="action-icon">"➕"</span>
                    <span class="action-label">"Add Account"</span>
                </a>
                <a href="/proxy" class="action-card">
                    <span class="action-icon">"🔌"</span>
                    <span class="action-label">"Start Proxy"</span>
                </a>
                <a href="/monitor" class="action-card">
                    <span class="action-icon">"📡"</span>
                    <span class="action-label">"View Logs"</span>
                </a>
                <a href="/settings" class="action-card">
                    <span class="action-icon">"⚙️"</span>
                    <span class="action-label">"Settings"</span>
                </a>
            </div>
        </section>
    }
}
