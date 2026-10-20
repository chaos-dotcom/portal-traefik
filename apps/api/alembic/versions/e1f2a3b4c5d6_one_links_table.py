"""one links table; link presets; brand profiles; expiry reminders; drop destinations

Upload and download links shared most of their columns, and the admin UI already shows them as
one list, so they move into a single `links` table with a `kind` ('upload' | 'download'). Columns
only one kind uses stay (folder/source, file limits/download limits), so every query only gains a
kind filter. Rows keep their ids, so upload_sessions/download_sessions only re-point their foreign
keys. Tokens were random per table; one unique index now covers both.

Also new:
- links.brand_profile_id → brand_profiles: a link can carry a named brand (name, accent, logo)
  instead of the app-wide one.
- links.notify_emails: who gets this link's notification emails (null = the app's notify address).
- links.remind_before_expiry / expiry_reminded_at: an optional email a day before a link expires.
- link_presets: presets the admin saves from the link composer.

Destinations were folded into links by d0e1f2a3b4c5; the table and upload_links.destination_id
go away now.

Revision ID: e1f2a3b4c5d6
Revises: d0e1f2a3b4c5
Create Date: 2026-10-20 00:00:00.000000

"""
from collections.abc import Sequence

from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql


# revision identifiers, used by Alembic.
revision: str = 'e1f2a3b4c5d6'
down_revision: str | None = 'd0e1f2a3b4c5'
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def _timestamps() -> list[sa.Column]:
    return [
        sa.Column('created_at', sa.DateTime(timezone=True), server_default=sa.text('now()'),
                  nullable=False),
        sa.Column('updated_at', sa.DateTime(timezone=True), server_default=sa.text('now()'),
                  nullable=False),
    ]


def upgrade() -> None:
    op.create_table(
        'brand_profiles',
        sa.Column('id', sa.UUID(), server_default=sa.text('gen_random_uuid()'), nullable=False),
        sa.Column('name', sa.String(length=255), nullable=False),
        sa.Column('display_name', sa.String(length=255), nullable=True),
        sa.Column('accent_color', sa.String(length=32), nullable=True),
        sa.Column('logo_png', sa.LargeBinary(), nullable=True),
        sa.Column('logo_content_type', sa.String(length=64), nullable=True),
        sa.Column('logo_updated_at', sa.DateTime(timezone=True), nullable=True),
        *_timestamps(),
        sa.PrimaryKeyConstraint('id'),
    )
    op.create_table(
        'link_presets',
        sa.Column('id', sa.UUID(), server_default=sa.text('gen_random_uuid()'), nullable=False),
        sa.Column('name', sa.String(length=255), nullable=False),
        sa.Column('kind', sa.String(length=16), nullable=False),
        sa.Column('settings', postgresql.JSONB(astext_type=sa.Text()),
                  server_default=sa.text("'{}'::jsonb"), nullable=False),
        *_timestamps(),
        sa.CheckConstraint("kind IN ('upload', 'download')", name='link_presets_kind_check'),
        sa.PrimaryKeyConstraint('id'),
    )
    op.create_table(
        'links',
        sa.Column('id', sa.UUID(), server_default=sa.text('gen_random_uuid()'), nullable=False),
        sa.Column('kind', sa.String(length=16), nullable=False),
        sa.Column('token', sa.String(length=64), nullable=False),
        sa.Column('name', sa.String(length=255), nullable=True),
        sa.Column('expires_at', sa.DateTime(timezone=True), nullable=True),
        sa.Column('password_hash', sa.String(length=255), nullable=True),
        sa.Column('revoked_at', sa.DateTime(timezone=True), nullable=True),
        sa.Column('verify_email', sa.Boolean(), server_default=sa.text('false'), nullable=False),
        sa.Column('brand_display_name', sa.String(length=255), nullable=True),
        sa.Column('brand_subtitle', sa.Text(), nullable=True),
        sa.Column('brand_profile_id', sa.UUID(), nullable=True),
        sa.Column('notify_emails', postgresql.JSONB(astext_type=sa.Text()), nullable=True),
        sa.Column('remind_before_expiry', sa.Boolean(), server_default=sa.text('false'),
                  nullable=False),
        sa.Column('expiry_reminded_at', sa.DateTime(timezone=True), nullable=True),
        # upload links
        sa.Column('folder', postgresql.JSONB(astext_type=sa.Text()), nullable=True),
        sa.Column('max_file_size', sa.BigInteger(), nullable=True),
        sa.Column('allowed_extensions', postgresql.JSONB(astext_type=sa.Text()), nullable=True),
        sa.Column('uploader_fields_required', postgresql.JSONB(astext_type=sa.Text()),
                  nullable=True),
        sa.Column('target_folder_id', sa.String(length=255), nullable=True),
        sa.Column('target_folder_name', sa.String(length=255), nullable=True),
        sa.Column('subfolder_template', sa.Text(), nullable=True),
        # download links
        sa.Column('source', postgresql.JSONB(astext_type=sa.Text()), nullable=True),
        sa.Column('max_downloads', sa.Integer(), nullable=True),
        sa.Column('downloads_count', sa.Integer(), server_default=sa.text('0'), nullable=False),
        sa.Column('viewer_fields_required', postgresql.JSONB(astext_type=sa.Text()),
                  nullable=True),
        sa.Column('allow_preview', sa.Boolean(), server_default=sa.text('true'), nullable=False),
        *_timestamps(),
        sa.CheckConstraint("kind IN ('upload', 'download')", name='links_kind_check'),
        sa.CheckConstraint("kind <> 'download' OR source IS NOT NULL",
                           name='links_download_source_check'),
        sa.ForeignKeyConstraint(['brand_profile_id'], ['brand_profiles.id'], ondelete='SET NULL'),
        sa.PrimaryKeyConstraint('id'),
    )
    op.create_index('ix_links_token', 'links', ['token'], unique=True)

    op.execute(
        """
        INSERT INTO links (id, kind, token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, folder, max_file_size,
            allowed_extensions, uploader_fields_required, target_folder_id, target_folder_name,
            subfolder_template, created_at, updated_at)
        SELECT u.id, 'upload', u.token, u.name, u.expires_at, u.password_hash, u.revoked_at,
            u.verify_email, u.brand_display_name, u.brand_subtitle,
            COALESCE(u.folder, d.config), u.max_file_size, u.allowed_extensions,
            u.uploader_fields_required, u.target_folder_id, u.target_folder_name,
            u.subfolder_template, u.created_at, u.updated_at
          FROM upload_links u LEFT JOIN destinations d ON d.id = u.destination_id
        """
    )
    op.execute(
        """
        INSERT INTO links (id, kind, token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, source, max_downloads,
            downloads_count, viewer_fields_required, allow_preview, created_at, updated_at)
        SELECT id, 'download', token, name, expires_at, password_hash, revoked_at,
            verify_email, brand_display_name, brand_subtitle, source, max_downloads,
            downloads_count, viewer_fields_required, allow_preview, created_at, updated_at
          FROM download_links
        """
    )

    op.drop_constraint('upload_sessions_upload_link_id_fkey', 'upload_sessions',
                       type_='foreignkey')
    op.create_foreign_key('upload_sessions_upload_link_id_fkey', 'upload_sessions', 'links',
                          ['upload_link_id'], ['id'], ondelete='CASCADE')
    op.drop_constraint('download_sessions_download_link_id_fkey', 'download_sessions',
                       type_='foreignkey')
    op.create_foreign_key('download_sessions_download_link_id_fkey', 'download_sessions', 'links',
                          ['download_link_id'], ['id'], ondelete='CASCADE')

    op.drop_table('upload_links')
    op.drop_table('download_links')
    op.drop_table('destinations')


