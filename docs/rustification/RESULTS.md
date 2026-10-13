# Portal → Rust: results

Machine: macOS 27.0.1, arm64, 10 logical CPUs, 24 GiB RAM, Docker 28.2.2 (Desktop).
Same load generator, same fixture DB for every figure.

> ⚠️ **Credibility caveat.** The Rust service implements the **unauthenticated** behaviour of 6
> route modules and the session/auth machinery; **authenticated response bodies are not ported
> yet** (they return 501). So the equivalence result below is real and exact for what it covers,
> and the runtime numbers are a pipeline demonstration, not yet a like-for-like comparison of the
> converted project.

## 1. Byte-for-byte equivalence — checkpoint 2

```
cd tools/rustify
uv run --with httpx python capture.py http://127.0.0.1:18081 ../../docs/rustification/golden.json
uv run --with httpx python capture.py http://127.0.0.1:18082 ../../docs/rustification/actual.json
uv run --with httpx python diff.py ../../docs/rustification/golden.json ../../docs/rustification/actual.json
# -> PASS: 10 requests byte-for-byte identical
```

All 10 requests in the matrix now match on status, headers and raw body bytes:

| Request | Python | Rust |
|---|---|---|
| `GET /api/health` | 200 `{"status":"ok","db":true,"redis":true,"version":"0.1.0"}` | identical |
| `GET /api/branding`, `GET /api/openapi.json` | 404 `{"detail":"Not Found"}` | identical |
| `GET /api/public/links/doesnotexist` | 404 `{"detail":"This upload link does not exist"}` | identical |
| `GET /api/{dashboard/transfers,upload-links,privacy/subject,settings/security,frameio/status}` | 401 `{"detail":"Authentication required"}` + `Vary: Cookie` | identical |
| `POST /api/auth/login` (wrong password) | 401 `{"detail":"Invalid email or password"}` | identical |

The `Vary: Cookie` header is in the contract: Starlette adds it **only when the session was
read**, so the Rust auth extractor has to reproduce that timing, not just the status code.

## 2. Session cookie interop — exact

The session cookie is the interesting bit: it is `itsdangerous.TimestampSigner` over
standard-base64 of `json.dumps(session)`. Rust reimplements it from the itsdangerous 2.2.0
source (salt, `django-concat` key derivation, HMAC-SHA1, url-safe base64, big-endian-stripped
timestamp) **and** reproduces Python's `json.dumps` spacing.

```
uv run --with httpx python interop.py http://127.0.0.1:18081 http://127.0.0.1:18082 \
    admin@example.com baseline-password
# Rust accepts Python cookie : True
# Python accepts Rust cookie : True
# Cookies byte-identical     : True
```

So a login issued by either service is accepted by the other, and, when both issue in the same
second, the two `Set-Cookie` values are identical byte for byte.

## 3. Size

| Artefact | Python (baseline) | Rust |
|---|---|---|
| All-in-one Docker image | **1.12 GB** | not built yet |
| Largest image layers | 506 MB Postgres/Redis/Node/supervisor apt · 148 MB venv · 48 MB Caddy | — |
| Server binary | n/a (Python venv ≈ 148 MB in image) | **7.0 MB** release binary |

## 4. Runtime (route: `GET /api/health`)

Python `uvicorn --workers 1`, `RATE_LIMIT_ENABLED=false`; Rust release, same Postgres/Redis.
Load: the compiled `loadgen`, 20 s, concurrency 16, zero errors both sides.

| Metric | Python | Rust | Ratio |
|---|---|---|---|
| Throughput | 811 req/s | 8 188 req/s | **10.1×** |
| p50 latency | 17.1 ms | 1.84 ms | 9.3× lower |
| p95 latency | 36.9 ms | 2.75 ms | 13.4× lower |
| Idle RSS | 98 MB | 10 MB | ~10× |
| RSS under load (mean/peak) | 136 MB | 10 MB | ~14× |
| CPU under load | 55 % of a core | 47 % of a core | ~90× lower CPU per request |

Two honest caveats on the Python side of the health endpoint: `portal/routes/health.py` opens a
**new Redis connection per request**, and only **one** uvicorn worker ran (production uses
`UVICORN_WORKERS=2`). At concurrency 64 the Python service also started returning `503 degraded`
from a Redis connection storm. The like-for-like benchmark will normalise worker count and use
endpoints that do no per-request connection setup.

## 5. Ported surface (checkpoint 3)

Byte-identical against the Python service, verified by `tools/rustify/{capture,diff}.py`
(unauthenticated) and `tools/rustify/authdiff.py` (authenticated, per-service cookies):

| Endpoint | Notes |
|---|---|
| `GET /api/health` | db + redis probe |
| `POST /api/auth/login` | bcrypt verify, TOTP branch, session write |
| `GET /api/auth/me` | |
| `GET /api/upload-links` | list, JSONB + computed fields |
| `GET /api/upload-links/extension-preset` | 26-extension preset |
| `GET /api/download-links` | list |
| `GET /api/dashboard/transfers` | merged upload/download/sync feed + sync health |
| `GET /api/frameio/status` | connection state + needs_refresh |
| `GET /api/branding/logo` | 404 when unset; ETag/Cache-Control/304 |
| `GET /api/public/links/{token}` | full body for ok and expired links |
| FastAPI's 404 + 401 shapes, `Vary: Cookie` | middleware/error layer |

Session cookies interop byte-for-byte with the Python service. Not yet ported: the write
endpoints, sync/upload/download engines, Frame.io client, TOTP/passkey writes, email.

## 6. Status

Ported: session middleware, `require_admin` 401 path, password login, health, public-link 404.
Not ported: every authenticated body, TOTP/passkeys, upload/download engines, sync worker,
Frame.io client, email/verify. See `PLAN.md` for scope and the equivalence definition.
