# Spec 08: Delivery and observability — root container, unused curl, no tests, no readiness

- **Priority:** medium
- **Effort:** M
- **Category:** delivery

## Problem(s)

### 08.1 Container runs as root; curl installed but no HEALTHCHECK defined
`docker/build.dockerfile:10-24`: no `USER` directive (runs as root); line 13 installs `curl` — evidently intended for a healthcheck — yet there is no `HEALTHCHECK` instruction, and the app's `GET /health` (`src/views.rs:196-198,235`) goes unused by the image.

**Fix:** Add a non-root user and `HEALTHCHECK --interval=30s CMD curl -f http://localhost:8080/health || exit 1` (or drop curl and rely on documented orchestrator probes).

### 08.2 `/health` ignores the database; Sentry DSN mandatory
`src/views.rs:196-198` returns 200 unconditionally — with the single-connection pool (Spec 04) a dead DB leaves the service green while every request aborts the process. `src/config.rs:30` + `src/main.rs:32` make a valid `SENTRY_DSN` a startup requirement (`Dsn::from_str(...).unwrap()`), blocking local/dev runs.

**Fix:** Add `/ready` doing `SELECT 1` (503 on failure) for the orchestrator probe; make `SENTRY_DSN` optional.

### 08.3 CI never tests; clippy is non-blocking; deploy fires on every main push
`.github/workflows/build_docker_image.yml:30-45` builds, pushes `:latest` and hits the deployment webhook with no test gate; `.github/workflows/rust-clippy.yml:49` has `continue-on-error: true`. The repository contains zero tests (no `#[test]`, no `tests/`), so races (Spec 03) and validation gaps (Spec 05) ship unchecked.

**Fix:** Add a gating CI job: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` with `SQLX_OFFLINE=true` plus integration tests against a `services: postgres` container; make the Docker job depend on it. Add a `.dockerignore` and cargo layer caching (`COPY . .` at `docker/build.dockerfile:5` recompiles everything each push).

### 08.4 Hardcoded bind address, no graceful shutdown, needless async
`src/main.rs:21`: fixed `0.0.0.0:8080`; `axum::serve` (main.rs:25) has no `with_graceful_shutdown`, so deploys drop in-flight requests. Minor: `get_router` is `async` (`src/views.rs:221`) but contains no `.await` — dead asyncness.

**Fix:** Read `PORT` from env; add SIGTERM handler; make `get_router` sync.

## Acceptance criteria
- Image runs as non-root and reports container health via the app's endpoint.
- `/ready` flips to 503 when Postgres is down; service starts without `SENTRY_DSN`.
- Failing tests or clippy warnings block image build and the deploy webhook.
- SIGTERM completes in-flight requests before exit.
