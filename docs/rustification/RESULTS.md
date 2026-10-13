# Portal → Rust: preliminary results (checkpoint 1)

Machine: macOS 27.0.1, arm64, 10 logical CPUs, 24 GiB RAM, Docker 28.2.2 (Desktop).
All figures on this page are from **this** machine, same load generator, same fixture DB.

> ⚠️ **Credibility caveat — read first.** The Rust service currently implements **1 of 91
> routes** (`GET /api/health`). The before/after numbers below are therefore a *proof of the
> measurement pipeline and of byte-for-byte equivalence on that route*, **not** a like-for-like
> comparison of the converted project. They will be re-measured as routes land. Reproduce with
> the commands in each section.

## 1. Byte-for-byte equivalence

Harness: `tools/rustify/{matrix,capture,diff}.py`.

```
cd tools/rustify
uv run --with httpx python capture.py http://127.0.0.1:18081 docs/rustification/golden.json   # Python
uv run --with httpx python capture.py http://127.0.0.1:18082 docs/rustification/actual.json   # Rust
uv run --with httpx python diff.py docs/rustification/golden.json docs/rustification/actual.json
```

Result on the current 10-request matrix: **3/10 identical**, and every difference is an
un-ported route (Rust returns its fallback 404 where Python returns 401 / a domain 404).

| Request | Python | Rust | Match |
|---|---|---|---|
| `GET /api/health` | 200 `{"status":"ok","db":true,"redis":true,"version":"0.1.0"}` | same | ✅ byte-identical |
| `GET /api/branding` | 404 `{"detail":"Not Found"}` | same | ✅ byte-identical |
| `GET /api/openapi.json` | 404 `{"detail":"Not Found"}` | same | ✅ byte-identical |
| `GET /api/{dashboard/transfers,upload-links,privacy/subject,settings/security,frameio/status}` | 401 + `Vary: Cookie` | 404 | ⏳ route not ported |
| `GET /api/public/links/doesnotexist` | 404 `{"detail":"This upload link does not exist"}` | 404 `{"detail":"Not Found"}` | ⏳ route not ported |
| `POST /api/auth/login` | 401 `{"detail":"Invalid email or password"}` | 404 | ⏳ route not ported |

Note the `Vary: Cookie` header on authenticated 401s — it comes from Starlette's
`SessionMiddleware` and is part of the byte contract.

## 2. Size

| Artefact | Python (baseline) | Rust | 
|---|---|---|
| All-in-one Docker image | **1.12 GB** (`docker images portal-baseline:py`) | not built yet |
| Largest layers | 506 MB Postgres/Redis/Node/supervisor apt; 148 MB venv; 48 MB Caddy | — |
| Server binary | n/a (Python venv; `.venv` ≈ 148 MB in image) | **6.9 MB** release binary |

## 3. Runtime (route: `GET /api/health`)

Python: `uvicorn portal.main:app --workers 1`, `RATE_LIMIT_ENABLED=false`.
Rust: release build, same Postgres/Redis. Load: `loadgen` (compiled, same for both),
20 s, concurrency 16, no errors on either side.

| Metric | Python | Rust | Ratio |
|---|---|---|---|
| Throughput | 811 req/s | 8 188 req/s | **10.1×** |
| p50 latency | 17.1 ms | 1.84 ms | 9.3× lower |
| p95 latency | 36.9 ms | 2.75 ms | 13.4× lower |
| Idle RSS | 98–112 MB | 10–11 MB | ~10× |
| RSS under load (mean/peak) | 136 MB | 10 MB | ~14× |
| CPU under load | 55% of a core | 47% of a core | ~90× lower CPU per request |

Two things inflate the Python health cost and we flag them rather than hide them:
`portal/routes/health.py` opens a **new Redis connection on every request**, and only **one**
uvicorn worker ran; production uses `UVICORN_WORKERS=2`. At concurrency 64 the Python service
also began returning `503 degraded` (Redis connection storm) — a real behaviour, recorded, not
swept up. A like-for-like benchmark will normalise worker count and use endpoints that do not
do per-request connection setup.

## 4. Status

- Baseline image builds and runs; golden capture + byte diff harness works end to end.
- Rust workspace `apps/api-rs/` builds; `/api/health` is byte-identical.
- Next: auth/session middleware (401 + `Vary: Cookie`), public link resolution, then the
  authenticated admin surface. See `PLAN.md` for scope and the equivalence definition.
