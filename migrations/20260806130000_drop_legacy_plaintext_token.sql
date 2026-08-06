-- Finalize token-at-rest encryption (Spec 01.1).
--
-- This is a REAL, auto-run migration. Deploying it means pushing to `main`,
-- which triggers an automatic Docker build + production deploy. Before
-- deploying this:
--   1. Take a full backup of the database.
--   2. Make sure the release containing the encryption migration
--      (20260806120000_encrypt_tokens_at_rest.sql) plus its startup backfill
--      (`db::backfill_token_encryption`) has already been deployed and run
--      to completion at least once.
--
-- What this does:
--   - Verifies every row already has non-NULL token_ciphertext/token_nonce/
--     token_hmac. If any row is still missing them (e.g. this migration runs
--     before the backfill has had a chance to run, such as on a fresh
--     database where both encryption migrations land in the same deploy),
--     it aborts the whole migration (and thus this transaction, since sqlx
--     runs each migration file in its own transaction) via RAISE EXCEPTION.
--     This guard is the load-bearing safety property here: it must be
--     impossible for this migration to silently drop plaintext tokens that
--     were never encrypted.
--   - Only once the guard passes: enforces NOT NULL on the encrypted columns
--     and drops the legacy plaintext `token` column entirely.

DO $$
DECLARE
    missing_count BIGINT;
BEGIN
    SELECT COUNT(*) INTO missing_count
    FROM services
    WHERE token_ciphertext IS NULL
       OR token_nonce IS NULL
       OR token_hmac IS NULL;

    IF missing_count > 0 THEN
        RAISE EXCEPTION 'cannot finalize token encryption: % row(s) still missing encrypted token columns; ensure the backfill has run to completion before deploying this migration', missing_count;
    END IF;
END
$$;

ALTER TABLE services
    ALTER COLUMN token_ciphertext SET NOT NULL,
    ALTER COLUMN token_nonce SET NOT NULL,
    ALTER COLUMN token_hmac SET NOT NULL;

ALTER TABLE services DROP COLUMN token;