def downgrade() -> None:
    # Back to two link tables. Destinations come back empty: links kept their folders.
    op.create_table(
        'destinations',
        sa.Column('id', sa.UUID(), server_default=sa.text('gen_random_uuid()'), nullable=False),
        sa.Column('display_name', sa.String(length=255), nullable=False),
        sa.Column('config', postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column('subtitle', sa.Text(), nullable=True),
        *_timestamps(),
        sa.PrimaryKeyConstraint('id'),
    )
    op.execute(
        """
        CREATE TABLE upload_links AS
        SELECT id, token, NULL::uuid AS destination_id, expires_at, password_hash, max_file_size,
               allowed_extensions, uploader_fields_required, revoked_at, brand_display_name,
               brand_subtitle, created_at, updated_at, verify_email, target_folder_id,
               target_folder_name, subfolder_template, name, folder
          FROM links WHERE kind = 'upload'
        """
    )
    op.execute(
        """
        CREATE TABLE download_links AS
        SELECT id, token, source, expires_at, password_hash, max_downloads, downloads_count,
               viewer_fields_required, revoked_at, allow_preview, brand_display_name,
               brand_subtitle, created_at, updated_at, verify_email, name
          FROM links WHERE kind = 'download'
        """
    )
    for table in ('upload_links', 'download_links'):
        op.create_primary_key(f'{table}_pkey', table, ['id'])
        op.create_index(f'ix_{table}_token', table, ['token'], unique=True)
        op.alter_column(table, 'created_at', server_default=sa.text('now()'))
        op.alter_column(table, 'updated_at', server_default=sa.text('now()'))
    op.alter_column('upload_links', 'verify_email', server_default=sa.text('false'))
    op.alter_column('download_links', 'verify_email', server_default=sa.text('false'))
    op.create_index('ix_upload_links_destination_id', 'upload_links', ['destination_id'])
    op.create_foreign_key('upload_links_destination_id_fkey', 'upload_links', 'destinations',
                          ['destination_id'], ['id'], ondelete='SET NULL')

    op.drop_constraint('upload_sessions_upload_link_id_fkey', 'upload_sessions',
                       type_='foreignkey')
    op.create_foreign_key('upload_sessions_upload_link_id_fkey', 'upload_sessions',
                          'upload_links', ['upload_link_id'], ['id'], ondelete='CASCADE')
    op.drop_constraint('download_sessions_download_link_id_fkey', 'download_sessions',
                       type_='foreignkey')
    op.create_foreign_key('download_sessions_download_link_id_fkey', 'download_sessions',
                          'download_links', ['download_link_id'], ['id'], ondelete='CASCADE')

    op.drop_index('ix_links_token', table_name='links')
    op.drop_table('links')
    op.drop_table('link_presets')
    op.drop_table('brand_profiles')
