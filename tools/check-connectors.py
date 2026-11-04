#!/usr/bin/env python3
"""Real-process connector checks on macOS, using a disposable host and Keychain key.

Run cargo build in host first, then python3 tools/check-connectors.py.
Add --network to also install a pinned npm MCP and contact DeepWiki/Registry.
No existing Codync data or third-party accounts are used. Tool calls only read
test files or public documentation. The temporary Keychain entry is removed.
"""
import argparse
import base64
import contextlib
import hashlib
import json
import os
from pathlib import Path
import queue
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "host/target/debug/codync-host"
SECRET = "connector-check-disposable-key"


def http(url, body=None, token=None, timeout=100):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(url, data=None if body is None else json.dumps(body).encode(), headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


class Remote(BaseHTTPRequestHandler):
    """Real loopback HTTP MCP with session/header checks and JSON or SSE replies."""
    def log_message(self, *_):
        pass

    def json_response(self, value, status=200):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(value).encode())

    def do_GET(self):
        origin = f"http://127.0.0.1:{self.server.server_port}"
        if self.path.startswith("/.well-known/oauth-protected-resource"):
            self.json_response({"resource": origin + "/oauth", "authorization_servers": [origin], "scopes_supported": ["read"]})
        elif self.path.startswith("/.well-known/oauth-authorization-server"):
            self.json_response({"issuer": origin, "authorization_endpoint": origin + "/authorize", "token_endpoint": origin + "/token", "registration_endpoint": origin + "/register"})
        else:
            self.json_response({"error": "not found"}, 404)

    def do_POST(self):
        raw = self.rfile.read(int(self.headers["Content-Length"]))
        if self.path == "/token":
            form = urllib.parse.parse_qs(raw.decode())
            if form.get("grant_type") == ["authorization_code"]:
                challenge = base64.urlsafe_b64encode(hashlib.sha256(form["code_verifier"][0].encode()).digest()).decode().rstrip("=")
                if challenge != self.server.challenge or form.get("code") != ["fixture-code"]:
                    self.json_response({"error": "invalid_grant"}, 400)
                    return
                self.json_response({"access_token": "initial-token", "refresh_token": "refresh-one", "expires_in": 3600})
            elif form.get("refresh_token") == ["refresh-one"] and not self.server.reject_refresh:
                self.server.refreshes += 1
                self.json_response({"access_token": "renewed-token", "refresh_token": "refresh-two", "expires_in": 3600})
            else:
                self.json_response({"error": "invalid_grant"}, 400)
            return
        msg = json.loads(raw)
        if self.path == "/register":
            self.json_response({"client_id": "fixture-client", "redirect_uris": msg["redirect_uris"]})
            return
        expected = self.server.oauth_token if self.path == "/oauth" else SECRET
        if self.headers.get("Authorization") != "Bearer " + expected:
            self.send_response(401)
            self.send_header("WWW-Authenticate", f'Bearer resource_metadata="http://127.0.0.1:{self.server.server_port}/.well-known/oauth-protected-resource"')
            self.end_headers()
            return
        method = msg["method"]
        if method != "initialize" and self.headers.get("Mcp-Session-Id") != "check-session":
            self.send_response(400)
            self.end_headers()
            return
        if "id" not in msg:
            self.send_response(202)
            self.end_headers()
            return
        if method == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "check", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": [{"name": "echo", "description": "Echo test input", "inputSchema": {"type": "object"}}]}
        else:
            result = {"content": [{"type": "text", "text": msg["params"]["arguments"]["text"]}]}
        wire = json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result})
        sse = self.path == "/sse-response"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if sse else "application/json")
        self.send_header("Mcp-Session-Id", "check-session")
        self.end_headers()
        self.wfile.write(("event: message\ndata: " + wire + "\n\n" if sse else wire).encode())


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


