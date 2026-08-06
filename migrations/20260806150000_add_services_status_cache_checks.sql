-- Spec 06.2: CHECK constraints on status/cache plus a DB-side default for
-- created_time, matching the application-level allow-lists already enforced
-- in src/views.rs (ALLOWED_STATUSES, ALLOWED_CACHE_VALUES), which mirror
-- book_bot's registration status and BotCache enum.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_name = 'services' AND constraint_name = 'services_status_check'
    ) THEN
        ALTER TABLE services
            ADD CONSTRAINT services_status_check CHECK (status IN ('approved'));
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_name = 'services' AND constraint_name = 'services_cache_check'
    ) THEN
        ALTER TABLE services
            ADD CONSTRAINT services_cache_check CHECK (cache IN ('original', 'cache', 'no_cache'));
    END IF;
END
$$;

ALTER TABLE services ALTER COLUMN created_time SET DEFAULT now();
