#!/bin/sh
# First-boot init for the all-in-one Portal container, then hand off to supervisord.
#
# Everything stateful lives under /data (a single mounted volume): the Postgres cluster, the
# Redis AOF, Caddy's data dir, and the generated secrets. A fresh /data is fully self-setting-up
# — no migration from a previous deploy is required.
set -e

# --- Deployment mode (derived) -----------------------------------------------------------------
# Operators set only their domain (BASE_URL) and, optionally, whether a host reverse proxy fronts
# Portal. Caddy's site address and the rate limiter's proxy-hop count are derived from those here
# and exported, so supervisord's children (caddy, api) inherit them — no duplicate domain/hop knobs.
#
#   PORTAL_BEHIND_PROXY=true  (default) — a host proxy terminates TLS and forwards to our loopback
#                              port. Caddy serves plain HTTP on :80; the real client IP is the
#                              2nd-from-last X-Forwarded-For hop (host proxy + this Caddy).
#   PORTAL_BEHIND_PROXY=false           — standalone: Caddy is the public TLS edge and provisions a
#                              Let's Encrypt cert for BASE_URL's host. One hop (this Caddy only).
#
# An explicit PORTAL_SITE_ADDRESS or TRUSTED_PROXY_HOPS still wins (power users, e.g. behind
# Cloudflare set TRUSTED_PROXY_HOPS=3). The bare host is parsed from BASE_URL (scheme + path
# stripped, any :port kept).
portal_host=$(printf '%s' "${BASE_URL:-}" | sed -E 's#^[a-zA-Z][a-zA-Z0-9+.-]*://##; s#/.*$##')
if [ "${PORTAL_BEHIND_PROXY:-true}" = "false" ]; then
	export PORTAL_SITE_ADDRESS="${PORTAL_SITE_ADDRESS:-${portal_host:-:80}}"
	export TRUSTED_PROXY_HOPS="${TRUSTED_PROXY_HOPS:-1}"
else
	export PORTAL_SITE_ADDRESS="${PORTAL_SITE_ADDRESS:-:80}"
	export TRUSTED_PROXY_HOPS="${TRUSTED_PROXY_HOPS:-2}"
fi

DATA=/data
PGDATA="$DATA/postgres"
SECRETS="$DATA/secrets"

mkdir -p "$PGDATA" "$DATA/redis" "$DATA/caddy" "$SECRETS"
chown postgres:postgres "$PGDATA"
chown redis:redis "$DATA/redis" 2>/dev/null || true
chmod 700 "$SECRETS"

# --- Secrets -----------------------------------------------------------------------------------
# For each crypto secret: an explicit env value wins (and is persisted); otherwise reuse the
# value persisted on a previous boot; otherwise generate one and persist it. This keeps cookies
# signable and stored Frame.io/SMTP tokens decryptable across restarts without the deployer
# having to manage these by hand. TOKEN_ENCRYPTION_KEY in particular must stay stable, which it
# does because it co-lives with the database it encrypts under /data.
gen() { head -c 32 /dev/urandom | base64 | tr "+/" "-_" | tr -d "="; }
for name in SESSION_SECRET TOKEN_ENCRYPTION_KEY; do
	file="$SECRETS/$name"
	cur=$(eval "printf '%s' \"\${$name:-}\"")
	if [ -n "$cur" ]; then
		printf '%s' "$cur" >"$file"
	elif [ -s "$file" ]; then
		export "$name=$(cat "$file")"
	else
		val=$(gen)
		printf '%s' "$val" >"$file"
		export "$name=$val"
	fi
	chmod 600 "$file"
done

# --- Postgres init + schema + seed ------------------------------------------------------------
# Local + 127.0.0.1 connections use trust auth: the cluster is reachable only from inside this
# container (Postgres binds loopback and the container publishes only :80), so no password is
# needed. We apply the schema (first boot), check its revision and run the admin seed HERE,
# against a temporary Postgres, before starting supervisord — so the supervised api and worker
# never race an unmigrated schema.
if [ ! -s "$PGDATA/PG_VERSION" ]; then
	echo "[entrypoint] initializing PostgreSQL data directory"
	su -s /bin/sh postgres -c "initdb -D '$PGDATA' -U postgres --auth-local=trust --auth-host=trust --encoding=UTF8"
fi

echo "[entrypoint] starting temporary Postgres for migrations"
su -s /bin/sh postgres -c "pg_ctl -D '$PGDATA' -o '-c listen_addresses=127.0.0.1' -w start"
# Create the portal role + database if they don't exist yet (first boot).
if ! su -s /bin/sh postgres -c "psql -h 127.0.0.1 -U postgres -d postgres -tAc \"SELECT 1 FROM pg_roles WHERE rolname='portal'\"" | grep -q 1; then
	su -s /bin/sh postgres -c "psql -h 127.0.0.1 -U postgres -d postgres -v ON_ERROR_STOP=1 \
		-c \"CREATE ROLE portal LOGIN;\" -c \"CREATE DATABASE portal OWNER portal;\""
fi

echo "[entrypoint] applying schema (first boot only) + seeding admin"
psql_portal() { su -s /bin/sh postgres -c "psql -h 127.0.0.1 -U portal -d portal -v ON_ERROR_STOP=1 $*"; }
if ! psql_portal "-tAc \"SELECT to_regclass('public.users')\"" | grep -q users; then
	# Apply as the portal role so it owns the objects (as Alembic would have).
	psql_portal "-q -f /app/schema.sql"
fi

# The Rust service has no migration runner: it ships one schema snapshot (schema.sql), stamped
# with the Alembic revision it matches. Refuse a database at any other revision rather than run
# against columns it doesn't expect — e.g. data from an older Python image (upgrade it with that
# image first, which migrates to head) or from a newer one (use a Rust image built from it).
expected_rev=$(sed -n "s/^INSERT INTO public.alembic_version (version_num) VALUES ('\([0-9a-f]*\)');$/\1/p" /app/schema.sql)
current_rev=$(psql_portal "-tAc \"SELECT version_num FROM alembic_version\"" | tr -d '[:space:]')
if [ -z "$current_rev" ]; then
	# Created by an earlier Rust image, before schema.sql carried its revision: same snapshot.
	echo "[entrypoint] stamping schema revision $expected_rev"
	psql_portal "-q -c \"INSERT INTO alembic_version (version_num) VALUES ('$expected_rev')\""
elif [ "$current_rev" != "$expected_rev" ]; then
	echo "[entrypoint] FATAL: the database is at schema revision $current_rev, but this image expects $expected_rev." >&2
	echo "[entrypoint] Migrate it with the matching Python image (farbhaus/portal) first, then start this one." >&2
	su -s /bin/sh postgres -c "pg_ctl -D '$PGDATA' -w stop"
	exit 1
fi
( cd /app && portal-api seed )

su -s /bin/sh postgres -c "pg_ctl -D '$PGDATA' -w stop"

echo "[entrypoint] starting supervisord"
exec supervisord -n -c /etc/supervisor/supervisord.conf
