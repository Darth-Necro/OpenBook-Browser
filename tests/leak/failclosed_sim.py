#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — fail-closed logic simulation (Build Plan §6, control #4).
#
# This is a SELF-CONTAINED, OFFLINE, deterministic proof of the *fail-closed
# logic*: if the proxy/tunnel is down, the client must block traffic and must
# NEVER silently fall back to a direct connection.
#
# It uses NO external network. Everything runs on 127.0.0.1:
#   - A "direct-internet sink": a local TCP server standing in for "the open
#     internet you would reach if you bypassed the proxy". If the client ever
#     connects to it while the tunnel is down, that is a leak.
#   - A "proxy endpoint": a local TCP server standing in for the user's
#     proxy/tunnel. When we want to model "tunnel down" we simply do not start it
#     (or stop it) so connections to it are refused.
#   - A FailClosedClient that models the policy: route only through the proxy; if
#     the proxy is unreachable, BLOCK — do not touch the direct sink.
#
# The full live harness (run the real browser against a real SOCKS proxy +
# sinkhole and assert nothing escapes on WebRTC/DNS/IPv6/tunnel-failure) is
# documented in README.md; this file is the offline gate for the fail-closed
# vector specifically.
#
# Exit code: 0 on success (all assertions hold), nonzero on any failure.

import socket
import sys
import threading


class CountingTcpSink:
    """A local TCP server that counts inbound connections.

    Stands in for the 'direct internet'. In a correct fail-closed client, the
    accept count MUST remain 0 while the tunnel is down.
    """

    def __init__(self, name):
        self.name = name
        self._srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._srv.bind(("127.0.0.1", 0))  # ephemeral port
        self._srv.listen(8)
        self._srv.settimeout(0.25)
        self.host, self.port = self._srv.getsockname()
        self.connections = 0
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._serve, name=f"sink-{name}", daemon=True)

    def start(self):
        self._thread.start()
        return self

    def _serve(self):
        while not self._stop.is_set():
            try:
                conn, _ = self._srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.connections += 1
            try:
                conn.sendall(b"SINK-REACHED")
            except OSError:
                pass
            finally:
                conn.close()

    def stop(self):
        self._stop.set()
        self._thread.join(timeout=2.0)
        try:
            self._srv.close()
        except OSError:
            pass


class TunnelState:
    """Models the proxy/tunnel. 'up' = a live local listener; 'down' = closed."""

    def __init__(self):
        self._srv = None
        self.host = "127.0.0.1"
        self.port = None
        self._stop = None
        self._thread = None

    def bring_up(self):
        self._srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._srv.bind(("127.0.0.1", 0))
        self._srv.listen(8)
        self._srv.settimeout(0.25)
        self.host, self.port = self._srv.getsockname()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._serve, name="proxy", daemon=True)
        self._thread.start()

    def _serve(self):
        while not self._stop.is_set():
            try:
                conn, _ = self._srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            try:
                conn.sendall(b"PROXY-OK")
            except OSError:
                pass
            finally:
                conn.close()

    def bring_down(self):
        """Tear the tunnel down: future connects to the proxy port are refused."""
        if self._stop is not None:
            self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2.0)
        if self._srv is not None:
            try:
                self._srv.close()
            except OSError:
                pass
            self._srv = None

    @property
    def is_up(self):
        return self._srv is not None


class FailClosedError(Exception):
    """Raised when a request is blocked because the tunnel is down (correct)."""


