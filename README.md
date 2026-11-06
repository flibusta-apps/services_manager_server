# services_manager_server

Manages Telegram bot registrations. Bot tokens are encrypted at rest (AES-256-GCM).

## Configuration

See `.env.example`. Invalid or unparsable values make the service fail at startup.

| Variable | Required | Description |
|---|---|---|
| `API_KEY` | yes | API secret, min 16 characters |
| `METRICS_API_KEY` | no | Separate `/metrics` secret, min 16 characters when nonblank; use a different value from `API_KEY` |
| `TOKEN_ENC_KEY` | yes | base64 of exactly 32 bytes. Generate: `openssl rand -base64 32` |
| `POSTGRES_USER/PASSWORD/HOST/PORT/DB` | yes | Postgres connection (`POSTGRES_PORT` is a u16). Default sslmode is `prefer` |
| `SENTRY_DSN` | no | Sentry DSN; empty/whitespace disables Sentry |
| `PORT` | no | Listen port, default 8080 |
| `POSTGRES_POOL_*` | no | Pool max connections / acquire timeout / max lifetime |
| `RUST_LOG` | no | Log filter, default `info` |

## Authentication

Send `Authorization: Bearer <API_KEY>` or the raw `Authorization: <API_KEY>`.
`/health` and `/ready` are unauthenticated.

When `METRICS_API_KEY` is set, `/metrics` accepts only that key (Bearer or raw),
not `API_KEY`. The metrics key does not grant API access. Use distinct secrets:
`API_KEY` permits decrypted token export (`GET /`) and mutations. If
`METRICS_API_KEY` is unset or blank, `/metrics` continues accepting `API_KEY`
for backward compatibility, with a startup warning.

## Upgrading from plaintext schema

Take a full database backup and stop other instances/writers before upgrading.
Keep the same `TOKEN_ENC_KEY` if any tokens have already been encrypted; never
replace it with a newly generated key during an upgrade.

Startup runs migrations through `20260806120000_encrypt_tokens_at_rest`, then
backfills legacy plaintext tokens using the existing AES-GCM/HMAC implementation,
and only then runs the guarded `20260806130000_drop_legacy_plaintext_token` and
remaining migrations. A migration advisory lock covers this entire sequence.
The backfill is restart-safe and skipped when the plaintext column is already
gone. Rows missing any encrypted field are rebuilt from their plaintext token;
rows with no recoverable plaintext remain subject to the unchanged guard, which
aborts startup rather than dropping their data.

The original backfill shipped in commit `90e60cca08bc6dc1cd0bb18bd6b6e18e656f5fc0`
and was removed in `be80bc8c39e515599436d5be8d1d6574aa328539`; it is now restored
with the ordering required for direct upgrades from a populated plaintext schema.

## Behavior notes

- `GET /` returns 500 (logging the offending row id) if any token cannot be decrypted, rather than omitting it: book_bot treats bots missing from the list as deleted.
- Requests time out after 10s; DB `statement_timeout`=10s, `lock_timeout`=5s.

## Tests

`DATABASE_URL=postgres://user:password@localhost/database SQLX_OFFLINE=true cargo test --offline --locked`
runs unit tests and migration integration tests. The database role must be able
to create databases; SQLx creates isolated scratch databases for the migration
tests. These cover plaintext upgrades, partially encrypted transitional rows,
and no-op startup on the latest schema. The DB readiness test is separately
`#[ignore]`d (`cargo test -- --ignored` with `POSTGRES_*` set).
