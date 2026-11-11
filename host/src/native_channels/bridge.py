"""Source: Hermes plugins/platforms/{signal,email}, gateway/platforms/base.py (MIT, 10d25f8a).
Adaptation: transport-only JSON lines; Codync owns routing, approval, persistence and authorization.
"""
from __future__ import annotations

import argparse
import asyncio
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import uuid

MAX_LINE = 512 * 1024
MAX_FILE = 20 * 1024 * 1024
WIRE = sys.stdout
sys.stdout = sys.stderr
pending = {}
output_lock = asyncio.Lock()


async def emit(value):
    line = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if len(line.encode()) > MAX_LINE:
        raise ValueError("Bridge metadata exceeds its limit")
    async with output_lock:
        WIRE.write(line + "\n")
        WIRE.flush()


async def ask(operation, **values):
    identifier = uuid.uuid4().hex
    future = asyncio.get_running_loop().create_future()
    pending[identifier] = future
    try:
        await emit({"op": operation, "id": identifier, **values})
        result = await asyncio.wait_for(future, 60)
        if not result.get("ok", False):
            raise RuntimeError("The host refused this transport operation")
        return result
    finally:
        pending.pop(identifier, None)


def source_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path, submodule_search_locations=[str(path.parent)])
    if spec is None or spec.loader is None:
        raise RuntimeError("Native platform module is missing")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def users(value):
    text = str(value or "").strip()
    if text.startswith("["):
        parsed = json.loads(text)
        if not isinstance(parsed, list):
            raise ValueError("Native sender list must be a list")
        return [str(item).strip() for item in parsed if str(item).strip()][:100]
    return [item.strip() for item in text.split(",") if item.strip()][:100]


def secret(name):
    from gateway.platforms._shared import get_scoped_secret
    return str(get_scoped_secret(name, "") or "")


def safe_identity(parts):
    # Only a digest leaves the process; even an endpoint token embedded in native config
    # cannot escape through status or the host's logs.
    return hashlib.sha256(json.dumps(parts, sort_keys=True).encode()).hexdigest()


