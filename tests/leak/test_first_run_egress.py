#!/usr/bin/env python3
"""First-run egress test scaffold.

Required by CLAUDE.md security invariant 1 and THREAT-MODEL.md ("CI must
include first-run egress tests"). Launches the OpenBook binary in a clean
profile pointed at a loopback HTTP sink and asserts that no outbound
connection is attempted in a fixed window after first launch.

Skip semantics: this test exits with code 77 (TAP "SKIP") when
OPENBOOK_BIN is unset, so CI can wire it up before a build pipeline exists
without failing the workflow. When OPENBOOK_BIN is set, the test runs and
non-zero outbound traffic is a hard failure.
"""

from __future__ import annotations

import http.server
import os
import socket
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

WINDOW_SECONDS = 30
SKIP_EXIT = 77


class CountingHandler(http.server.BaseHTTPRequestHandler):
    hits: list[tuple[str, str]] = []

    def do_GET(self):  # noqa: N802
        CountingHandler.hits.append(("GET", self.path))
        self.send_response(204)
        self.end_headers()

    def do_POST(self):  # noqa: N802
        CountingHandler.hits.append(("POST", self.path))
        self.send_response(204)
        self.end_headers()

    def log_message(self, *_args, **_kwargs):
        return


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _serve_in_background(port: int) -> socketserver.ThreadingTCPServer:
    server = socketserver.ThreadingTCPServer(("127.0.0.1", port), CountingHandler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def main() -> int:
    binary = os.environ.get("OPENBOOK_BIN", "").strip()
    if not binary:
        print(
            "OPENBOOK_BIN not set; first-run egress test SKIPPED. "
            "Set OPENBOOK_BIN to a built OpenBook binary path to run."
        )
        return SKIP_EXIT

    bin_path = Path(binary).expanduser()
    if not bin_path.is_file() or not os.access(bin_path, os.X_OK):
        print(f"OPENBOOK_BIN is not an executable file: {bin_path}", file=sys.stderr)
        return 2

    port = _free_port()
    server = _serve_in_background(port)
    proxy_addr = f"127.0.0.1:{port}"

    with tempfile.TemporaryDirectory(prefix="openbook-egress-") as profile_dir:
        env = os.environ.copy()
        env.update(
            {
                "http_proxy": f"http://{proxy_addr}",
                "https_proxy": f"http://{proxy_addr}",
                "ALL_PROXY": f"http://{proxy_addr}",
            }
        )
        proc = subprocess.Popen(
            [
                str(bin_path),
                "-profile",
                profile_dir,
                "-no-remote",
                "-headless",
                "about:blank",
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            time.sleep(WINDOW_SECONDS)
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
            server.shutdown()

    if CountingHandler.hits:
        print("First-run egress detected:", file=sys.stderr)
        for method, path in CountingHandler.hits:
            print(f"  {method} {path}", file=sys.stderr)
        return 1
    print("First-run egress test passed: no outbound requests in window.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
