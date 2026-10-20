-- portal-migration: d0e1f2a3b4c5 -> e1f2a3b4c5d6
BEGIN;

-- Running upgrade d0e1f2a3b4c5 -> e1f2a3b4c5d6

CREATE TABLE brand_profiles (
    id UUID DEFAULT gen_random_uuid() NOT NULL, 
    name VARCHAR(255) NOT NULL, 
    display_name VARCHAR(255), 
    accent_color VARCHAR(32), 
    logo_png BYTEA, 
    logo_content_type VARCHAR(64), 
    logo_updated_at TIMESTAMP WITH TIME ZONE, 
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    PRIMARY KEY (id)
);

CREATE TABLE link_presets (
    id UUID DEFAULT gen_random_uuid() NOT NULL, 
    name VARCHAR(255) NOT NULL, 
    kind VARCHAR(16) NOT NULL, 
    settings JSONB DEFAULT '{}'::jsonb NOT NULL, 
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    PRIMARY KEY (id), 
    CONSTRAINT link_presets_kind_check CHECK (kind IN ('upload', 'download'))
);

CREATE TABLE links (
    id UUID DEFAULT gen_random_uuid() NOT NULL, 
    kind VARCHAR(16) NOT NULL, 
    token VARCHAR(64) NOT NULL, 
    name VARCHAR(255), 
    expires_at TIMESTAMP WITH TIME ZONE, 
    password_hash VARCHAR(255), 
    revoked_at TIMESTAMP WITH TIME ZONE, 
    verify_email BOOLEAN DEFAULT false NOT NULL, 
    brand_display_name VARCHAR(255), 
    brand_subtitle TEXT, 
    brand_profile_id UUID, 
    notify_emails JSONB, 
    remind_before_expiry BOOLEAN DEFAULT false NOT NULL, 
    expiry_reminded_at TIMESTAMP WITH TIME ZONE, 
    folder JSONB, 
    max_file_size BIGINT, 
    allowed_extensions JSONB, 
    uploader_fields_required JSONB, 
    target_folder_id VARCHAR(255), 
    target_folder_name VARCHAR(255), 
    subfolder_template TEXT, 
    source JSONB, 
    max_downloads INTEGER, 
    downloads_count INTEGER DEFAULT 0 NOT NULL, 
    viewer_fields_required JSONB, 
    allow_preview BOOLEAN DEFAULT true NOT NULL, 
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL, 
    PRIMARY KEY (id), 
    CONSTRAINT links_kind_check CHECK (kind IN ('upload', 'download')), 
    CONSTRAINT links_download_source_check CHECK (kind <> 'download' OR source IS NOT NULL), 
    FOREIGN KEY(brand_profile_id) REFERENCES brand_profiles (id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX ix_links_token ON links (token);

INSERT INTO links (id, kind, token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, folder, max_file_size,
            allowed_extensions, uploader_fields_required, target_folder_id, target_folder_name,
            subfolder_template, created_at, updated_at)
        SELECT u.id, 'upload', u.token, u.name, u.expires_at, u.password_hash, u.revoked_at,
            u.verify_email, u.brand_display_name, u.brand_subtitle,
            COALESCE(u.folder, d.config), u.max_file_size, u.allowed_extensions,
            u.uploader_fields_required, u.target_folder_id, u.target_folder_name,
            u.subfolder_template, u.created_at, u.updated_at
          FROM upload_links u LEFT JOIN destinations d ON d.id = u.destination_id;

INSERT INTO links (id, kind, token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, source, max_downloads,
            downloads_count, viewer_fields_required, allow_preview, created_at, updated_at)
        SELECT id, 'download', token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, source, max_downloads,
            downloads_count, viewer_fields_required, allow_preview, created_at, updated_at
          FROM download_links;

ALTER TABLE upload_sessions DROP CONSTRAINT upload_sessions_upload_link_id_fkey;

ALTER TABLE upload_sessions ADD CONSTRAINT upload_sessions_upload_link_id_fkey FOREIGN KEY(upload_link_id) REFERENCES links (id) ON DELETE CASCADE;

ALTER TABLE download_sessions DROP CONSTRAINT download_sessions_download_link_id_fkey;

ALTER TABLE download_sessions ADD CONSTRAINT download_sessions_download_link_id_fkey FOREIGN KEY(download_link_id) REFERENCES links (id) ON DELETE CASCADE;

DROP TABLE upload_links;

DROP TABLE download_links;

DROP TABLE destinations;

UPDATE alembic_version SET version_num='e1f2a3b4c5d6' WHERE alembic_version.version_num = 'd0e1f2a3b4c5';

COMMIT;

