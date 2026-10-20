-- portal-migration: c9d0e1f2a3b4 -> d0e1f2a3b4c5
BEGIN;

-- Running upgrade c9d0e1f2a3b4 -> d0e1f2a3b4c5

ALTER TABLE upload_links ADD COLUMN name VARCHAR(255);

ALTER TABLE upload_links ADD COLUMN folder JSONB;

ALTER TABLE download_links ADD COLUMN name VARCHAR(255);

ALTER TABLE sync_jobs ADD COLUMN file_name VARCHAR(512);

ALTER TABLE upload_links ALTER COLUMN destination_id DROP NOT NULL;

ALTER TABLE upload_links DROP CONSTRAINT upload_links_destination_id_fkey;

ALTER TABLE upload_links ADD CONSTRAINT upload_links_destination_id_fkey FOREIGN KEY(destination_id) REFERENCES destinations (id) ON DELETE SET NULL;

UPDATE upload_links u SET
          folder = CASE
            WHEN u.target_folder_id IS NULL THEN d.config
            ELSE (d.config || jsonb_build_object(
              'folder_id', u.target_folder_id,
              'folder_name', COALESCE(u.target_folder_name, u.target_folder_id),
              'path', COALESCE(d.config->'path', '[]'::jsonb) || jsonb_build_array(
                jsonb_build_object('id', u.target_folder_id,
                                   'name', COALESCE(u.target_folder_name, u.target_folder_id)))))
              - 'path_resolved_at'
          END,
          target_folder_id = NULL,
          target_folder_name = NULL,
          name = COALESCE(NULLIF(u.brand_display_name, ''), d.display_name),
          brand_display_name = COALESCE(NULLIF(u.brand_display_name, ''), d.display_name),
          brand_subtitle = COALESCE(NULLIF(u.brand_subtitle, ''), d.subtitle)
        FROM destinations d
        WHERE d.id = u.destination_id;

UPDATE download_links SET name = brand_display_name WHERE name IS NULL;

UPDATE alembic_version SET version_num='d0e1f2a3b4c5' WHERE alembic_version.version_num = 'c9d0e1f2a3b4';

COMMIT;

