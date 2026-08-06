# Spec 01: Telegram bot tokens are stored and served in plaintext

- **Priority:** high
- **Effort:** M
- **Category:** security

## Problem(s)

### 01.1 Tokens stored unencrypted at rest
`migrations/20260116092854_initial_schema.sql:7`: `token VARCHAR(128) NOT NULL UNIQUE` — raw Telegram bot tokens (full account takeover of each bot) live in plaintext in Postgres. Any DB backup, dump, replica, or SQL access leaks every registered bot's credentials. The DB is reachable at a public host/port (see Spec 06.1), making this the single most valuable target in the system.

**Fix:** Encrypt the token column at the application layer (e.g. AES-GCM/XChaCha20-Poly1305 with a `TOKEN_ENC_KEY` env secret; store nonce+ciphertext, keep a deterministic HMAC column for the uniqueness check). Provide a migration that encrypts existing rows. Alternatively, at minimum, document and enforce pgcrypto/at-rest disk encryption plus network isolation of the DB.

### 01.2 Every endpoint returns the full token; `GET /` dumps all tokens to any key holder
`src/views.rs:22-31`:
```rust
#[derive(sqlx::FromRow, Serialize)]
pub struct Service {
    pub id: i32,
    pub token: String,
    ...
}
```
`get_services` (`src/views.rs:33-45`) returns `SELECT * FROM services` — one request with the single shared `API_KEY` yields every bot token. The same `Service` struct (token included) is echoed by `GET /{id}/`, `DELETE /{id}/`, `POST /`, `PATCH /{id}/update_status`, `PATCH /{id}/update_cache` (views.rs:60,78,147,168,191), even though status/cache updaters have no need for the token. book_bot legitimately needs tokens from `GET /` to launch bots (`book_bot/book_bot/src/bots_manager/bot_manager_client.rs:26,32`), but the blast radius of the one static key is total.

**Fix:** Split responses: a `ServiceInfo` without `token` for list/get/update/delete responses, and a dedicated token-bearing response only where the consumer requires it (documented). Longer term, per-consumer keys or scoped keys so status updaters cannot read tokens.

### 01.3 Nothing prevents tokens from reaching logs or Sentry
Handlers `.unwrap()` sqlx results (e.g. `src/views.rs:145` on the INSERT that includes the token as `$1`); sqlx error/panic messages can embed query context, and the sentry-tracing layer forwards ERROR events (`src/main.rs:40-43`). There is no redaction or `Debug`-suppression on `Service`/`CreateServiceData` (`src/views.rs:83-90`).

**Fix:** As part of the error-handling rework (Spec 02), map DB errors to responses without echoing values; implement a manual `Debug` for `Service`/`CreateServiceData` that masks `token` (first 5 chars max, as book_bot already does in `axum_server.rs:60`).

## Acceptance criteria
- Tokens encrypted at rest (or a documented, enforced compensating control); DB dump does not reveal usable tokens.
- List/get/update/delete responses contain no token field; only the documented token-bearing endpoint returns it.
- Grepping logs/Sentry events from a failure injection test shows no full token anywhere.
