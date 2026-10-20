-- portal-migration: b8c9d0e1f2a3 -> c9d0e1f2a3b4
BEGIN;

-- Running upgrade b8c9d0e1f2a3 -> c9d0e1f2a3b4

ALTER TABLE download_events ADD COLUMN ip VARCHAR(64);

ALTER TABLE download_events ADD COLUMN user_agent TEXT;

UPDATE alembic_version SET version_num='c9d0e1f2a3b4' WHERE alembic_version.version_num = 'b8c9d0e1f2a3';

COMMIT;

