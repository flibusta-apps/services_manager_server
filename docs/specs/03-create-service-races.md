# Spec 03: create_service — TOCTOU races and odd status semantics

- **Priority:** high
- **Effort:** S
- **Category:** correctness

## Problem(s)

### 03.1 Bot-count limit check races with the insert
`src/views.rs:101-114`: `SELECT COUNT(*) ... WHERE "user" = $1`, then `if exist_count >= BOTS_COUNT_LIMIT { 402 }`, then a separate INSERT (views.rs:131-145). Two concurrent `POST /` for the same user both read `count = 4` and both insert — the 5-bot limit (`BOTS_COUNT_LIMIT`, views.rs:20) is not actually enforced under concurrency. There is no transaction anywhere in the handler.

**Fix:** Enforce atomically: single statement `INSERT ... SELECT ... WHERE (SELECT COUNT(*) FROM services WHERE "user" = $2) < 5 RETURNING *` (0 rows → limit hit), or a transaction with `SELECT ... FOR UPDATE`/advisory lock on the user id.

### 03.2 Token-uniqueness check races with the insert and then crashes
`src/views.rs:116-129` checks `SELECT EXISTS(... WHERE token = $1)` and returns 409 — but a concurrent insert between the check and views.rs:131-145 hits the `token UNIQUE` constraint (`migrations/20260116092854_initial_schema.sql:7`), and the `.unwrap()` on the INSERT panics → process abort (Spec 02.1).

**Fix:** Drop the pre-check; attempt the INSERT and map `sqlx::Error::Database` with code `23505` (unique_violation) to `409 CONFLICT`.

### 03.3 `402 Payment Required` for the bot limit
`src/views.rs:113`: `StatusCode::PAYMENT_REQUIRED` for "too many bots" is non-standard and undocumented; clients treat it as an opaque error. If it intentionally means "pay to raise the limit", document it; otherwise 409 or 422 with a JSON error body is clearer.

**Fix:** Document the 402 contract with book_bot (`book_bot/src/bots/bots_manager/register.rs` posts here) or switch to 409 + machine-readable error code, coordinating with the consumer.

### 03.4 `created_time` set from app-local clock
`src/views.rs:141`: `chrono::Local::now()` — depends on container TZ and app clock; the column is `TIMESTAMPTZ` so it works, but DB-side `now()` is more consistent.

**Fix:** `DEFAULT now()` in schema (Spec 06) and drop the parameter.

## Acceptance criteria
- Concurrency test: N parallel creates for one user never exceed 5 rows; extras get the documented limit status.
- Parallel creates with the same token: exactly one 200, others 409; server never crashes.
- Limit-exceeded status code documented in the route table and matched by book_bot.
