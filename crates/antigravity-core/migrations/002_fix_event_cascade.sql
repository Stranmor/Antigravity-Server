-- Fix ON DELETE CASCADE for account_events
-- Events should be preserved for audit trail even after account deletion

-- Step 1: Make account_id nullable
ALTER TABLE account_events 
    ALTER COLUMN account_id DROP NOT NULL;

-- Step 2: Drop existing FK constraint and recreate with SET NULL
ALTER TABLE account_events 
    DROP CONSTRAINT IF EXISTS account_events_account_id_fkey;

ALTER TABLE account_events 
    ADD CONSTRAINT account_events_account_id_fkey 
    FOREIGN KEY (account_id) 
    REFERENCES accounts(id) 
    ON DELETE SET NULL;


