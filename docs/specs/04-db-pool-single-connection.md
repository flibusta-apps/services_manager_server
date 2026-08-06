# Spec 04: Database pool is limited to a single connection

- **Priority:** medium
- **Effort:** S
- **Category:** reliability

## Problem(s)

### 04.1 `max_connections(1)` serializes all DB access
`src/db.rs:25-29`:
```rust
PgPoolOptions::new()
    .max_connections(1)
    .connect(&database_url)
```
Every request queues behind one Postgres connection. Any slow/stuck query stalls the whole API; under sqlx's default 30 s acquire timeout, queued requests then start failing — and those failures are `.unwrap()`ed into process aborts (Spec 02.1). Unlike users_settings_server (which makes pool size and acquire timeout configurable), nothing here is tunable, and there is no `application_name` in the connection string (`src/db.rs:16-23`) for identifying the client in `pg_stat_activity`.

**Fix:** Default `max_connections` to ~5-10 and read it from env (`POSTGRES_POOL_MAX_CONNECTIONS`), set an explicit `acquire_timeout` (<= 10 s) from env, and add `?application_name=services_manager_server` to the URL — mirroring `users_settings_server/src/db.rs:24-31` and `config.rs:39-47`.

## Acceptance criteria
- Pool size and acquire timeout configurable via env with sane defaults (> 1 connection, <= 10 s timeout).
- `pg_stat_activity` shows the service's `application_name`.
- A deliberately slow query (pg_sleep) on one request does not block a concurrent `GET /{id}/`.
