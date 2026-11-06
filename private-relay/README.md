# Codync private relay

A single-process relay for one person's computers. It speaks the same WebSocket subset the Codync host and phones already use (`docs/reference/remote-relay.md`). It does not decrypt chat. One process can hold several computers; each `computerId` is its own room.

It does not do accounts, routine webhooks, TURN, or push. Push stays on the existing APNs worker. Remote screen over the relay needs TURN, which this service does not issue (`POST /v1/host/screen-ice` is 404). Direct Wi-Fi and Tailscale screen still work. It does bounce a connector sign-in on the phone back to the app (`/v1/oauth/callback`), as the Codync cloud does.

## Run

Node.js 22.13 or newer (built-in `node:sqlite` and `--experimental-strip-types`). The process listens on `127.0.0.1` only. Put TLS and the public hostname on a reverse proxy.

```bash
cd private-relay
npm ci
export CODYNC_PRIVATE_RELAY_DATA=/var/lib/codync-private-relay
export CODYNC_PRIVATE_RELAY_TOKEN='a-long-random-secret'
export CODYNC_PRIVATE_RELAY_PUBLIC_URL=https://relay.example.com
export PORT=8788
npm start
```

`CODYNC_PRIVATE_RELAY_PUBLIC_URL` is the address computers put in their Relay address (`https://relay.example.com`, or `https://relay.example.com:8443`). Every `Codync-Sig` must be signed for it, so a signature made for another relay is refused here. The request's `Host` header is not used. Unset, only the loopback address itself verifies (`http://127.0.0.1:8788`), which suits a relay on the same machine and refuses everything that arrives through a proxy.

`CODYNC_PRIVATE_RELAY_TOKEN` is required: 16 to 256 visible ASCII characters (no spaces). A computer this relay does not already know can register only by presenting that token. Without it, anyone who can reach the relay could create a computer and hold a room. The process refuses to start without such a token. At most 32 computers are stored. One that is already registered can refresh its row and reconnect with its own key, without the token.

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
Environment=CODYNC_PRIVATE_RELAY_TOKEN=a-long-random-secret
Environment=CODYNC_PRIVATE_RELAY_PUBLIC_URL=https://relay.example.com
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

TLS terminates here. Do not enable `http2` on this server: the relay is a WebSocket upgrade.

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

On the computer (not on the VPS), in Codync's Computers settings, turn on Reach from anywhere, set Relay address to `https://relay.example.com`, and paste the same value as `CODYNC_PRIVATE_RELAY_TOKEN` into Relay token. The token stays on that computer: it is sent only in the `Codync-Relay-Token` header when the computer enrolls, never in a URL, and phones never see it. Or:

```bash
codync-host cloud --url https://relay.example.com --token
```

`--token` reads the token without echo (or one line from stdin when piped), so it does not land in shell history. `--clear-url` forgets the saved address and the token, and uses this build's Codync cloud again. Clearing the Relay address field does the same, and moving to another address drops the old token. Each computer keeps its own `identity.json`. Do not copy that file to a second machine; register the second machine on its own, with the token. Both can use this same relay. They are separate rooms.

Accounts, public routine webhooks and remote screen through the relay are not served (`404`). Direct Wi-Fi and Tailscale screen still work. Push stays on the existing notification worker. Devices approved through a Codync account stay on the computer but can't connect while it uses this relay; they can again once it goes back to the Codync relay. Devices paired with a QR code are not affected.

Phones learn the address from the pairing code and from the next `hello`. They do not have their own address field.

## Manage computers

The relay keeps each computer in `relay.sqlite`. Changing `CODYNC_PRIVATE_RELAY_TOKEN` only stops new enrollments: computers already registered keep refreshing and connecting with their own keys. To take one out (a lost machine, or a leaked token used to fill the 32 places), use `sqlite3` on the relay, then restart it so a live connection drops:

```bash
cd /var/lib/codync-private-relay
sqlite3 relay.sqlite "SELECT id, name, platform, status, datetime(updated_at / 1000, 'unixepoch') FROM computers"
# Keep it out: it can no longer register, connect, or be reached by its devices.
sqlite3 relay.sqlite "UPDATE computers SET status = 'disabled' WHERE id = '<computerId>'"
# Or forget it and free its place; it can enroll again only with the token.
sqlite3 relay.sqlite "DELETE FROM computers WHERE id = '<computerId>'; DELETE FROM acl WHERE computer_id = '<computerId>'; DELETE FROM mailbox WHERE computer_id = '<computerId>'"
sudo systemctl restart codync-private-relay
```