@contextlib.contextmanager
def proxy(env, port, connector, kind):
    process = subprocess.Popen([str(BINARY), "mcp", kind, "--connector", connector, "--port", str(port)],
                               env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               text=True, bufsize=1)
    replies = queue.Queue()
    def read():
        for line in process.stdout:
            try:
                replies.put(json.loads(line))
            except json.JSONDecodeError:
                pass
        replies.put(None)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    seq = 0
    def rpc(method, params=None, notification=False):
        nonlocal seq
        seq += 1
        msg = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if not notification:
            msg["id"] = seq
        process.stdin.write(json.dumps(msg) + "\n")
        process.stdin.flush()
        if notification:
            return None
        deadline = time.monotonic() + 100
        while True:
            reply = replies.get(timeout=max(.01, deadline - time.monotonic()))
            assert reply is not None, "MCP proxy exited before answering"
            if reply.get("id") == seq:
                assert "error" not in reply, str(reply)
                return reply["result"]
    try:
        rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "connector-check", "version": "1"}})
        rpc("notifications/initialized", notification=True)
        yield rpc
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            stop(process)
        reader.join(timeout=2)
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--network", action="store_true")
    parser.add_argument("--slow", action="store_true", help="also exercise the real 90-second handshake timeout")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This live check currently implements temporary key cleanup for macOS only.")
    assert BINARY.exists(), "Run cargo build in host first"
    results = []
    def check(name, action):
        started = time.monotonic()
        try:
            action()
            results.append((name, True))
            print(f"PASS {name} ({time.monotonic() - started:.1f}s)", flush=True)
        except Exception as error:
            results.append((name, False))
            print(f"FAIL {name}: {error}", flush=True)

    with tempfile.TemporaryDirectory(prefix="codync-connectors-check-") as temp:
        home = Path(temp)
        env = {**os.environ, "CODYNC_HOME": str(home), "CODYNC_CLOUD": "off", "CODYNC_AGENT_CHECKS": "off"}
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        log = open(home / "host.log", "w")
        host = None
        remote = ThreadingHTTPServer(("127.0.0.1", 0), Remote)
        remote.daemon_threads = True
        threading.Thread(target=remote.serve_forever, daemon=True).start()
        def start():
            process = subprocess.Popen([str(BINARY), "serve", "--bind", "127.0.0.1", "--port", str(port)], env=env, stdout=log, stderr=log)
            try:
                for _ in range(200):
                    try:
                        http(base + "/health", timeout=1)
                        return process
                    except (OSError, ValueError):
                        assert process.poll() is None, "Host exited at startup"
                        time.sleep(.1)
                raise TimeoutError("Host startup")
            except Exception:
                stop(process)
                raise
        try:
            host = start()
            token = (home / "token").read_text().strip()
            def api(method, body=None, ok=True):
                status, result = http(base + "/api/" + method, body or {}, token)
                assert (200 <= status < 300) == ok, f"{method}: {status} {result}"
                return result
            def install(body):
                return api("installConnector", body)["connector"]["id"]
            def verify(connector):
                result = api("connectorVerify", {"id": connector})
                assert result["status"] == "ready", result
                return result
            local = install({"name": "Credential fixture", "command": "python3", "args": [str(ROOT / "host/tests/fixtures/credential_mcp.py")], "env": {"API_KEY": "wrong"}})
            check("wrong local credential is rejected", lambda: api("connectorVerify", {"id": local}, ok=False))
            check("credential correction verifies without reinstall", lambda: api("credentialUpdateConnector", {"id": local, "fields": {"API_KEY": "test-private-key"}}))
            def edit_in_place():
                edited = api("updateConnector", {"id": local, "args": ["-u", str(ROOT / "host/tests/fixtures/credential_mcp.py")]})
                assert edited["connector"]["id"] == local, "an update keeps the id bots point at"
                assert edited["verify"]["status"] == "ready", edited
                assert api("connectorRuntime", {"id": local})["args"][0] == "-u", "the new command line is what runs"
                assert [c["id"] for c in api("connectors")["items"]] == [local], "an update adds nothing"
            check("updateConnector edits the command line in place and verifies it", edit_in_place)
            def failed_edit():
                before = api("connectorRuntime", {"id": local})
                api("updateConnector", {"id": local, "command": "/nonexistent/codync-connector-check"}, ok=False)
                assert api("connectorRuntime", {"id": local}) == before, "a connector that won't start isn't saved"
            check("a failed updateConnector leaves the connector as it was", failed_edit)
            def local_proxy():
                with proxy(env, port, local, "local") as rpc:
                    assert rpc("tools/list")["tools"] == []
            check("local proxy injects stored credential into real subprocess", local_proxy)
            def redaction():
                listing = json.dumps(api("connectors"))
                assert "test-private-key" not in listing
                assert "credential_mcp.py" not in listing
                with sqlite3.connect(home / "codync.db") as db:
                    raw = db.execute("SELECT v FROM kv WHERE k='connectors'").fetchone()[0]
                    assert raw.startswith("vault:v1:") and "test-private-key" not in raw
            check("listings redact secrets and SQLite stores ciphertext", redaction)
            stop(host)
            host = start()
            check("credentials survive host restart through OS keychain", lambda: verify(local))
            for path in ("/json", "/sse-response"):
                def remote_check(path=path):
                    connector = install({"name": path[1:], "url": f"http://127.0.0.1:{remote.server_port}{path}", "headers": {"Authorization": "Bearer " + SECRET}})
                    assert verify(connector)["toolCount"] == 1
                    with proxy(env, port, connector, "remote") as rpc:
                        assert rpc("tools/list")["tools"][0]["name"] == "echo"
                        result = rpc("tools/call", {"name": "echo", "arguments": {"text": "roundtrip"}})
                        assert result["content"][0]["text"] == "roundtrip"
                    api("removeConnector", {"id": connector})
                    api("connectorVerify", {"id": connector}, ok=False)
                check("remote authenticated handshake, session, tool call and removal " + path, remote_check)
            def missing():
                connector = install({"name": "Missing runtime", "command": str(home / "does-not-exist")})
                error = api("connectorVerify", {"id": connector}, ok=False)
                assert "runtime" in str(error).lower()
            check("missing runtime gives actionable verification failure", missing)
            def imported():
                body = {"mcpServers": {"Imported": {"command": "python3", "args": [str(ROOT / "host/tests/fixtures/credential_mcp.py")], "env": {"API_KEY": "test-private-key"}}}}
                connector = api("importConnectors", {"config": json.dumps(body)})["items"][0]["id"]
                verify(connector)
            check("pasted MCP config imports and verifies", imported)
            def malformed():
                fixture = home / "malformed.py"
                fixture.write_text('import sys,json\nfor line in sys.stdin:\n m=json.loads(line)\n if "id" in m: print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{}}),flush=True)\n')
                connector = install({"name": "Malformed handshake", "command": sys.executable, "args": [str(fixture)]})
                error = api("connectorVerify", {"id": connector}, ok=False)
                assert "Invalid MCP handshake" in str(error)
            check("malformed initialize response is rejected", malformed)
            if args.slow:
                def timeout():
                    fixture = home / "hung.py"
                    pid_file = home / "hung.pid"
                    fixture.write_text('import os,time,sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_text(str(os.getpid()))\ntime.sleep(180)\n')
                    connector = install({"name": "Hung handshake", "command": sys.executable, "args": [str(fixture), str(pid_file)]})
                    error = api("connectorVerify", {"id": connector}, ok=False)
                    assert "timed out" in str(error)
                    pid = int(pid_file.read_text())
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        return
                    raise AssertionError("Timed-out connector process was not reaped")
                check("90-second handshake timeout terminates and reaps subprocess", timeout)
            def oauth():
                remote.oauth_token = "initial-token"
                remote.refreshes = 0
                remote.reject_refresh = False
                connector = install({"name": "OAuth fixture", "url": f"http://127.0.0.1:{remote.server_port}/oauth"})
                api("connectorVerify", {"id": connector}, ok=False)
                plan = api("connectorSignIn", {"id": connector})
                query = urllib.parse.parse_qs(urllib.parse.urlparse(plan["url"]).query)
                assert query["code_challenge_method"] == ["S256"] and query["scope"] == ["read"]
                remote.challenge = query["code_challenge"][0]
                state = query["state"][0]
                api("connectorSignInFinish", {"state": "forged", "code": "fixture-code"}, ok=False)
                result = api("connectorSignInFinish", {"state": state, "code": "fixture-code"})
                assert result["connector"]["auth"] == "signedIn"
                api("connectorSignInFinish", {"state": state, "code": "fixture-code"}, ok=False)
                verify(connector)
                remote.oauth_token = "renewed-token"  # force a 401 before the expiry time
                with proxy(env, port, connector, "remote") as rpc:
                    assert rpc("tools/list")["tools"]
                assert remote.refreshes == 1
                listing = json.dumps(api("connectors"))
                assert "renewed-token" not in listing and "refresh-two" not in listing
                remote.oauth_token = "revoked-token"
                remote.reject_refresh = True
                api("connectorVerify", {"id": connector}, ok=False)
                current = next(c for c in api("connectors")["items"] if c["id"] == connector)
                assert current["auth"] == "signedOut"
                plan = api("connectorSignIn", {"id": connector})
                state = urllib.parse.parse_qs(urllib.parse.urlparse(plan["url"]).query)["state"][0]
                api("connectorSignInFinish", {"state": state, "error": "access_denied"}, ok=False)
                api("connectorVerify", {"id": connector}, ok=False)
            check("OAuth discovery, PKCE, state/replay rejection, 401 refresh, revocation and cancellation", oauth)
            if args.network:
                def filesystem():
                    folder = home / "allowed"
                    folder.mkdir()
                    (folder / "proof.txt").write_text("connector-live-proof")
                    connector = install({"name": "Official filesystem", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem@2026.8.31", str(folder)]})
                    verify(connector)
                    with proxy(env, port, connector, "local") as rpc:
                        names = [tool["name"] for tool in rpc("tools/list")["tools"]]
                        assert "read_text_file" in names
                        result = rpc("tools/call", {"name": "read_text_file", "arguments": {"path": str(folder / "proof.txt")}})
                        assert not result.get("isError") and "connector-live-proof" in json.dumps(result)
                        denied = rpc("tools/call", {"name": "read_text_file", "arguments": {"path": str(home / "token")}})
                        assert denied.get("isError"), "filesystem tool must reject paths outside allowed directory"
                check("pinned official npm connector installs, reads fixture, rejects outside path", filesystem)
                def public_remote():
                    connector = install({"name": "DeepWiki live", "url": "https://mcp.deepwiki.com/mcp"})
                    verify(connector)
                    with proxy(env, port, connector, "remote") as rpc:
                        assert any(t["name"] == "read_wiki_structure" for t in rpc("tools/list")["tools"])
                        result = rpc("tools/call", {"name": "read_wiki_structure", "arguments": {"repoName": "modelcontextprotocol/python-sdk"}})
                        assert not result.get("isError") and result.get("content"), result
                check("public HTTPS MCP handshake and read-only documentation call", public_remote)
                def registry():
                    item = api("connectorInfo", {"registryName": "io.github.upstash/context7"})
                    assert item["options"], item
                    option = next(o for o in item["options"] if o["kind"] == "npm")
                    connector = install({"registryName": item["name"], "option": option["id"], "inputs": {}})
                    verify(connector)
                check("MCP Registry lookup, pinned installation and verification", registry)
        finally:
            if host:
                stop(host)
            remote.shutdown()
            remote.server_close()
            log.close()
            if (home / "codync.db").exists():
                with sqlite3.connect(home / "codync.db") as db:
                    row = db.execute("SELECT v FROM kv WHERE k='vault.id'").fetchone()
                if row:
                    # The vault's service name follows the host's environment (vault.rs).
                    removed = [subprocess.run(["security", "delete-generic-password", "-s", service, "-a", row[0]], capture_output=True)
                               for service in ("dev.codync.development.credentials", "dev.codync.credentials")]
                    assert any(r.returncode == 0 for r in removed), "Could not remove temporary Keychain entry: " + row[0]
            print("Temporary host, data and Keychain key cleaned up.", flush=True)
    failed = sum(not ok for _, ok in results)
    print(f"{len(results) - failed}/{len(results)} passed", flush=True)
    return bool(failed)


if __name__ == "__main__":
    sys.exit(main())
