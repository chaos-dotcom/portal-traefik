# Portal → Rust: results

Machine: macOS 27.0.1, arm64, 10 logical CPUs, 24 GiB RAM, Docker 28.2.2 (Desktop).
Same host, same Postgres/Redis/Caddy inside each image, same compiled load generator.

> **Status: 1:1 functional drop-in.** All **101 API routes** are ported (`docs/rustification/COVERAGE.md`:
> 100 done, 1 partial — `POST /api/destinations` needs a live Frame.io connection to validate a
> folder, and reproduces the unconnected path). The Rust container runs Postgres, Redis, the API +
> static SPA, **and the sync worker**; `portal-api seed` replaces the Python seed and `portal-api
> worker` replaces the arq worker. No Python or Node at runtime.
>
> Paths that need live credentials or a browser (real Frame.io downloads/uploads, SMTP delivery,
> browser WebAuthn) are ported but could not be end-to-end exercised here. Two deviations are
> recorded in COVERAGE.md: passkeys registered by the Python service must be re-registered, and
> the TOTP QR SVG bytes differ (same payload).

## 1. Byte-for-byte equivalence

Harness in `tools/rustify/`: `capture.py` + `diff.py` for unauthenticated requests,
`authdiff.py` for authenticated ones (logs in to each service, replays the path list),
`interop.py` for session-cookie cross-acceptance.

```
# unauthenticated matrix
uv run --with httpx python capture.py http://127.0.0.1:18081 golden.json
uv run --with httpx python capture.py http://127.0.0.1:18082 actual.json
uv run --with httpx python diff.py golden.json actual.json      # PASS: 10/10
# authenticated (per-service cookies)
uv run --with httpx python authdiff.py http://127.0.0.1:18081 http://127.0.0.1:18082 \
    admin@example.com baseline-password                           # PASS: 14/14
```

Two details that make "byte-for-byte" more than "same JSON":
- **`Vary: Cookie`** — Starlette adds it only when the session was *read*; the Rust auth
  extractor reproduces that, so the header set matches on 401s.
- **Session cookies** — the cookie is `itsdangerous.TimestampSigner` over standard-base64 of
  `json.dumps(session)`. `src/session.rs` reproduces it exactly (salt, `django-concat` key
  derivation, HMAC-SHA1, url-safe base64, stripped big-endian timestamp, Python's `", "`/`": "`
  spacing). A cookie issued by either service is accepted by the other, and the two `Set-Cookie`
  values are byte-identical when issued in the same second.

## 2. Frontend: SvelteKit's output served by Rust, no Node at runtime

`apps/web` now builds with `adapter-static` (`fallback: index.html`, `ssr = false`). The build is
static files the Rust process serves on the same origin as `/api/*`, so route loads use relative
`fetch` and same-origin cookies just work. Rust is a file server for SvelteKit's own bytes; the
19 `+page.server.ts`/`+layout.server.ts` loads became universal `+page.ts`/`+layout.ts`.

## 3. Size — Python image vs Rust image

| Artefact | Python (`portal-baseline:py`) | Rust (`portal-rust:dev`) |
|---|---|---|
| **All-in-one image** | **1.12 GB** | **808 MB** (−28%) |
| · apt stack (Postgres 16 + Redis + supervisor + nodejs) | 506 MB | 448 MB |
| · application layer | 148 MB (venv) | 10.8 MB (binary) |
| · web build | 3.8 MB (SSR) | 643 KB (static) |
| · Caddy | 48.4 MB | 48.4 MB |
| Server binary | n/a | 10.3 MB in-image (9.5 MiB local release) |
| Node runtime | present | **absent** |

The image is dominated by Postgres/Redis/Caddy; the *application* payload drops ~145 MB → ~11 MB.

## 4. Runtime — both containers, behind-proxy, same load

`docker run` the two images with the same env; load `GET /api/health` for 20 s at concurrency 16
through each container's Caddy port; container figures from `docker stats`.

