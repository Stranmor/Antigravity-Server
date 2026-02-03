-- Antigravity Manager: Initial PostgreSQL Schema
-- Replaces JSON file storage with proper relational database

-- ============================================================================
-- ACCOUNTS TABLE
-- ============================================================================
CREATE TABLE IF NOT EXISTS accounts (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name TEXT,
    
    -- Status flags
    disabled BOOLEAN NOT NULL DEFAULT FALSE,
    disabled_reason TEXT,
    disabled_at TIMESTAMPTZ,
    
    proxy_disabled BOOLEAN NOT NULL DEFAULT FALSE,
    proxy_disabled_reason TEXT,
    proxy_disabled_at TIMESTAMPTZ,
    
    -- Protected models (stored as JSON array for flexibility)
    protected_models JSONB NOT NULL DEFAULT '[]'::jsonb,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
CREATE INDEX IF NOT EXISTS idx_accounts_disabled ON accounts(disabled) WHERE disabled = FALSE;
CREATE INDEX IF NOT EXISTS idx_accounts_proxy_disabled ON accounts(proxy_disabled) WHERE proxy_disabled = FALSE;

-- ============================================================================
-- TOKENS TABLE (separate for security, 1:1 with accounts)
-- ============================================================================
CREATE TABLE IF NOT EXISTS tokens (
    account_id UUID PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    expiry_timestamp BIGINT NOT NULL,
    project_id TEXT,
    email TEXT,
    tier TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tokens_expiry ON tokens(expiry_timestamp);
CREATE INDEX IF NOT EXISTS idx_tokens_project_id ON tokens(project_id) WHERE project_id IS NOT NULL;

-- ============================================================================
-- QUOTAS TABLE (latest quota snapshot per account)
-- ============================================================================
CREATE TABLE IF NOT EXISTS quotas (
    account_id UUID PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    is_forbidden BOOLEAN NOT NULL DEFAULT FALSE,
    models JSONB NOT NULL DEFAULT '[]'::jsonb,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- ACCOUNT EVENTS (Event Sourcing - append-only)
-- ============================================================================
CREATE TABLE IF NOT EXISTS account_events (
    id BIGSERIAL PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_account_events_account_id ON account_events(account_id);
CREATE INDEX IF NOT EXISTS idx_account_events_type ON account_events(event_type);
CREATE INDEX IF NOT EXISTS idx_account_events_created_at ON account_events(created_at DESC);

-- Event types:
-- - account_created
-- - account_updated
-- - account_disabled
-- - account_enabled
-- - token_refreshed
-- - quota_updated
-- - rate_limited
-- - model_protected
-- - model_unprotected
-- - phone_verification_required

-- ============================================================================
-- REQUEST LOG (Analytics - for identifying reliable accounts)
-- ============================================================================
CREATE TABLE IF NOT EXISTS requests (
    id BIGSERIAL PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    tokens_in INTEGER,
    tokens_out INTEGER,
    latency_ms INTEGER,
    status_code INTEGER NOT NULL,
    error_type TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_requests_account_id ON requests(account_id);
CREATE INDEX IF NOT EXISTS idx_requests_model ON requests(model);
CREATE INDEX IF NOT EXISTS idx_requests_created_at ON requests(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_status_code ON requests(status_code) WHERE status_code >= 400;

-- Partial index for failed requests (for debugging)
CREATE INDEX IF NOT EXISTS idx_requests_errors ON requests(account_id, created_at DESC) 
    WHERE status_code >= 400;

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Auto-update updated_at on accounts table
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

DROP TRIGGER IF EXISTS update_accounts_updated_at ON accounts;
CREATE TRIGGER update_accounts_updated_at
    BEFORE UPDATE ON accounts
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_tokens_updated_at ON tokens;
CREATE TRIGGER update_tokens_updated_at
    BEFORE UPDATE ON tokens
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- APP SETTINGS (key-value store for app state like current_account_id)
-- ============================================================================
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DROP TRIGGER IF EXISTS update_app_settings_updated_at ON app_settings;
CREATE TRIGGER update_app_settings_updated_at
    BEFORE UPDATE ON app_settings
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- VIEWS (Computed from events + current state)
-- ============================================================================

-- Account health view: aggregates request success rate per account
CREATE OR REPLACE VIEW account_health AS
SELECT 
    a.id,
    a.email,
    a.name,
    COUNT(r.id) AS total_requests,
    COUNT(CASE WHEN r.status_code < 400 THEN 1 END) AS successful_requests,
    COUNT(CASE WHEN r.status_code = 429 THEN 1 END) AS rate_limited_requests,
    ROUND(
        COUNT(CASE WHEN r.status_code < 400 THEN 1 END)::numeric / 
        NULLIF(COUNT(r.id), 0) * 100, 2
    ) AS success_rate_pct,
    AVG(r.latency_ms) AS avg_latency_ms,
    MAX(r.created_at) AS last_request_at
FROM accounts a
LEFT JOIN requests r ON a.id = r.account_id
    AND r.created_at > NOW() - INTERVAL '24 hours'
GROUP BY a.id, a.email, a.name;

-- Recent events view: last 100 events for debugging
CREATE OR REPLACE VIEW recent_events AS
SELECT 
    ae.id,
    a.email,
    ae.event_type,
    ae.metadata,
    ae.created_at
FROM account_events ae
JOIN accounts a ON ae.account_id = a.id
ORDER BY ae.created_at DESC
LIMIT 100;
