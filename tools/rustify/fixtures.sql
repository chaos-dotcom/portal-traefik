
-- Deterministic fixtures for the byte-for-byte differential. Fixed ids and timestamps so
-- responses that embed them are reproducible. Applied identically to both services.
BEGIN;

-- Global branding (single row).
DELETE FROM app_settings;
INSERT INTO app_settings (id, brand_display_name, brand_accent_color, config_seeded, created_at, updated_at)
VALUES ('00000000-0000-4000-8000-000000000001', 'Acme Post', '#ff8800', true,
        '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00');

-- One destination with a cached Frame.io breadcrumb under config.path.
DELETE FROM download_links;
DELETE FROM upload_links;
DELETE FROM destinations;
DELETE FROM frameio_connections;
INSERT INTO destinations (id, display_name, config, subtitle, created_at, updated_at)
VALUES ('00000000-0000-4000-8000-000000000010', 'Acme Deliveries',
        '{"type":"frameio","account_id":"acct-fixed","workspace_id":"ws-fixed","project_id":"proj-fixed","folder_id":"fold-fixed","path":[{"id":"proj-fixed","name":"Acme Project"},{"id":"fold-fixed","name":"Incoming"}]}'::jsonb,
        'Send your rushes here',
        '2026-01-02T09:00:00+00:00', '2026-01-02T09:00:00+00:00');

-- Two upload links, one of each state, with fixed timestamps and no password.
INSERT INTO upload_links (id, token, destination_id, expires_at, password_hash, max_file_size,
                          allowed_extensions, uploader_fields_required, revoked_at,
                          brand_display_name, brand_subtitle, verify_email, target_folder_id,
                          target_folder_name, subfolder_template, created_at, updated_at)
VALUES
 ('00000000-0000-4000-8000-000000000100', 'up-fixed-token-aaaaaaaaaaaaaaaa', '00000000-0000-4000-8000-000000000010',
  NULL, NULL, 4294967296, '["mov","mxf","wav"]'::jsonb, '{"name":true,"email":false,"message":false}'::jsonb,
  NULL, 'Acme Rushes', 'Drop your camera files', false, NULL, NULL, NULL,
  '2026-02-01T10:00:00+00:00', '2026-02-01T10:00:00+00:00'),
 ('00000000-0000-4000-8000-000000000101', 'up-fixed-token-bbbbbbbbbbbbbbbb', '00000000-0000-4000-8000-000000000010',
  '2026-01-15T00:00:00+00:00', NULL, NULL, NULL, '{"name":false,"email":true,"message":false}'::jsonb,
  NULL, NULL, NULL, false, NULL, NULL, NULL,
  '2026-01-05T08:30:00+00:00', '2026-01-05T08:30:00+00:00');
-- One download link (single-file source) so the download list has a deterministic row.
INSERT INTO download_links (id, token, source, expires_at, password_hash, max_downloads,
                            downloads_count, viewer_fields_required, revoked_at, allow_preview,
                            brand_display_name, brand_subtitle, verify_email, created_at, updated_at)
VALUES ('00000000-0000-4000-8000-000000000200', 'dl-fixed-token-cccccccccccccccc',
        '{"type":"file","file_id":"file-fixed-1","name":"A001_C001.mxf"}'::jsonb,
        NULL, NULL, 5, 2, '{"name":true,"email":true}'::jsonb, NULL, true,
        'Acme Review', 'Watch the cut', false,
        '2026-02-03T11:00:00+00:00', '2026-02-03T11:00:00+00:00');
COMMIT;
