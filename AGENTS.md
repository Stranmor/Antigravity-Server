# Antigravity Manager - Leptos UI Parity Plan

## Current Status: ~45% Feature Parity

## Phase 1: Core Types & IPC (DONE ✅)
- [x] Account, ProxyStatus, AppConfig types
- [x] Basic Tauri IPC bindings
- [x] Extended IPC (OAuth, quotas, logs, updates)

## Phase 2: Pages Foundation (DONE ✅)
- [x] Dashboard - basic stats
- [x] Accounts - basic list
- [x] Proxy - basic controls
- [x] Settings - basic form
- [x] Monitor - basic logs

---

## Phase 3: Accounts Page Full Parity (IN PROGRESS)
### 3.1 View Modes
- [ ] List view (current, improve)
- [ ] Grid view (AccountGrid cards)
- [ ] View mode toggle button group

### 3.2 Filtering & Pagination
- [ ] Filter tabs: All / Pro / Ultra / Free with counts
- [ ] Pagination component
- [ ] Dynamic page size

### 3.3 Account Actions
- [ ] Individual refresh (per account)
- [ ] Toggle proxy status on/off
- [ ] Batch enable/disable proxy
- [ ] Export accounts to JSON
- [ ] View details modal
- [ ] Confirmation dialogs

### 3.4 Reordering
- [ ] Drag-and-drop reorder (optional - complex in WASM)

---

## Phase 4: API Proxy Page Full Parity
### 4.1 Configuration Section
- [ ] Port, timeout, auto-start (done)
- [ ] LAN access toggle
- [ ] Auth mode selector (off/strict/auto/all_except_health)
- [ ] API key section (done)

### 4.2 Model Routing
- [ ] Custom mapping table (add/edit/remove)
- [ ] Preset mappings button (GPT-4→Gemini etc)
- [ ] Reset mappings button

### 4.3 Scheduling
- [ ] Sticky session config
- [ ] Balance mode selector
- [ ] Clear session bindings button

### 4.4 External Providers
- [ ] Z.ai/GLM integration section
- [ ] Base URL, dispatch mode
- [ ] Model mapping for Z.ai

### 4.5 Quick Start Examples
- [ ] Protocol selector (OpenAI/Anthropic/Gemini)
- [ ] Model selector dropdown
- [ ] Dynamic code generation

---

## Phase 5: Dashboard Improvements
- [ ] BestAccounts component (top 5 by quota)
- [ ] Export all button
- [ ] Personalized greeting with account name
- [ ] Quick action cards (done)

---

## Phase 6: Settings Improvements  
- [ ] Export path selector (file dialog)
- [ ] Antigravity path config
- [ ] Locale/i18n support (optional)

---

## Phase 7: Shared Components
- [ ] Modal dialogs (confirm/alert)
- [ ] Pagination component
- [ ] Tooltip component
- [ ] Collapsible card component
- [ ] Select/Dropdown component

---

## Build & Deploy
- [ ] Fix wasm-opt for release builds
- [ ] Production bundle optimization
- [ ] Tauri integration test

---

## Priority Order (Critical Path):
1. **Phase 3.2** - Pagination & Filters (high visibility)
2. **Phase 4.2** - Model Routing (key feature)
3. **Phase 7** - Modal dialogs (needed for confirmations)
4. **Phase 3.3** - Account actions
5. **Phase 4.1** - LAN/Auth config
6. **Phase 5** - Dashboard polish
