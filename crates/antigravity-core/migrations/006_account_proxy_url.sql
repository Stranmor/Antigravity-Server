-- Add per-account proxy URL support.
-- Each account can have its own proxy (socks5://, http://) for all requests.
-- When set, ALL requests for this account go through this proxy (chat, OAuth, quota).
-- When NULL, uses the global proxy pool / direct connection.

ALTER TABLE accounts ADD COLUMN IF NOT EXISTS proxy_url TEXT;