| Metric | Python | Rust | Ratio |
|---|---|---|---|
| Throughput | 1 569 req/s | 13 869 req/s | **8.8×** |
| p50 latency | 9.84 ms | 1.02 ms | 9.6× lower |
| p95 latency | 12.8 ms | 2.0 ms | 6.4× lower |
| Idle container memory | 269 MB | 75 MB | 3.6× less |
| Container CPU under load | 145 % | 435 % | — |
| CPU per request | 0.092 % | 0.031 % | **~3× less** |
| Cold start (run → first 200) | ~7 s | ~5 s | — |

Caveats, stated rather than hidden: `portal/routes/health.py` opens a **new Redis connection per
request** (the Rust health reuses one) and runs a single uvicorn worker; the Rust server is
multi-threaded and uses more cores, which is where its CPU-per-request advantage comes from.

## 5. Ported surface (byte-identical)

Unauthenticated: `GET /api/health`, `GET /api/branding/logo`, `GET /api/public/links/{token}`,
FastAPI's 404/401 shapes, `POST /api/auth/login`.
Authenticated: `GET /api/auth/me`, `/api/upload-links`, `/api/upload-links/extension-preset`,
`/api/download-links`, `/api/dashboard/transfers`, `/api/dashboard/history`, `/api/frameio/status`,
`/api/settings/security`, `/api/settings/email`, `/api/settings/frameio-config`, `/api/sync-rules`,
`/api/activity`, `/api/activity/actions`, `/api/activity/summary`.

That list is the byte-diffed read surface. Every route is ported (COVERAGE.md); the write paths
and engines were checked by behaviour rather than byte diff — see §6.

## 6. Parity with `main` (a6adf7e)

The port was branched at ff0e1f9; `main` then gained the changes below. Each is ported, alongside
fixes for places where the port had diverged from the Python service it copies.

Ported from `main`:
- Sync stays inside a rule's folder (#65): `folder_relative_path` returns `None` when the walk
  never reaches the rule's folder, folder metadata expires after 600s, scope is checked before the
  readiness wait, and a file skipped as outside is re-run by reconcile once it shows up in the
  rule's listing.
