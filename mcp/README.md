# Portal MCP server

A small [MCP](https://modelcontextprotocol.io) server that exposes Portal's admin API as tools:
upload links (including the Project Zeus `.turbosort` field), sync rules, and Frame.io browsing.

It is a client of the HTTP API, not part of the Portal container. It authenticates with
`PORTAL_API_TOKEN` (the bearer token described in [../docs/API.md](../docs/API.md)).

## Run

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt

PORTAL_URL=https://portal.example.com \
PORTAL_API_TOKEN='<the token from Portal .env>' \
python portal_mcp.py
```

The server speaks MCP over stdio by default.

## MCP client configuration

```json
{
  "mcpServers": {
    "portal": {
      "command": "/path/to/.venv/bin/python",
      "args": ["/path/to/portal/mcp/portal_mcp.py"],
      "env": {
        "PORTAL_URL": "https://portal.example.com",
        "PORTAL_API_TOKEN": "<token>"
      }
    }
  }
}
```

## Tools

| Tool | Purpose |
|---|---|
| `health` | Service, database and Redis health |
| `list_upload_links` / `get_upload_link` | Read upload links |
| `create_upload_link` | Create a link for a Frame.io folder, with optional `turbosort` |
| `update_upload_link` | Patch any link fields |
| `set_project_zeus` | Set or clear the Live_Work folder for a link |
| `delete_upload_link` | Delete a link and its marker |
| `list_sync_rules` / `get_sync_rule` | Read Frame.io-to-NAS rules |
| `create_sync_rule` / `run_sync_rule` | Create a rule, or reconcile one now |
| `frameio_status`, `list_frameio_accounts`, `list_frameio_workspaces`, `list_frameio_projects`, `list_frameio_folders`, `list_frameio_files` | Browse the connected Frame.io account |

The token is full-admin. Keep it out of logs and client configuration files you share.
