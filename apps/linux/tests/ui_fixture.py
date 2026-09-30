#!/usr/bin/env python3
"""Run the native GTK regression test against a deterministic, isolated host API."""
import base64
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BOT = dict(id="bot", name="Parity bot", backend="fixture", description="UI regression fixture", cwd="/tmp/project", status="working", workingChat=None, workingThread=None, avatarColor="blue", avatarShape="blob", permission="ask", notify=True, unread=0, lastAt=1, lastMessage="Fixture", kind="bot", computer=True)
BACKEND = dict(id="fixture", name="Fixture Agent", installed=True, available=True, curated=True, canInstall=True)
ENTRIES = [dict(id="user", botId="bot", seq=201, rev=201, turn=1, kind="user", createdAt=1720000000000, threadId=None, data=dict(text="Read this attachment", attachments=[dict(id="file", name="parity.txt", size=7)], reactions=["👍"])), dict(id="live", botId="bot", seq=202, rev=202, turn=1, kind="agent", createdAt=1720000000001, threadId=None, data=dict(text="Streaming fixture answer", final=False))]

class Host(BaseHTTPRequestHandler):
    requests = []
    facts = [dict(id="fact", content="Prefers native interfaces", kind="profile")]
    routines = []
    skills = []
    attempts = {}
    lock = threading.Lock()

    def log_message(self, *_):
        pass

    def send_json(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        method = self.path.rsplit("/", 1)[-1]
        cls = type(self)
        with cls.lock:
            cls.requests.append([method, body])
        if method == "_testState":
            return self.send_json(dict(requests=cls.requests))
        if method == "hello":
            return self.send_json(dict(name="Fixture computer", home="/tmp", version="test", backends=[BACKEND]))
        if method == "refreshBackends":
            return self.send_json(dict(backends=[BACKEND]))
        if method == "screenStatus":
            return self.send_json(dict(enabled=True, connected=True))
        if method == "history":
            return self.send_json(dict(entries=[dict(id="older", botId="bot", seq=1, rev=1, turn=0, kind="user", createdAt=1710000000000, data=dict(text="Older fixture message"))]))
        if method == "send":
            nonce = body["clientNonce"]
            cls.attempts[nonce] = cls.attempts.get(nonce, 0) + 1
            if cls.attempts[nonce] == 1:
                return self.send_json(dict(error="Fixture connection interrupted"), 503)
            return self.send_json(dict(entry=dict(id="sent", botId=body["botId"], threadId=body.get("threadId"), seq=300, rev=300, turn=2, kind="user", createdAt=1720000000002, data=dict(text=body["text"], clientNonce=nonce, status="queued"))))
        if method == "readUpload":
            return self.send_json(dict(data=base64.b64encode(b"fixture").decode(), size=7))
        if method == "memory":
            return self.send_json(dict(facts=cls.facts, location="fixture"))
        if method in ("forgetMemory", "clearMemory"):
            cls.facts = []
        if method == "routines":
            return self.send_json(dict(routines=cls.routines, runs=[]))
        if method == "saveRoutine":
            assert body["schedule"]["kind"] == "cron"
            assert body["timeoutSeconds"] == 3600
            routine = dict(id="routine", name=body["name"], instruction=body["instruction"], enabled=True, triggers=[dict(type="cron", expression="0 9 * * *", timeZone="UTC")], triggerDescriptions=["Daily at 09:00 UTC"], nextRunAt=1820000000000)
            cls.routines = [routine]
            return self.send_json(routine)
        if method == "routineSchedule":
            return self.send_json(dict(summary="Daily at 09:00 UTC", nextRunAt=1820000000000))
        if method == "setRoutineEnabled":
            cls.routines[0]["enabled"] = body["enabled"]
            cls.routines[0]["nextRunAt"] = 1820000000000 if body["enabled"] else None
        if method == "deleteRoutine":
            cls.routines = []
        if method == "skills":
            return self.send_json(dict(items=cls.skills))
        if method == "marketSkills":
            return self.send_json(dict(items=[dict(name="Review code", source="fixture/review", description="Review native UI changes")]))
        if method == "installSkill":
            cls.skills = [dict(id="skill", name="Review code", source=body["source"])]
        if method == "removeSkill":
            cls.skills = []
        if method == "agentAuth":
            return self.send_json(dict(signedIn=False, login=True, methods=[dict(id="env", name="API key", kind="envVar", vars=[dict(name="FIXTURE_KEY", label="API key", secret=True, optional=False)])]))
        if method == "agentSetup":
            return self.send_json(dict(term="fixture-terminal"))
        if method == "setAgentEnv":
            return self.send_json(dict(signedIn=True))
        if method in ("connectors", "marketConnectors"):
            return self.send_json(dict(items=[]))
        self.send_json({})

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        try:
            if self.path.startswith("/term/"):
                events = [dict(type="output", data=base64.b64encode(b"\x1b[32mFixture terminal ready\x1b[0m\r\n").decode())]
            else:
                events = [dict(type="bot", rev=1, bot=BOT), dict(type="bot", rev=2, bot={**BOT, "id":"other", "name":"Other bot", "status":"idle"})] + [dict(type="entry", rev=e["rev"], entry=e) for e in ENTRIES]
            for event in events:
                self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
            self.wfile.flush()
            while True:
                time.sleep(0.5)
                self.wfile.write(b": keepalive\n\n")
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", 0), Host)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="codync-ui-fixture-") as directory:
        pathlib.Path(directory, "token").write_text("fixture-token")
        downloads = pathlib.Path(directory, "downloads")
        downloads.mkdir()
        config = pathlib.Path(directory, "config")
        config.mkdir()
        (config / "user-dirs.dirs").write_text(f'XDG_DOWNLOAD_DIR="{downloads}"\n')
        env = {**os.environ, "XDG_CONFIG_HOME":str(config), "CODYNC_UI_DOWNLOADS":str(downloads), "CODYNC_HOME":directory, "CODYNC_URL":f"http://127.0.0.1:{server.server_port}", "CODYNC_UI_TEST":"1", "CODYNC_UI_ARTIFACTS":os.environ.get("CODYNC_UI_ARTIFACTS", directory)}
        try:
            result = subprocess.run(sys.argv[1:] or ["cargo", "test", "native_ui_flows", "--", "--ignored", "--test-threads=1", "--nocapture"], env=env)
        finally:
            server.shutdown()
        sys.exit(result.returncode)
