# services_manager_server — Audit Specs Index

Audit date: 2026-07-07. Scope: `src/`, `Cargo.toml`, `migrations/`, `docker/`, `scripts/`, `.github/`, plus the book_bot consumer contract. This service stores live Telegram bot tokens — treat Specs 01 and 07 as the top of the queue.

| Spec | Title | Priority | Effort | Category |
|------|-------|----------|--------|----------|
| [01](01-bot-token-protection.md) | Telegram bot tokens stored and served in plaintext | high | M | security |
| [02](02-panic-error-handling.md) | Panic-driven error handling and error-swallowing fallbacks | high | M | reliability |
| [03](03-create-service-races.md) | create_service — TOCTOU races and odd status semantics | high | S | correctness |
| [04](04-db-pool-single-connection.md) | Database pool limited to a single connection | medium | S | reliability |
| [05](05-input-validation.md) | No input validation — column overflows become 500s/crashes | medium | S | correctness |
| [06](06-schema-hardening.md) | Schema hardening — missing index, missing CHECKs, dead SQL | medium | S | correctness |
| [07](07-security-auth-secrets.md) | Committed DB credentials, API key comparison, open metrics | high | S | security |
| [08](08-delivery-observability.md) | Root container, unused curl, no tests, no readiness probe | medium | M | delivery |
| [09](09-performance-pool-logging.md) | Performance hygiene — pool timeouts, per-request log volume | low | S | performance |

Suggested order: 07.1 (rotate creds / isolate DB) → 01 → 02 → 03 → 04/05 → 06 → 08.
