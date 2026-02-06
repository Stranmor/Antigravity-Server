-- Antigravity Manager: Thinking Signatures Persistent Storage
-- Stores Claude thinking signatures for recovery across server restarts

-- ============================================================================
-- THINKING SIGNATURES TABLE
-- ============================================================================
-- Keyed by SHA256 hash of thinking content (first 16 chars)
-- Allows signature recovery when the same thinking content appears in history

CREATE TABLE IF NOT EXISTS thinking_signatures (
    content_hash TEXT PRIMARY KEY,
    signature TEXT NOT NULL,
    model_family TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_thinking_signatures_last_used 
    ON thinking_signatures(last_used_at DESC);

CREATE INDEX IF NOT EXISTS idx_thinking_signatures_model_family
    ON thinking_signatures(model_family);

-- Auto-cleanup: entries older than 7 days that haven't been used
-- Run periodically via cron or application logic
-- DELETE FROM thinking_signatures WHERE last_used_at < NOW() - INTERVAL '7 days';
