-- Initial schema migration for services table
-- This migration is idempotent and safe to run on existing databases

-- Create services table if it doesn't exist
CREATE TABLE IF NOT EXISTS services (
    id SERIAL PRIMARY KEY,
    token VARCHAR(128) NOT NULL UNIQUE,
    "user" BIGINT NOT NULL,
    status VARCHAR(12) NOT NULL,
    created_time TIMESTAMPTZ NOT NULL,
    cache VARCHAR(12) NOT NULL,
    username VARCHAR(64) NOT NULL
);

-- Create unique index on token if it doesn't exist
-- Note: The UNIQUE constraint already creates an index, but we ensure it exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'services'
        AND indexname = 'services_token_key'
    ) THEN
        CREATE UNIQUE INDEX services_token_key ON services(token);
    END IF;
END
$$;
