-- Finalize token-at-rest encryption (Spec 01.1).
--
-- MANUAL, ONE-TIME step. Do NOT run this automatically and do NOT add it to
-- migrations/ (sqlx would run it unattended on every deploy). It is
-- intentionally kept out of the auto-migration path so it only runs when an
-- operator deliberately decides the rollout is safe to finalize.
--
-- Preconditions before running this against any real database:
--   1. Take a full backup of the database.
--   2. Deploy the release containing the encryption migration
--      (see migrations/*_encrypt_tokens_at_rest.sql) and let the service
--      start at least once so its startup backfill runs.
--   3. Confirm the backfill is complete:
--        SELECT COUNT(*) FROM services WHERE token_ciphertext IS NULL;
--      This MUST return 0 before proceeding.
--   4. Confirm decryption round-trips correctly for a sample of rows using
--      the running service (e.g. GET / and compare against known tokens)
--      before dropping the plaintext column, since this step is
--      irreversible without the backup from step 1.
--
-- What this does:
--   - Enforces NOT NULL on the encrypted columns now that every row has
--     been backfilled.
--   - Drops the legacy plaintext `token` column entirely.
--
-- Run with: psql "$DATABASE_URL" -f scripts/finalize_token_encryption.sql

BEGIN;

ALTER TABLE services
    ALTER COLUMN token_ciphertext SET NOT NULL,
    ALTER COLUMN token_nonce SET NOT NULL,
    ALTER COLUMN token_hmac SET NOT NULL;

ALTER TABLE services DROP COLUMN token;

COMMIT;
