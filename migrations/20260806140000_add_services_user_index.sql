-- Spec 06.1: index on "user" — create_service runs
-- `SELECT COUNT(*) FROM services WHERE "user" = $1` on every POST
-- (src/views.rs) and previously had no supporting index.

CREATE INDEX IF NOT EXISTS services_user_idx ON services ("user");
