# Spec 02: Panic-driven error handling and error-swallowing fallbacks

- **Priority:** high
- **Effort:** M
- **Category:** reliability

## Problem(s)

### 02.1 Handlers `.unwrap()` DB results; `panic = 'abort'` kills the process
All six handlers unwrap sqlx results: `src/views.rs:42,57,75,145,165,188` (e.g. `.fetch_all(&db.0).await.unwrap()`). `Cargo.toml:13` sets `panic = 'abort'` for release, so any transient DB error or constraint violation aborts the whole server — no unwinding, no `CatchPanicLayer`. With a single-connection pool (Spec 04) a DB hiccup is not rare.

**Fix:** Introduce an error type implementing `IntoResponse` (500 + traced error), return `Result` from handlers, use `?`. Reconsider `panic = 'abort'` or add `CatchPanicLayer` after unwraps are gone.

### 02.2 `unwrap_or(Some(0))` silently converts DB errors into "0 bots"
`src/views.rs:101-110`:
```rust
let exist_count = sqlx::query_scalar!(...)
    .fetch_one(&db.0)
    .await
    .unwrap_or(Some(0))
    .unwrap();
```
If the COUNT query fails, the error is swallowed and `exist_count = 0`, so the per-user limit check (views.rs:112) is bypassed and creation proceeds. Same pattern for the token-existence check at views.rs:116-125 (`unwrap_or(Some(false))`): a DB error skips the duplicate check and falls through to the INSERT, where the unique constraint violation then panics (views.rs:145) and aborts the process (02.1).

**Fix:** Propagate both errors as 500; never default security/limit checks to the permissive value.

### 02.3 Startup config unwraps with poor diagnostics
`src/config.rs:27`: `get_env("POSTGRES_PORT").parse().unwrap()`; `src/main.rs:32`: `Dsn::from_str(&config::CONFIG.sentry_dsn).unwrap()`; `src/db.rs:29`: pool `.connect(...).unwrap()`. Boot-time failure is acceptable, but messages should name the offending variable, and Sentry should be optional (see Spec 08.2).

**Fix:** `expect`/`unwrap_or_else` with explicit variable names; optional `SENTRY_DSN`.

## Acceptance criteria
- No `.unwrap()`/`.expect()` on fallible operations in request handlers; injected DB failures yield 500 responses and the process keeps serving.
- A failing COUNT/EXISTS query results in 500, not a created service.
- Creating a service with an already-registered token returns 409 (see Spec 03), never a crash.
