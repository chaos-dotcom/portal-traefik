
"""Diff authenticated responses between two services (log in to each, replay the path list).

  python authdiff.py <py_url> <rust_url> <email> <password> [path ...]

Cookies are per-service (session tokens differ by design); response bodies must match.
"""

import sys

import httpx

from matrix import VOLATILE_HEADERS

DEFAULT_PATHS = [
    "/api/auth/me",
    "/api/upload-links",
    "/api/upload-links/extension-preset",
    "/api/download-links",
    "/api/dashboard/transfers?limit=15",
    "/api/frameio/status",
    "/api/settings/security",
    "/api/settings/email",
    "/api/settings/frameio-config",
    "/api/sync-rules",
    "/api/dashboard/history",
    "/api/activity",
    "/api/activity/actions",
    "/api/activity/summary",
    "/api/destinations",
]


def login(base, email, pw):
    with httpx.Client(base_url=base, timeout=30.0) as c:
        r = c.post("/api/auth/login", json={"email": email, "password": pw})
        r.raise_for_status()
        return r.cookies.get("portal_session")


def get(base, cookie, path):
    with httpx.Client(base_url=base, timeout=30.0) as c:
        return c.get(path, headers={"Cookie": f"portal_session={cookie}"})


def main():
    py, rust, email, pw, *paths = sys.argv[1:5] + sys.argv[5:]
    paths = paths or DEFAULT_PATHS
    pyc = login(py, email, pw)
    rc = login(rust, email, pw)
    fails = 0
    for path in paths:
        a = get(py, pyc, path)
        b = get(rust, rc, path)
        ha = {k: v for k, v in a.headers.items() if k.lower() not in VOLATILE_HEADERS}
        hb = {k: v for k, v in b.headers.items() if k.lower() not in VOLATILE_HEADERS}
        same = a.status_code == b.status_code and ha == hb and a.content == b.content
        print(f"{'OK  ' if same else 'DIFF'} {path}  {a.status_code} vs {b.status_code}")
        if not same:
            fails += 1
            if a.content != b.content:
                print("   py  :", a.content[:400])
                print("   rust:", b.content[:400])
    print("PASS" if fails == 0 else f"FAIL ({fails})")
    sys.exit(0 if fails == 0 else 1)


if __name__ == "__main__":
    main()
