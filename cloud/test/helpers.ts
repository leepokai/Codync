// Shared test plumbing: Clerk tokens, signed requests, WebSocket wrappers, a registered host.

import { SELF, type D1Migration } from "cloudflare:test";
import { env as rawEnv } from "cloudflare:workers";
import type { Env } from "../src/index";
import { aclMessage, b64url, boxKey, computerId, enc, signKey, signRequest, type AclBody, type BoxKey, type SignKey } from "./ref";

export interface TestEnv extends Env {
  TEST_MIGRATIONS: D1Migration[];
  TEST_CLERK_PRIVATE_JWK: string;
}

export const env = rawEnv as unknown as TestEnv;
export const AUTHORITY = "cloud.test";
export const ORIGIN = `https://${AUTHORITY}`;

// ---- Clerk ----

let clerkKey: Promise<CryptoKey> | undefined;

export async function clerkToken(
  sub: string,
  claims: Record<string, unknown> = {},
): Promise<string> {
  clerkKey ??= crypto.subtle.importKey(
    "jwk",
    JSON.parse(env.TEST_CLERK_PRIVATE_JWK),
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const now = Math.floor(Date.now() / 1000);
  const part = (v: unknown) => b64url(enc.encode(JSON.stringify(v)));
  const head = `${part({ alg: "RS256", typ: "JWT", kid: "ins_test" })}.${part({
    iss: env.CLERK_ISSUER,
    sub,
    sid: `sess_${sub}`,
    iat: now - 5,
    nbf: now - 5,
    exp: now + 300,
    email: `${sub}@example.com`,
    ...claims,
  })}`;
  const sig = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", await clerkKey, enc.encode(head));
  return `${head}.${b64url(new Uint8Array(sig))}`;
}

// ---- HTTP ----

export interface CallOptions {
  token?: string;
  key?: SignKey;
  body?: unknown;
  authority?: string;
  ts?: number;
  nonce?: string;
  headers?: Record<string, string>;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Json = any;

export async function call(method: string, path: string, o: CallOptions = {}): Promise<{ status: number; body: Json }> {
  const bytes = o.body === undefined ? new Uint8Array() : enc.encode(JSON.stringify(o.body));
  // A fresh client IP per call so the per-IP registration limit only bites where a test means it to.
  const headers: Record<string, string> = { "CF-Connecting-IP": `10.0.${Math.floor(Math.random() * 255)}.${Math.floor(Math.random() * 255)}`, ...o.headers };
  if (o.token) headers.Authorization = `Bearer ${o.token}`;
  if (o.body !== undefined) headers["Content-Type"] = "application/json";
  if (o.key) {
    headers["Codync-Sig"] = signRequest(o.key, {
      method,
      authority: o.authority ?? AUTHORITY,
      pathAndQuery: path,
      body: bytes,
      ts: o.ts,
      nonce: o.nonce,
    });
  }
  const res = await SELF.fetch(ORIGIN + path, { method, headers, body: bytes.length ? bytes : undefined });
  return { status: res.status, body: await res.json() };
}

// ---- WebSockets ----

/** A client socket with a message queue, so tests can await specific messages. */
export class Sock {
  readonly msgs: Json[] = [];
  close?: { code: number; reason: string };
  private wake: (() => void)[] = [];

  constructor(readonly ws: WebSocket) {
    ws.accept();
    ws.addEventListener("message", (e) => {
      this.msgs.push(JSON.parse(e.data as string));
      this.notify();
    });
    ws.addEventListener("close", (e) => {
      this.close = { code: e.code, reason: e.reason };
      this.notify();
    });
  }

  private notify() {
    for (const w of this.wake.splice(0)) w();
  }

  private async until<T>(check: () => T | undefined, ms: number, what: string): Promise<T> {
    const deadline = Date.now() + ms;
    for (;;) {
      const v = check();
      if (v !== undefined) return v;
      const left = deadline - Date.now();
      if (left <= 0) throw new Error(`timed out waiting for ${what}; got ${JSON.stringify(this.msgs)} close=${JSON.stringify(this.close)}`);
      await new Promise<void>((resolve) => {
        const t = setTimeout(resolve, left);
        this.wake.push(() => {
          clearTimeout(t);
          resolve();
        });
      });
    }
  }

  /** Removes and returns the first queued message matching `pred`. */
  next(pred: (m: Json) => boolean = () => true, ms = 3000): Promise<Json> {
    return this.until(
      () => {
        const i = this.msgs.findIndex(pred);
        return i < 0 ? undefined : this.msgs.splice(i, 1)[0];
      },
      ms,
      "a message",
    );
  }

  nextT(t: string, ms = 3000): Promise<Json> {
    return this.next((m) => m.t === t, ms);
  }

  closed(ms = 3000): Promise<number> {
    return this.until(() => this.close?.code, ms, "close");
  }

  /** Asserts nothing matching `pred` arrives within `ms`. */
  async none(pred: (m: Json) => boolean, ms = 200): Promise<void> {
    await new Promise((r) => setTimeout(r, ms));
    const hit = this.msgs.find(pred);
    if (hit) throw new Error(`unexpected message ${JSON.stringify(hit)}`);
  }

  send(m: unknown) {
    this.ws.send(typeof m === "string" ? m : JSON.stringify(m));
  }
}

export async function upgrade(
  path: string,
  key: SignKey,
  o: { authority?: string; nonce?: string } = {},
): Promise<{ status: number; sock?: Sock; body?: Json }> {
  const res = await SELF.fetch(ORIGIN + path, {
    headers: {
      Upgrade: "websocket",
      "Codync-Sig": signRequest(key, { method: "GET", authority: o.authority ?? AUTHORITY, pathAndQuery: path, nonce: o.nonce }),
    },
  });
  if (res.status !== 101) return { status: res.status, body: await res.json() };
  return { status: 101, sock: new Sock(res.webSocket!) };
}

export async function socket(path: string, key: SignKey): Promise<Sock> {
  const r = await upgrade(path, key);
  if (!r.sock) throw new Error(`upgrade ${path} failed: ${r.status} ${JSON.stringify(r.body)}`);
  return r.sock;
}

// ---- a host ----

export interface TestHost {
  sign: SignKey;
  box: BoxKey;
  cid: string;
  ver: number;
}

export async function newHost(sign = signKey(), box = boxKey()): Promise<TestHost> {
  const r = await call("POST", "/v1/host/register", {
    key: sign,
    body: { boxKey: b64url(box.pub), name: "Mac", platform: "macos", device: "laptop", version: "2.2.0" },
  });
  if (r.status !== 200) throw new Error(`register failed: ${JSON.stringify(r.body)}`);
  return { sign, box, cid: computerId(sign.pub), ver: 0 };
}

export function acl(host: TestHost, devices: AclBody["devices"] = [], offers: AclBody["offers"] = []) {
  host.ver += 1;
  return aclMessage(host.sign, { v: 1, computerId: host.cid, ver: host.ver, devices, offers });
}

/** Connects the host, publishes an ACL and (unless `ready: false`) marks it ready. */
export async function hostSocket(
  host: TestHost,
  devices: AclBody["devices"] = [],
  o: { offers?: AclBody["offers"]; ready?: boolean } = {},
): Promise<Sock> {
  const sock = await socket("/v1/relay/host?v=1", host.sign);
  sock.send(acl(host, devices, o.offers));
  await sock.nextT("acl.ok");
  if (o.ready !== false) sock.send({ t: "ready" });
  return sock;
}

export const devicePath = (host: TestHost, pair?: string) =>
  `/v1/relay/device/${host.cid}?v=1${pair ? `&pair=${pair}` : ""}`;

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
