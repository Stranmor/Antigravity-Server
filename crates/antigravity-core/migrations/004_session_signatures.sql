-- Antigravity Manager: Session Signatures Persistent Storage
-- Stores thinking signatures per session_id for recovery across server restarts
-- This ensures clients that don't send thinking blocks (e.g. OpenWebUI)
-- can still continue conversations with thinking models after server restart.

-- ============================================================================
-- SESSION SIGNATURES TABLE
-- ============================================================================
-- Keyed by session_id (fingerprint of conversation content)
-- Allows signature recovery even when clients strip thinking blocks

CREATE TABLE IF NOT EXISTS session_signatures (
    session_id TEXT PRIMARY KEY,
    signature TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_session_signatures_updated
    ON session_signatures(updated_at DESC);

-- Auto-cleanup: entries older than 30 days
-- Run periodically via application logic
-- DELETE FROM session_signatures WHERE updated_at < NOW() - INTERVAL '30 days';
