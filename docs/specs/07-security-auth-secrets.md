# Spec 07: Security — committed DB credentials, API key comparison, open metrics

- **Priority:** high
- **Effort:** S
- **Category:** security

## Problem(s)

### 07.1 Live database credentials in `.env`; no `.dockerignore`
`.env:1`:
```
DATABASE_URL="postgres://flibusta_bots_manager_server_user:<password>@kurbezz.me:54322/flibusta_bots_manager_server"
```
Real credentials for the database that stores all bot tokens (Spec 01), pointing at a **publicly resolvable host and port**. The file is gitignored (`.gitignore:3`) but present on developer machines, and there is no `.dockerignore`, so `COPY . .` (`docker/build.dockerfile:5`) pulls `.env` (and `target/`) into the build context/builder layer. sqlx macros also read `.env` at compile time — local and Docker builds prepare queries against the production DB instead of using the committed `.sqlx/` cache.

**Fix:** Rotate the password; restrict the DB to the internal network (it must not be reachable from the internet given Spec 01.1); add `.dockerignore` with `.env`, `target/`, `.git`; set `ENV SQLX_OFFLINE=true` in the builder stage.

### 07.2 API key compared with non-constant-time equality, no scheme
`src/views.rs:214`: `if auth_header != CONFIG.api_key` — plain equality on the raw `Authorization` header (no `Bearer`). One static key guards read access to all bot tokens, so hardening is cheap insurance.

**Fix:** Constant-time compare (`subtle::ConstantTimeEq`) after a length check; accept `Bearer <key>`; consider separate keys per consumer (see Spec 01.2).

### 07.3 `/metrics` mounted outside auth
`src/views.rs:237-238` exposes the Prometheus handle publicly (auth layer applies only to `app_router`, views.rs:231). Route templates and request volumes for a bot-token registry are mildly sensitive.

**Fix:** Protect `/metrics` with the API key, serve it on an internal-only listener, or document mandatory network isolation.

## Acceptance criteria
- Exposed DB password rotated; database no longer reachable from public networks.
- `.dockerignore` present; `docker build` succeeds offline with `SQLX_OFFLINE=true` and the context contains no `.env`.
- Auth accepts `Bearer <key>` and compares in constant time; `/metrics` not publicly reachable (or auth-protected).