async def run(args, initial):
    config = initial.get("config", {})
    if not isinstance(config, dict):
        raise ValueError("Invalid transport configuration")
    profile = config.get("CODYNC_CHANNEL_HERMES_HOME", "").strip()
    if profile:
        os.environ["HERMES_HOME"] = profile
    root = Path(args.root).resolve(strict=True)
    sys.path.insert(0, str(root))
    from native_profile import configure_profile, configure_logging
    import hermes_bootstrap
    hermes_bootstrap.harden_import_path()
    if not profile:
        configure_profile()
    configure_logging()
    from hermes_constants import get_hermes_home
    from hermes_cli.env_loader import load_hermes_dotenv
    for key, value in config.items():
        if key.startswith(("SIGNAL_", "EMAIL_", "HASS_")) and value:
            os.environ[key] = str(value)
    load_hermes_dotenv(hermes_home=get_hermes_home())
    from gateway.config import PlatformConfig, load_gateway_config
    ha_module = None
    if args.kind == "homeassistant":
        source_module("codync_homeassistant", Path(__file__).parent / "homeassistant" / "__init__.py")
        ha_module = __import__("codync_homeassistant.adapter", fromlist=["HomeAssistantAdapter"])
        from gateway.platform_registry import PlatformEntry, platform_registry
        if not platform_registry.is_registered("homeassistant"):
            platform_registry.register(PlatformEntry(name="homeassistant", label="Home Assistant", adapter_factory=ha_module.HomeAssistantAdapter, check_fn=ha_module.check_ha_requirements, source="builtin"))
    profile_platforms = load_gateway_config().platforms

    def platform_config(name):
        for platform, native in profile_platforms.items():
            if getattr(platform, "value", platform) == name:
                selected = copy.deepcopy(native)
                selected.enabled = True
                return selected
        return PlatformConfig(enabled=True)
    from gateway.platforms.event import MessageEvent, MessageType
    cache = Path(args.cache).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    loop = asyncio.get_running_loop()
    adapter = None

    async def authorize(user, chat, text="", chat_type="dm"):
        return await ask("authorize", user=str(user or ""), chat=str(chat or ""), text=str(text)[:32768], chatType=str(chat_type))

    async def ingest(event):
        source = event.source
        user = str(getattr(source, "user_id", "") or getattr(event, "user_id", "") or "")
        chat = str(getattr(source, "chat_id", "") or "")
        if args.kind == "homeassistant":
            user = "homeassistant"
        text = str(event.text or "")[:32768]
        verdict = await authorize(user, chat, text, getattr(source, "chat_type", "dm"))
        if not verdict.get("authorized") and not verdict.get("pairing"):
            return None
        media = []
        total = 0
        if verdict.get("authorized"):
            for index, value in enumerate(event.media_urls[:10]):
                path = Path(str(value)).resolve(strict=True)
                if not path.is_file():
                    continue
                size = path.stat().st_size
                if size < 1 or size > MAX_FILE or total + size > MAX_FILE:
                    continue
                total += size
                destination = cache / (uuid.uuid4().hex + path.suffix[:16])
                await asyncio.to_thread(shutil.copyfile, path, destination)
                media.append({"path": str(destination), "name": path.name[:255], "mime": event.media_types[index] if index < len(event.media_types) else "application/octet-stream"})
        message = str(event.message_id or "")
        if not message:
            message = safe_identity([chat, user, text, getattr(event, "reply_to_message_id", None)])
        response = await ask("incoming", event={
            "user": user, "chat": chat, "chatType": getattr(source, "chat_type", "dm"),
            "text": text, "messageId": message, "media": media,
            "replyTo": str(event.reply_to_message_id or ""), "quote": str(event.reply_to_text or "")[:4000],
        })
        if response.get("notice"):
            await adapter.send(chat, response["notice"], reply_to=event.message_id)
        return None

    if args.kind == "signal":
        from plugins.platforms.signal.adapter import SignalAdapter

        class OwnedSignal(SignalAdapter):
            async def _handle_envelope(self, envelope):
                # The native adapter extracts media before BasePlatformAdapter's auth gate.
                # Ask the host first and strip pairing-message attachments before delegation.
                copied = copy.deepcopy(envelope)
                data = copied.get("envelope", copied)
                sender = data.get("sourceNumber") or data.get("sourceUuid") or data.get("source")
                message = data.get("dataMessage") or (data.get("editMessage") or {}).get("dataMessage")
                if message is None:
                    return  # no Note-to-Self/sync echoes enter a Codync user conversation
                group = (message.get("groupInfo") or {}).get("groupId")
                chat = f"group:{group}" if group else sender
                verdict = await authorize(sender, chat, message.get("message", ""), "group" if group else "dm")
                if not verdict.get("authorized") and not verdict.get("pairing"):
                    return
                if not verdict.get("authorized"):
                    message["attachments"] = []
                await super()._handle_envelope(copied)

        selected = platform_config("signal")
        selected.extra = dict(selected.extra or {})
        selected.extra["http_url"] = secret("SIGNAL_HTTP_URL") or selected.extra.get("http_url") or "http://127.0.0.1:8080"
        selected.extra["account"] = secret("SIGNAL_ACCOUNT") or selected.extra.get("account", "")
        adapter = OwnedSignal(selected)
        native_users = users(secret("SIGNAL_ALLOWED_USERS")) + users(secret("GATEWAY_ALLOWED_USERS"))
        identity = safe_identity([adapter.http_url, adapter.account])
    elif args.kind == "email":
        from plugins.platforms.email import adapter as email_module

        class OwnedEmail(email_module.EmailAdapter):
            def _parse_fetched_message(self, uid, raw_email):
                if len(raw_email) > MAX_FILE:
                    return None
                msg = email_module.email_lib.message_from_bytes(raw_email)
                metadata = self._message_metadata(uid, msg)
                if metadata is None or not self._sender_accepted(metadata["sender_addr"], metadata):
                    return None
                text = email_module._extract_text_body(msg)[:32768]
                verdict = asyncio.run_coroutine_threadsafe(authorize(metadata["sender_addr"], metadata["sender_addr"], text), loop).result(60)
                if not verdict.get("authorized") and not verdict.get("pairing"):
                    return None
                metadata.update(body=text, attachments=email_module._extract_attachments(msg, skip_attachments=not verdict.get("authorized")))
                metadata["message_id"] = metadata["message_id"] or safe_identity([base64.b64encode(raw_email).decode()])
                return metadata

        selected = platform_config("email")
        selected.extra = dict(selected.extra or {})
        selected.extra["unauthorized_dm_behavior"] = "pair"
        adapter = OwnedEmail(selected)
        adapter.set_authorization_check(lambda *_args, **_kwargs: True)  # native From auth still precedes our host gate
        native_users = [user.lower() for user in users(secret("EMAIL_ALLOWED_USERS")) + users(secret("GATEWAY_ALLOWED_USERS"))]
        identity = safe_identity([adapter._address, adapter._imap_host, adapter._smtp_host, adapter._authserv_id])
    elif args.kind == "homeassistant":
        module = ha_module
        entities = users(config.get("HASS_WATCH_ENTITIES", ""))
        if not entities:
            raise ValueError("Choose Home Assistant entity IDs")
        adapter = module.HomeAssistantAdapter(PlatformConfig(enabled=True, token=secret("HASS_TOKEN"), extra={"url": secret("HASS_URL") or "http://homeassistant.local:8123", "watch_entities": entities, "watch_all": False}))
        native_users = ["homeassistant"]
        identity = safe_identity([adapter._hass_url, adapter._hass_token])
    else:
        raise ValueError("Unsupported native platform")

    # Transport adapters alone: native gateway command/session logic must not run beside Codync.
    if not native_users and any(secret(key).strip().lower() in {"1", "true", "yes"} for key in (f"{args.kind.upper()}_ALLOW_ALL_USERS", "GATEWAY_ALLOW_ALL_USERS")):
        native_users.append("*")
    adapter.handle_message = ingest
    adapter.set_message_handler(ingest)
    await emit({"op": "identity", "identity": identity, "nativeUsers": native_users})

    async def send(command):
        identifier = command.get("id")
        try:
            chat, text, reply = command["chat"], command.get("text", ""), command.get("replyTo") or None
            files = command.get("files", [])[:10]
            if args.kind == "email" and files:
                paths = [(Path(item["path"]), str(item.get("name") or Path(item["path"]).name)) for item in files]
                result_id = await asyncio.to_thread(adapter._send_with_files, chat, text, paths, lenient=False, reply_to_msg_id=reply)
                result = {"success": True, "messageId": result_id}
            else:
                sent = await adapter.send(chat, text or "Attached files", reply_to=reply)
                success = bool(sent.success)
                external = sent.message_id
                for item in files:
                    sent = await adapter.send_document(chat, item["path"], filename=item.get("name"), reply_to=reply)
                    success = success and bool(sent.success)
                    external = sent.message_id or external
                result = {"success": success, "messageId": external}
            await emit({"op": "sent", "id": identifier, **result})
        except Exception:
            await emit({"op": "sent", "id": identifier, "success": False})

    async def commands():
        while True:
            line = await asyncio.to_thread(sys.stdin.buffer.readline, MAX_LINE + 1)
            if not line:
                return
            if len(line) > MAX_LINE or not line.endswith(b"\n"):
                raise ValueError("Invalid transport frame")
            command = json.loads(line)
            if command.get("op") == "answer":
                future = pending.get(command.get("id"))
                if future is not None and not future.done():
                    future.set_result(command)
            elif command.get("op") == "send":
                # One writer; incoming authorization replies remain independently readable.
                try:
                    outbound.put_nowait(command)
                except asyncio.QueueFull:
                    await emit({"op":"sent", "id":command.get("id"), "success":False, "unsent":True})
            elif command.get("op") == "webhook" and args.kind == "email":
                try:
                    webhooks.put_nowait(command)
                except asyncio.QueueFull:
                    await emit({"op":"webhookDone", "id":command.get("id"), "success":False})

    async def sending():
        while True:
            await send(await outbound.get())

    async def accepting_webhooks():
        while True:
            command = await webhooks.get()
            try:
                path = Path(command["path"]).resolve(strict=True)
                if not path.is_relative_to(cache) or not path.is_file() or not 1 <= path.stat().st_size <= MAX_FILE:
                    raise ValueError("Invalid webhook spool")
                raw = await asyncio.to_thread(path.read_bytes)
                metadata = await asyncio.to_thread(adapter._parse_fetched_message, b"webhook", raw)
                if metadata:
                    await adapter._dispatch_message(metadata)
                await emit({"op":"webhookDone", "id":command.get("id"), "success":True})
            except Exception:
                await emit({"op":"webhookDone", "id":command.get("id"), "success":False})

    async def health():
        while True:
            await asyncio.sleep(5)
            await emit({"op":"health", "connected":bool(adapter.is_connected()) and not bool(getattr(adapter,"_last_fetch_failed",False))})

    outbound = asyncio.Queue(maxsize=32)
    webhooks = asyncio.Queue(maxsize=8)
    reader = asyncio.create_task(commands())
    sender = asyncio.create_task(sending())
    webhook_reader = asyncio.create_task(accepting_webhooks())
    monitor = asyncio.create_task(health())
    try:
        connected = await asyncio.wait_for(adapter.connect(), 60)
        if not connected:
            raise RuntimeError("Native platform connection failed")
        await emit({"op": "ready"})
        await reader
    finally:
        sender.cancel()
        reader.cancel()
        webhook_reader.cancel()
        monitor.cancel()
        await asyncio.gather(sender, reader, webhook_reader, monitor, return_exceptions=True)
        await asyncio.wait_for(adapter.disconnect(), 10)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--kind", choices=["signal", "email", "homeassistant"], required=True)
    parser.add_argument("--cache", required=True)
    args = parser.parse_args()
    line = sys.stdin.buffer.readline(MAX_LINE + 1)
    if not line or len(line) > MAX_LINE:
        raise ValueError("Missing bounded initialization frame")
    asyncio.run(run(args, json.loads(line)))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Do not forward exception strings: native YAML/URLs may contain credentials.
        WIRE.write(json.dumps({"op": "error", "error": "Native messaging transport failed. Check the native account, dependencies and permissions."}) + "\n")
        WIRE.flush()
        raise SystemExit(1)
