#!/usr/bin/env python3
"""Portal MCP server.

Wraps Portal's admin REST API with bearer-token auth so an MCP client can list and manage
upload links, sync rules and Frame.io folders. Configure with:

  PORTAL_URL=https://portal.example.com
  PORTAL_API_TOKEN=<the server's PORTAL_API_TOKEN>
"""

from __future__ import annotations

import os
from typing import Any

import httpx
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("portal")

BASE_URL = os.environ.get("PORTAL_URL", "http://127.0.0.1:18090").rstrip("/")
API_TOKEN = os.environ.get("PORTAL_API_TOKEN", "")


def _request(method: str, path: str, **kwargs: Any) -> Any:
    if not API_TOKEN:
        raise RuntimeError("PORTAL_API_TOKEN is not set")
    headers = {"Authorization": f"Bearer {API_TOKEN}"}
    with httpx.Client(base_url=BASE_URL, headers=headers, timeout=60.0) as client:
        response = client.request(method, path, **kwargs)
    if response.status_code == 204:
        return {"ok": True}
    try:
        body = response.json()
    except ValueError:
        body = response.text
    if response.is_error:
        detail = body.get("detail") if isinstance(body, dict) else body
        raise RuntimeError(f"Portal API {response.status_code}: {detail}")
    return body


# --- health -----------------------------------------------------------------

@mcp.tool()
def health() -> dict:
    """Return Portal's service, database and Redis health."""
    return _request("GET", "/api/health")


# --- upload links -----------------------------------------------------------

@mcp.tool()
def list_upload_links() -> list[dict]:
    """List every upload link, including public URL, state and Project Zeus folder."""
    return _request("GET", "/api/upload-links")


@mcp.tool()
def get_upload_link(link_id: str) -> dict:
    """Read one upload link by id."""
    return _request("GET", f"/api/upload-links/{link_id}")


@mcp.tool()
def create_upload_link(
    name: str,
    account_id: str,
    workspace_id: str,
    project_id: str,
    folder_id: str,
    turbosort: str | None = None,
    expires_at: str | None = None,
    password: str | None = None,
    max_file_size: int | None = None,
    allowed_extensions: list[str] | None = None,
    subfolder_template: str | None = None,
) -> dict:
    """Create an upload link for a Frame.io folder.

    `turbosort` is the Project Zeus Live_Work folder name; when set, Portal writes a
    `.turbosort` marker into the Frame.io folder. `expires_at` is RFC 3339.
    """
    body: dict[str, Any] = {
        "name": name,
        "folder": {
            "type": "frameio",
            "account_id": account_id,
            "workspace_id": workspace_id,
            "project_id": project_id,
            "folder_id": folder_id,
        },
    }
    optional = {
        "turbosort": turbosort,
        "expires_at": expires_at,
        "password": password,
        "max_file_size": max_file_size,
        "allowed_extensions": allowed_extensions,
        "subfolder_template": subfolder_template,
    }
    body.update({k: v for k, v in optional.items() if v is not None})
    return _request("POST", "/api/upload-links", json=body)


@mcp.tool()
def update_upload_link(link_id: str, fields: dict) -> dict:
    """Patch an upload link. Only the supplied fields change. Send `turbosort: null` to remove it."""
    return _request("PATCH", f"/api/upload-links/{link_id}", json=fields)


@mcp.tool()
def set_project_zeus(link_id: str, livework_folder: str | None) -> dict:
    """Set (or clear, with null) the Project Zeus Live_Work folder for an upload link."""
    return _request("PATCH", f"/api/upload-links/{link_id}", json={"turbosort": livework_folder})


@mcp.tool()
def delete_upload_link(link_id: str) -> dict:
    """Delete an upload link and its Project Zeus marker."""
    return _request("DELETE", f"/api/upload-links/{link_id}")


# --- sync rules -------------------------------------------------------------

@mcp.tool()
def list_sync_rules() -> list[dict]:
    """List Frame.io-to-NAS sync rules."""
    return _request("GET", "/api/sync-rules")


@mcp.tool()
def get_sync_rule(rule_id: str) -> dict:
    """Read one sync rule."""
    return _request("GET", f"/api/sync-rules/{rule_id}")


@mcp.tool()
def create_sync_rule(
    name: str,
    account_id: str,
    workspace_id: str,
    project_id: str,
    folder_id: str,
    destination_path: str,
    conflict_policy: str = "rename_suffix",
    path_template: str | None = None,
    enabled: bool = True,
) -> dict:
    """Create a sync rule. `destination_path` is a path the Portal container can write to."""
    body: dict[str, Any] = {
        "name": name,
        "source": {
            "type": "frameio",
            "account_id": account_id,
            "workspace_id": workspace_id,
            "project_id": project_id,
            "folder_id": folder_id,
            "recursive": True,
        },
        "destination_path": destination_path,
        "conflict_policy": conflict_policy,
        "enabled": enabled,
    }
    if path_template is not None:
        body["path_template"] = path_template
    return _request("POST", "/api/sync-rules", json=body)


@mcp.tool()
def run_sync_rule(rule_id: str) -> dict:
    """Reconcile a sync rule now: queue any missing files."""
    return _request("POST", f"/api/sync-rules/{rule_id}/run")


# --- Frame.io ---------------------------------------------------------------

@mcp.tool()
def frameio_status() -> dict:
    """Return whether Frame.io is connected and when its token expires."""
    return _request("GET", "/api/frameio/status")


@mcp.tool()
def list_frameio_accounts() -> list[dict]:
    """List Frame.io accounts visible to the connected user."""
    return _request("GET", "/api/frameio/accounts")


@mcp.tool()
def list_frameio_workspaces(account_id: str) -> list[dict]:
    """List workspaces in a Frame.io account."""
    return _request("GET", "/api/frameio/workspaces", params={"account_id": account_id})


@mcp.tool()
def list_frameio_projects(account_id: str, workspace_id: str) -> list[dict]:
    """List projects in a Frame.io workspace."""
    return _request(
        "GET",
        "/api/frameio/projects",
        params={"account_id": account_id, "workspace_id": workspace_id},
    )


@mcp.tool()
def list_frameio_folders(account_id: str, folder_id: str) -> list[dict]:
    """List subfolders of a Frame.io folder."""
    return _request(
        "GET",
        "/api/frameio/folders",
        params={"account_id": account_id, "folder_id": folder_id},
    )


@mcp.tool()
def list_frameio_files(account_id: str, folder_id: str) -> list[dict]:
    """List files in a Frame.io folder (including the .turbosort marker)."""
    return _request(
        "GET",
        "/api/frameio/files",
        params={"account_id": account_id, "folder_id": folder_id},
    )


if __name__ == "__main__":
    mcp.run()
