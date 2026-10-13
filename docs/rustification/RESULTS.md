# Portal → Rust: results

Machine: macOS 27.0.1, arm64, 10 logical CPUs, 24 GiB RAM, Docker 28.2.2 (Desktop).
Same host, same Postgres/Redis/Caddy inside each image, same compiled load generator.

> **Scope note (read first).** The Rust service is byte-for-byte identical to the Python service
> for the endpoints listed in §5 — the unauthenticated surface plus the main read endpoints the
> admin shell loads. Write endpoints, the sync worker, the Frame.io client, TOTP/passkey writes
> and email are **not ported yet** (the Rust container's `worker` program is deliberately absent).
> The size and runtime numbers below are honest for the two committed images, but the Rust image
> is not yet a functional replacement until those are ported.

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

Not ported: all write endpoints, upload/download session engines, sync engine + worker, Frame.io
client + OAuth, TOTP/passkey mutations, email/verify, GDPR export/erase.
