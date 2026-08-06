# Spec 06: Schema hardening — missing index, missing CHECKs, dead SQL

- **Priority:** medium
- **Effort:** S
- **Category:** correctness

## Problem(s)

### 06.1 No index on `"user"` despite a per-user COUNT on every create
`migrations/20260116092854_initial_schema.sql:5-13` indexes only `token` (via UNIQUE). `create_service` runs `SELECT COUNT(*) FROM services WHERE "user" = $1` on every POST (`src/views.rs:101-107`) — a sequential scan. Data volume is tiny today, so impact is low, but the index is one line.

**Fix:** Migration: `CREATE INDEX IF NOT EXISTS services_user_idx ON services ("user");`

### 06.2 No CHECK constraints or defaults on `status`/`cache`/`created_time`
`status VARCHAR(12) NOT NULL`, `cache VARCHAR(12) NOT NULL` (migration lines 9,11) accept any short string (see Spec 05.2), and `created_time TIMESTAMPTZ NOT NULL` (line 10) relies on the app clock (`src/views.rs:141`).

**Fix:** After agreeing the value sets with book_bot: `ALTER TABLE services ADD CONSTRAINT services_status_check CHECK (status IN (...))`, same for `cache`; `ALTER COLUMN created_time SET DEFAULT now()`.

### 06.3 Dead redundant index block in the migration
`migrations/20260116092854_initial_schema.sql:17-27`: a `DO $$ ... CREATE UNIQUE INDEX services_token_key ...` block guarded by a pg_indexes lookup — but the `UNIQUE` on line 7 already creates an index named `services_token_key`, so the block can never execute. Dead SQL that suggests the constraint might be absent.

**Fix:** Remove the block (note: sqlx checksums applied migrations — either accept the checksum implication for fresh installs only, or clean it up in the next new migration file and leave the applied one untouched).

## Acceptance criteria
- `\d services` shows an index on `"user"`, CHECK constraints on `status`/`cache`, and a `now()` default on `created_time`.
- Migrations still run cleanly on both a fresh database and the existing production database (sqlx migration checksums preserved for already-applied files).