class FailClosedClient:
    """A client that routes ONLY through the proxy and fails closed.

    The policy under test:
      1. Always attempt the proxy first.
      2. If the proxy connection fails (tunnel down), BLOCK the request.
      3. NEVER connect to the direct sink as a fallback.

    `direct_sink` is passed in only so the test can prove the client never
    touches it; a correct client treats it as forbidden.
    """

    def __init__(self, tunnel, direct_sink, allow_direct_fallback=False):
        self.tunnel = tunnel
        self.direct_sink = direct_sink
        # This flag exists ONLY to model the BROKEN (fail-open) behavior in a
        # negative control test. The shipped policy is allow_direct_fallback=False.
        self.allow_direct_fallback = allow_direct_fallback

    def _connect(self, host, port, timeout=1.0):
        s = socket.create_connection((host, port), timeout=timeout)
        try:
            s.sendall(b"GET / HTTP/1.0\r\n\r\n")
            return s.recv(64)
        finally:
            s.close()

    def request(self):
        """Perform one request under the fail-closed policy.

        Returns the proxy's response bytes on success. Raises FailClosedError if
        the tunnel is down and the (correct) policy blocks. Returns the sink's
        response only in the deliberately-broken fail-open control.
        """
        # Step 1: try the proxy.
        if self.tunnel.is_up and self.tunnel.port is not None:
            try:
                return self._connect(self.tunnel.host, self.tunnel.port)
            except OSError:
                pass  # fall through to policy decision

        # Step 2: proxy unreachable. Decide based on policy.
        if not self.allow_direct_fallback:
            # CORRECT: fail closed. Do not touch the direct sink.
            raise FailClosedError("tunnel down; request blocked (fail-closed)")

        # BROKEN fail-open behavior (negative control only): go direct.
        return self._connect(self.direct_sink.host, self.direct_sink.port)


def _run() -> int:
    failures = []

    def check(cond, msg):
        if cond:
            print(f"  PASS: {msg}")
        else:
            print(f"  FAIL: {msg}")
            failures.append(msg)

    sink = CountingTcpSink("direct-internet").start()
    tunnel = TunnelState()

    try:
        # ---- Case A: tunnel UP -> request succeeds via proxy, sink untouched ----
        print("[case A] tunnel UP: traffic flows via proxy, never the direct sink")
        tunnel.bring_up()
        client = FailClosedClient(tunnel, sink, allow_direct_fallback=False)
        resp = client.request()
        check(resp == b"PROXY-OK", "request served by the proxy while tunnel up")
        check(sink.connections == 0, "direct sink received ZERO connections while tunnel up")

        # ---- Case B: tunnel DOWN -> fail closed, request blocked, sink untouched -
        print("[case B] tunnel DOWN: requests are BLOCKED and the direct sink stays at zero")
        tunnel.bring_down()
        sink_before = sink.connections
        blocked = 0
        attempts = 5
        for _ in range(attempts):
            try:
                client.request()
                check(False, "request should have been blocked while tunnel down")
            except FailClosedError:
                blocked += 1
        check(blocked == attempts, f"all {attempts} requests blocked while tunnel down (fail-closed)")
        check(
            sink.connections == sink_before,
            "direct sink received ZERO new connections while tunnel down (no silent direct fallback)",
        )

        # ---- Case C (negative control): a FAIL-OPEN client WOULD leak ----------
        # This proves the test can actually observe a leak — i.e. the assertions
        # in Case B are meaningful and not vacuously true.
        print("[case C] negative control: a (broken) fail-OPEN client DOES reach the sink")
        leaky = FailClosedClient(tunnel, sink, allow_direct_fallback=True)
        leak_before = sink.connections
        resp = leaky.request()
        check(resp == b"SINK-REACHED", "fail-open client reaches the direct sink (control)")
        check(
            sink.connections == leak_before + 1,
            "sink connection counter incremented for the fail-open control (test can detect leaks)",
        )
    finally:
        tunnel.bring_down()
        sink.stop()

    print()
    if failures:
        print(f"FAIL-CLOSED SIMULATION: {len(failures)} assertion(s) failed.")
        return 1
    print("FAIL-CLOSED SIMULATION: all assertions passed.")
    return 0


# --- pytest entry points (discoverable) -------------------------------------

def test_failclosed_blocks_and_no_leak():
    assert _run() == 0


if __name__ == "__main__":
    sys.exit(_run())
