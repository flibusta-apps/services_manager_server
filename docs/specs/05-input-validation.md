# Spec 05: No input validation — column overflows become 500s/crashes

- **Priority:** medium
- **Effort:** S
- **Category:** correctness

## Problem(s)

### 05.1 `CreateServiceData` accepts arbitrary strings for constrained columns
`src/views.rs:83-90` deserializes `token`, `status`, `cache`, `username` with no checks, while the schema (`migrations/20260116092854_initial_schema.sql:7-12`) constrains them to `VARCHAR(128)/(12)/(12)/(64)`. A `status` longer than 12 chars makes the INSERT (views.rs:131-145) fail with a value-too-long DB error → `.unwrap()` panic → process abort (Spec 02.1). The Telegram token format (`^\d+:[A-Za-z0-9_-]{35}$`) is also never validated, so garbage tokens are accepted and later crash bot startup in the consumer.

**Fix:** Validate lengths and token format in the handler; return 422 with a descriptive body.

### 05.2 `update_status`/`update_cache` take a raw JSON string with no allowed-values check
`src/views.rs:150-194`: `Json(state): Json<String>` / `Json(cache): Json<String>` write any string (up to the 12-char column limit, above which they panic as in 05.1). Status is clearly an enum in practice (`status VARCHAR(12)`), but nothing restricts it, so typos silently corrupt the registry.

**Fix:** Define the allowed status/cache values (with book_bot), validate against them (or a DB CHECK constraint, Spec 06.2), return 422 otherwise.

### 05.3 Route/handler naming mismatch
`src/views.rs:229`: route `/{id}/update_status` is served by `update_state` (views.rs:150) — cosmetic, but confusing when grepping.

**Fix:** Rename the handler to `update_status`.

## Acceptance criteria
- POST `/` with a 300-char `status` or a malformed token returns 422; server stays alive.
- PATCH `update_status`/`update_cache` reject values outside the documented set with 422.
- Allowed status/cache values documented in the repo.
