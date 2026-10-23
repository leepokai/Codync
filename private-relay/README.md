# Codync private relay

A single-process relay for one person's computers. It speaks the same WebSocket subset the Codync host and phones already use (`docs/reference/remote-relay.md`). It does not decrypt chat. One process can hold several computers; each `computerId` is its own room.

Phase 1 does not do accounts, routine webhooks, TURN, or push. Push stays on the existing APNs worker. Remote screen over the relay needs TURN, which this service does not issue (`POST /v1/host/screen-ice` is 404). Direct Wi-Fi and Tailscale screen still work.

## Run

Node.js 22 or newer. The process listens on `127.0.0.1` only. Put TLS and the public hostname on a reverse proxy.

```bash
cd private-relay
npm ci
export CODYNC_PRIVATE_RELAY_DATA=/var/lib/codync-private-relay
export PORT=8788
npm start
```

The database is `$CODYNC_PRIVATE_RELAY_DATA/relay.sqlite` (mode `0600` on Unix). When `CODYNC_PRIVATE_RELAY_DATA` is unset it uses `/var/lib/codync-private-relay`.

`GET /health` returns `{"ok":true}`.

## systemd

Run it as its own user, not root.

```ini
[Unit]
Description=Codync private relay
After=network-online.target

[Service]
Type=simple
User=codync-relay
Group=codync-relay
Environment=PORT=8788
Environment=CODYNC_PRIVATE_RELAY_DATA=/var/lib/codync-private-relay
ExecStart=/usr/bin/node --experimental-strip-types /opt/codync-private-relay/src/index.ts
Restart=always
RestartSec=2
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

Create `/var/lib/codync-private-relay` owned by that user (`0700`) before starting.

## Nginx

TLS terminates here. Do not enable `http2` on this server: the relay is a WebSocket upgrade. Proxy the `Host` header as `$host` (no port), because that is the authority `Codync-Sig` signs.

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    server_name relay.example.com;

    ssl_certificate     /etc/letsencrypt/live/relay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8788;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

Open TCP 443 to the internet. Leave 8788 on loopback.

## Point a computer at it

On the computer (not on the VPS), in Codync's Computers settings, turn on Reach from anywhere and set the relay address to `https://relay.example.com`. Or:

```bash
codync-host cloud --url https://relay.example.com
```

`--clear-url` forgets the saved address and uses this build's Codync cloud again. Each computer keeps its own `identity.json`. Do not copy that file to a second machine; register the second machine on its own. Both can use this same relay. They are separate rooms.

Phones learn the address from the pairing code and from the next `hello`. They do not have their own address field.
