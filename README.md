# services_manager_server

Manages Telegram bot registrations. Bot tokens are encrypted at rest (AES-256-GCM).

## Configuration

See `.env.example`. Invalid or unparsable values make the service fail at startup.

| Variable | Required | Description |
|---|---|---|
| `API_KEY` | yes | API secret, min 16 characters |
| `TOKEN_ENC_KEY` | yes | base64 of exactly 32 bytes. Generate: `openssl rand -base64 32` |
| `POSTGRES_USER/PASSWORD/HOST/PORT/DB` | yes | Postgres connection (`POSTGRES_PORT` is a u16). Default sslmode is `prefer` |
| `SENTRY_DSN` | no | Sentry DSN |
| `PORT` | no | Listen port, default 8080 |
| `POSTGRES_POOL_*` | no | Pool max connections / acquire timeout / max lifetime |
| `RUST_LOG` | no | Log filter, default `info` |

## Authentication

Send `Authorization: Bearer <API_KEY>` or the raw `Authorization: <API_KEY>`.
`/health` and `/ready` are unauthenticated.

## Behavior notes

- `GET /` returns 500 (logging the offending row id) if any token cannot be decrypted, rather than omitting it: book_bot treats bots missing from the list as deleted.
- Requests time out after 10s; DB `statement_timeout`=10s, `lock_timeout`=5s.

## Tests

`cargo test` runs unit tests only; the DB readiness test is `#[ignore]`d (`cargo test -- --ignored` with `POSTGRES_*` set).
