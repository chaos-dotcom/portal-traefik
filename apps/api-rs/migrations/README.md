# Schema migrations for the Rust image

The schema is authored with Alembic (`apps/api/alembic`). The Rust image can't run Alembic, so it
ships the result as SQL, which `portal-api migrate` applies when the container starts:

- `../schema.sql` — the full schema at the Alembic head, stamped with its revision. Applied to an
  empty database.
- `<to>.sql` here — one file per revision step after the oldest revision the image can upgrade,
  applied in order to a database at an older revision.

A database at a revision with no path to the head is refused at startup.

## Adding a migration

1. Write the Alembic migration in `apps/api/alembic/versions/` as usual (`from` → `to`).
2. Generate its SQL and start the file with a header naming the step:

   ```sh
   cd apps/api
   { echo "-- portal-migration: <from> -> <to>"; uv run alembic upgrade <from>:<to> --sql; } \
     > ../api-rs/migrations/<to>.sql
   ```

   Review it: it's what runs against every deployment.
3. Register it at the end of `MIGRATIONS` in `src/migrate.rs`:
   `Migration { from: "<from>", to: "<to>", sql: include_str!("../migrations/<to>.sql") },`
4. Refresh the snapshot: `tools/rustify/schema_check.sh --write`.

`tools/rustify/schema_check.sh` (run in CI) fails when `schema.sql` differs from the Alembic head,
when applying these files to a database at the oldest revision doesn't reproduce it, or when a
file here isn't registered in `MIGRATIONS`.
