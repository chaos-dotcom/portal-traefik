# Portal API

Portal's admin API is the same JSON API the web app uses, under `/api`. Two ways to authenticate:

- **Session cookie** — log in with `POST /api/auth/login`, keep the `portal_session` cookie.
- **Bearer token** — set `PORTAL_API_TOKEN` on the server and send
  `Authorization: Bearer <token>`. The token acts as the active admin and works on every admin
  endpoint. Leave the variable unset to disable token auth.

All request and response bodies are JSON. Errors look like `{"detail":"..."}` with a 4xx/5xx
status. `GET /api/health` is public.

## Upload links

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/upload-links` | List upload links |
| `POST` | `/api/upload-links` | Create an upload link |
| `GET` | `/api/upload-links/{id}` | Read one |
| `PATCH` | `/api/upload-links/{id}` | Update fields (only keys present are applied) |
| `DELETE` | `/api/upload-links/{id}` | Delete |
| `POST` | `/api/upload-links/{id}/revoke` | Stop accepting uploads |
| `POST` | `/api/upload-links/{id}/restore` | Undo a revoke |

Create an upload link:

```bash
curl -X POST https://portal.example.com/api/upload-links \
  -H "Authorization: Bearer $PORTAL_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "ACME Spring Spot",
    "folder": {
      "type": "frameio",
      "account_id": "…",
      "workspace_id": "…",
      "project_id": "…",
      "folder_id": "…"
    },
    "turbosort": "20261028_ACME-SpringSpot"
  }'
```

The response includes `public_url` (the client-facing link) and `turbosort`.

`turbosort` is optional. When set, Portal writes a `.turbosort` marker into the link's Frame.io
folder with that value; Project Zeus reads it and copies the folder to
`Live_Work/<value>/1_DRIVE`. Changing the field rewrites the marker; sending `null` or deleting
the link removes it.

## Other resources

The same bearer token works on the rest of the admin API, including:

- `/api/links`, `/api/download-links`
- `/api/sync-rules` (Frame.io → NAS sync rules)
- `/api/frameio/accounts`, `/api/frameio/workspaces`, `/api/frameio/projects`,
  `/api/frameio/folders`, `/api/frameio/files`
- `/api/activity`, `/api/dashboard`, `/api/settings`

The web app's `/api` calls are the reference for field names and response shapes.
