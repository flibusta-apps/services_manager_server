# Spec 09: Performance hygiene — pool timeouts, per-request log volume

- **Priority:** low
- **Effort:** S
- **Category:** performance

This is a small internal service (~366 LOC) with modest traffic; Spec 04 (single-connection pool) is the real capacity issue. Two hygiene items remain.

## Problem(s)

### 09.1 Pool has no explicit acquire timeout or lifetime

`src/db.rs:25-29`: `PgPoolOptions::new().max_connections(1).connect(...)` relies on sqlx defaults (30 s acquire timeout, unbounded lifetime). With Spec 04's single connection, any slow query queues every other handler behind a 30-second wait; stale connections are never recycled after a Postgres restart until they fail mid-request.

**Fix:** When implementing Spec 04's pool-size increase, also set `.acquire_timeout(Duration::from_secs(5))` and `.max_lifetime(Duration::from_secs(1800))`, both env-tunable with sane defaults.

### 09.2 Every successful request logged at INFO by TraceLayer

`src/views.rs:245-247`: `TraceLayer` emits per-request INFO lines. book_bot polls this service (service lists / token lookups), so steady-state logs are dominated by identical 200-OK lines — log I/O and noise that buries the ERROR lines Spec 08 cares about.

**Fix:** Configure the layer with `DefaultOnResponse::new().level(Level::DEBUG)` (keep failures at WARN/ERROR via `DefaultOnFailure`), or filter the access-log target in the subscriber env filter.

### 09.3 `GET /` returns the full unpaginated table (note, not a defect today)

`src/views.rs:33-45`: all services are loaded and serialized at once. The realistic row count (tens of bot services) makes this a non-issue now; recorded here only so a future "list all" consumer with thousands of rows knows pagination is absent by design, not oversight.

## Acceptance criteria

- Pool configured with explicit acquire timeout ≤ 5 s and max lifetime; a held-open transaction on the single connection causes concurrent requests to fail within the timeout, not queue for 30 s.
- Steady-state logs at INFO contain no per-request access lines; 4xx/5xx still logged.
