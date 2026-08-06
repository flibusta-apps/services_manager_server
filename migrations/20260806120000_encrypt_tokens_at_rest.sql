-- Add encrypted-token storage columns. The legacy plaintext `token` column
-- is kept (now nullable) during the transition; the app no longer reads or
-- writes it going forward except during the one-time startup backfill.
-- Dropping it is a deliberate manual step — see scripts/finalize_token_encryption.sql.

ALTER TABLE services
    ADD COLUMN IF NOT EXISTS token_ciphertext BYTEA,
    ADD COLUMN IF NOT EXISTS token_nonce BYTEA,
    ADD COLUMN IF NOT EXISTS token_hmac BYTEA;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_name = 'services' AND constraint_name = 'services_token_key'
    ) THEN
        ALTER TABLE services DROP CONSTRAINT services_token_key;
    END IF;
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'services' AND indexname = 'services_token_key'
    ) THEN
        DROP INDEX services_token_key;
    END IF;
END
$$;

ALTER TABLE services ALTER COLUMN token DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'services' AND indexname = 'services_token_hmac_key'
    ) THEN
        CREATE UNIQUE INDEX services_token_hmac_key ON services(token_hmac);
    END IF;
END
$$;