- Waiting jobs wake on arrival (#85): webhooks and upload triggers wake a job parked in `waiting`;
  the worker claims jobs atomically and follow-ups carry the job's `updated_at` (`as_of`), so a
  superseded re-check is dropped. Reconcile also heals stranded `waiting` jobs, with stale
  thresholds derived from the retry backoff and poll cadence.
- Retrying a running job is a 409; the dashboard reports `queued` (pending) jobs.
- Uploads (#69, #86): `total_bytes` is recorded when the session starts; completion sets
  `uploaded_bytes`, mails only on the first completion, sends the uploader a delivery receipt
  (verified addresses only) and returns `receipt_to`. Trust is slid on uploads and downloads only
  for an address this browser verified.
- Email: one recipient per message, and `main`'s stricter address syntax (no lists, display names
  or quoted parts; 254-char cap). Rust's regex engine is linear, so the ReDoS fix holds by design.
- Healthy `/api/health` probes are not request-logged.

Port divergences fixed along the way:
- `POST /api/auth/login/totp` read `{email, password}`; the web app sends `{code}`, so 2FA admins
  could not log in.
- Rate limits matched Python's on only 5 of its 12 limited routes (none on admin login).
- Placeholder secrets were silently accepted (a forgeable session cookie); now refused at start,
  like `require_secure_secrets` (`ALLOW_INSECURE_DEFAULTS=true` for local runs).
- Email verification codes came from a non-cryptographic RNG; now the OS CSPRNG, with a
  constant-time compare and an atomic attempt counter.
- No `x-request-id` / request log; plain-SMTP relays (STARTTLS off) unsupported; SMTP errors
  surfaced raw; pydantic-style booleans (`yes`/`on`) ignored; the legacy `.env` SMTP/Frame.io
  import (`bootstrap_config`) missing from `portal-api seed`; download selections fetched with
  unbounded concurrency; sync downloads used a 300s total (not per-read) timeout and no job
  timeout; job errors stored as Rust `Debug` text.
- The harness excluded `x-request-id` from comparison; it now pins and compares it.
- `TokenCipher` derived a different AES key than Python for a 43-character
  `secrets.token_urlsafe(32)` key (Rust also accepted unpadded base64; Python's decoder requires
  padding and hashes the key instead). Found in production: Python-stored secrets (Frame.io tokens
  and client secret, SMTP password, TOTP secret) were unreadable, while Settings still said
  "connected". Derivation now matches Python byte for byte (test vectors from Python), `portal-api
  migrate` re-encrypts anything an earlier Rust build stored under the old key, and
  `/api/frameio/status` reports not connected when the stored tokens can't be decrypted
  (deliberately stricter than Python, which only checks the row exists).

Verified (against the Python service at `main`, sharing one database):
- `capture.py`/`diff.py` 10/10 byte-identical, `authdiff.py` 22/22, cookie interop both ways.
- Security checks: 2FA login, 429 at the same request counts on every limited route, refusal of
  placeholder secrets.
- Upload completion on both services (verified/unverified/replayed) gives identical responses;
  the receipt Rust sends is byte-identical (text and HTML) to Python's `render_upload_receipt`.
- Sync worker against a mock Frame.io (13 scenarios: scope, nesting, wake-up within 4s of
  arrival vs a 20s poll, one download per file, 409 on retry, reconcile healing).
- Image: first boot, restart, revision guard (older revision refused; unstamped stamped), and a
  `/data` volume created by `farbhaus/portal:latest` booting on the Rust image with its sessions
  and data intact (and back again).
- `cargo test`: 17 unit tests plus one live-DNS test (`-- --ignored`); none existed before.
- CI (`.github/workflows/ci.yml`): `cargo clippy --all-targets -- -D warnings`, `cargo test`, and
  `tools/rustify/schema_check.sh`. The port's 94 clippy/rustc warnings were fixed to get there.

Also closed after the first pass:
- The email plausibility check now asks DNS for MX, then A, then AAAA (hickory-resolver), rejects a
  domain that doesn't exist and fails open on timeouts/SERVFAIL — as Python does. It previously
  resolved A/AAAA through getaddrinfo and failed open on everything, NXDOMAIN included.
- Download sessions created before the `files` snapshot column existed resolve the file live
  against the link's source, as in Python.
- The Rust image's Caddyfile lacked `trusted_proxies static private_ranges` (added to the Python
  image on 2026-08-09), so behind a host proxy Caddy replaced X-Forwarded-For with the docker
  gateway: every client looked like 127.0.0.1 to the rate limiter and the download log.

The Rust image is now *the* image (`Dockerfile`, `docker/container/`): the compose healthcheck
uses curl, the Python/Node image and its container files are gone, and `portal-api migrate`
replaces the entrypoint's schema logic — it applies `schema.sql` to an empty database, upgrades
older revisions with `apps/api-rs/migrations/` (generated from Alembic; see its README), and
refuses a revision it has no path from. Also restored from the SSR days: `/u/{token}` and
`/d/{token}` are served with the link's branded `<title>` (link previews) and a 404 for an unknown
link; logout expires the session cookie (Starlette's cleared-session cookie was never ported, so
the browser kept a valid one); `SESSION_COOKIE_NAME`, `BRANDING_LOGO_MAX_BYTES` and `LOG_LEVEL`
are honoured.

Known deviations:
- Passkeys registered by the Python service must be re-registered (accepted); the TOTP QR SVG
  bytes differ (same payload).
- Live Frame.io, SMTP over TLS and browser WebAuthn remain unexercised (no credentials here).

