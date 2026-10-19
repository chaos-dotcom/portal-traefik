#!/bin/sh
# Check that apps/api-rs/schema.sql is exactly what Alembic produces at head, stamp included.
#
#   tools/rustify/schema_check.sh            # compare; exit 1 on drift
#   tools/rustify/schema_check.sh --write    # regenerate schema.sql from Alembic
#
# The Rust image has no migration runner, so a new Alembic migration must be reflected here.
# Runs Alembic (via uv) against a throwaway postgres:16 container on a random loopback port.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
schema="$root/apps/api-rs/schema.sql"
name="portal-schema-check-$$"

docker run -d --rm --name "$name" -p 127.0.0.1::5432 \
	-e POSTGRES_USER=portal -e POSTGRES_PASSWORD=portal -e POSTGRES_DB=portal postgres:16 >/dev/null
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
port=$(docker port "$name" 5432/tcp | head -1 | sed 's/.*://')
until docker exec "$name" pg_isready -U portal -d portal >/dev/null 2>&1; do sleep 0.5; done

(
	cd "$root/apps/api"
	DATABASE_URL="postgresql+asyncpg://portal:portal@127.0.0.1:$port/portal" \
		SESSION_SECRET=schema-check TOKEN_ENCRYPTION_KEY=schema-check \
		uv run --quiet alembic upgrade head >/dev/null 2>&1
)
head=$(docker exec "$name" psql -U portal -d portal -tAc "SELECT version_num FROM alembic_version" | tr -d '[:space:]')

generated=$(mktemp)
trap 'rm -f "$generated" "$generated.want" "$generated.got"; docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
{
	docker exec "$name" pg_dump -U portal -d portal --schema-only --no-owner --no-privileges
	cat <<EOF

--
-- Alembic revision this schema corresponds to. A database created from this file is stamped with
-- it, so the Python service (and Alembic) see a normal, up-to-date database; the Rust entrypoint
-- refuses any database at another revision (see docker/container/rust/portal-entrypoint.sh).
-- Keep it in step with the head of apps/api/alembic/versions — tools/rustify/schema_check.sh
-- regenerates this file from Alembic and fails if they drift.
--
INSERT INTO public.alembic_version (version_num) VALUES ('$head');
EOF
} >"$generated"

# Ignore what varies between dumps of the same schema: the \restrict/\unrestrict lines carrying a
# random key, and the server/pg_dump version banner.
if [ "${1:-}" = "--write" ]; then
	cp "$generated" "$schema"
	echo "wrote $schema (revision $head)"
	exit 0
fi
stable() { grep -v -e '^\\\(un\)\?restrict ' -e '^-- Dumped \(from database\|by pg_dump\) version ' "$1"; }
stable "$schema" >"$generated.want"
stable "$generated" >"$generated.got"
if diff -u "$generated.want" "$generated.got"; then
	echo "schema.sql matches Alembic head ($head)"
else
	echo "schema.sql has drifted from Alembic head ($head) — run: $0 --write" >&2
	exit 1
fi
