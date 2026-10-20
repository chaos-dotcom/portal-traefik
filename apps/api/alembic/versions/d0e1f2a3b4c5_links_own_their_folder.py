"""links own their folder — fold destinations into upload links; names; sync job file names

A destination was a named bookmark of a Frame.io folder (display_name, subtitle, config) that only
upload links used. Links now carry the folder themselves (`upload_links.folder`, the same JSONB
shape as `destinations.config`) and an internal `name` separate from the client-facing title, so
the admin UI can drop destinations as a concept.

The backfill keeps every client page identical: the public page used to fall back to the
destination's display name and subtitle, so those are copied onto the link where it had none. A
link's base subfolder (`target_folder_id`) becomes the folder itself and is cleared; its breadcrumb
gets the subfolder appended (the levels in between are unknown here and fill in from Frame.io
lazily).

`destination_id` becomes optional and stops cascading: links created from now on have none, and
removing a destination must never take links with it. The destinations table stays (nothing writes
to it any more) so `downgrade()` can bring the old shape back; it drops links that never had a
destination.

`sync_jobs.file_name` records the file a job is about, so the admin can show names, not IDs.

Revision ID: d0e1f2a3b4c5
Revises: c9d0e1f2a3b4
Create Date: 2026-10-19 00:00:00.000000

"""
from collections.abc import Sequence

from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql


# revision identifiers, used by Alembic.
revision: str = 'd0e1f2a3b4c5'
down_revision: str | None = 'c9d0e1f2a3b4'
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    op.add_column('upload_links', sa.Column('name', sa.String(length=255), nullable=True))
    op.add_column(
        'upload_links',
        sa.Column('folder', postgresql.JSONB(astext_type=sa.Text()), nullable=True),
    )
    op.add_column('download_links', sa.Column('name', sa.String(length=255), nullable=True))
    op.add_column('sync_jobs', sa.Column('file_name', sa.String(length=512), nullable=True))

    op.alter_column('upload_links', 'destination_id', existing_type=sa.UUID(), nullable=True)
    op.drop_constraint('upload_links_destination_id_fkey', 'upload_links', type_='foreignkey')
    op.create_foreign_key(
        'upload_links_destination_id_fkey', 'upload_links', 'destinations',
        ['destination_id'], ['id'], ondelete='SET NULL',
    )

    op.execute(
        """
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
        WHERE d.id = u.destination_id
        """
    )
    op.execute("UPDATE download_links SET name = brand_display_name WHERE name IS NULL")


def downgrade() -> None:
    op.drop_constraint('upload_links_destination_id_fkey', 'upload_links', type_='foreignkey')
    op.create_foreign_key(
        'upload_links_destination_id_fkey', 'upload_links', 'destinations',
        ['destination_id'], ['id'], ondelete='CASCADE',
    )
    op.execute("DELETE FROM upload_links WHERE destination_id IS NULL")
    op.alter_column('upload_links', 'destination_id', existing_type=sa.UUID(), nullable=False)
    op.drop_column('sync_jobs', 'file_name')
    op.drop_column('download_links', 'name')
    op.drop_column('upload_links', 'folder')
    op.drop_column('upload_links', 'name')
