// What every end-to-end script needs: a local D1 + `wrangler dev`, Clerk test tokens (or the dev Clerk
// instance, for real apps), signed and unsigned calls to the local cloud, D1 queries, a file server, a lock
// for runs that share an outside resource, and the child processes to clean up.

import { type ChildProcess, execFile, spawn, spawnSync } from "node:child_process";
import { createPrivateKey, createPublicKey, generateKeyPairSync, type KeyObject } from "node:crypto";
import { closeSync, createReadStream, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, statSync, writeFileSync, writeSync } from "node:fs";
import { createServer as createHttpServer, type Server } from "node:http";
import { createServer } from "node:net";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as ref from "../ref.ts";
import { waitFor, type Json } from "./phone.ts";

export const CLOUD_DIR = fileURLToPath(new URL("../..", import.meta.url));
export const ROOT = join(CLOUD_DIR, "..");
/** Test machines are named with this prefix (the cloud's `CLOUD_MACHINE_PREFIX`), on every provider. */
export const TEST_PREFIX = "codync-e2e";
const WRANGLER = join(CLOUD_DIR, "node_modules/.bin/wrangler");
const ISSUER = "https://clerk.test.local"; // [env.local] CLERK_ISSUER

export function step(name: string) {
  console.log(`\n▶ ${name}`);
}

export function check(cond: unknown, what: string): asserts cond {
  if (!cond) throw new Error(`check failed: ${what}`);
  console.log(`  ✓ ${what}`);
}

export async function freePort(): Promise<number> {
  return new Promise((resolve) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      const port = (s.address() as { port: number }).port;
      s.close(() => resolve(port));
    });
  });
}

export async function stop(child: ChildProcess) {
  if (child.exitCode !== null || child.signalCode) return;
  const exited = new Promise((r) => child.once("exit", r));
  child.kill("SIGTERM");
  await Promise.race([exited, new Promise((r) => setTimeout(r, 5000))]);
  if (child.exitCode === null && !child.signalCode) child.kill("SIGKILL");
}

const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
// @clerk/backend strips PEM armor itself; a single line survives `--var`.
const oneLine = (pem: string) => pem.replace(/-----[A-Z ]+-----|\s/g, "");
const jwtKey = oneLine(publicKey.export({ type: "spki", format: "pem" }).toString());
// The cloud signs device certificates with the private half (DEVICE_CERT_KEY); hosts check them with the public
// half (CODYNC_DEVICE_CERT_KEY), as a self-hosted cloud's hosts do.
type KeyPair = { publicKey: KeyObject; privateKey: KeyObject };
const certKey: KeyPair = generateKeyPairSync("ed25519");

/** The device certificate key kept in `file`: a cloud that keeps its data across runs keeps its key, so the devices
 * its hosts accepted before still check out. */
function keptCertKey(file: string): KeyPair {
  if (existsSync(file)) {
    const privateKey = createPrivateKey({ key: JSON.parse(readFileSync(file, "utf8")) as JsonWebKey, format: "jwk" });
    return { privateKey, publicKey: createPublicKey(privateKey) };
  }
  const pair = generateKeyPairSync("ed25519");
  writeFileSync(file, JSON.stringify(pair.privateKey.export({ format: "jwk" })), { mode: 0o600 });
  return pair;
}

/** The dev Clerk instance's issuer and public key, so a local cloud accepts real apps signed in to Codync Dev. */
async function devClerk(): Promise<Record<string, string>> {
  const issuer = /\[env\.dev\][\s\S]*?CLERK_ISSUER = "([^"]+)"/.exec(readFileSync(join(CLOUD_DIR, "wrangler.toml"), "utf8"))?.[1];
  if (!issuer) throw new Error("no CLERK_ISSUER in wrangler.toml [env.dev]");
  const { keys } = (await (await fetch(`${issuer}/.well-known/jwks.json`)).json()) as { keys: JsonWebKey[] };
  const pem = createPublicKey({ key: keys[0]!, format: "jwk" }).export({ type: "spki", format: "pem" }).toString();
  return { CLERK_ISSUER: issuer, CLERK_JWT_KEY: oneLine(pem) };
}

/** Serves `files` (published name → absolute path) on `port`, for machines that reach this computer. */
export function serveFiles(files: Record<string, string>, port: number): Server {
  const server = createHttpServer((req, res) => {
    const file = files[(req.url ?? "").replace(/^\//, "")];
    if (!file) return void res.writeHead(404).end();
    res.writeHead(200, { "Content-Length": statSync(file).size, "Content-Type": "application/octet-stream" });
    createReadStream(file).pipe(res);
  });
  server.listen(port, "127.0.0.1");
  return server;
}

/** One run at a time on this computer for runs sharing an outside resource (Boat's trial allows 2 machines). */
export function lock(name: string): () => void {
  const dir = join(homedir(), ".codync-e2e");
  mkdirSync(dir, { recursive: true });
  const path = join(dir, `${name}.lock`);
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const fd = openSync(path, "wx");
      writeSync(fd, String(process.pid));
      closeSync(fd);
      return () => rmSync(path, { force: true });
    } catch {
      const holder = Number(readFileSync(path, "utf8"));
      if (holder && alive(holder)) throw new Error(`another ${name} run (pid ${holder}) is going; wait for it`);
      rmSync(path, { force: true }); // left by a run that died
    }
  }
  throw new Error(`couldn't take the ${name} lock`);
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** A local cloud on `port`, with its own D1 and temp folder. With `keep`, its D1 and device certificate key live in
 * that folder and outlast the run (a worktree sandbox's own cloud, `npm run dev`); else they go with the temp folder. */
export class LocalCloud {
  readonly tmp = mkdtempSync(join(tmpdir(), "codync-e2e-"));
  readonly persist: string;
  readonly port: number;
  readonly url: string;
  /** For hosts of this cloud: `CODYNC_DEVICE_CERT_KEY`. */
  readonly certPub: string;
  private readonly cert: KeyPair;
  private readonly children: ChildProcess[] = [];

  constructor(port: number, o: { keep?: string } = {}) {
    this.port = port;
    this.url = `http://127.0.0.1:${port}`;
    if (o.keep) mkdirSync(o.keep, { recursive: true });
    this.persist = join(o.keep ?? this.tmp, "wrangler");
    this.cert = o.keep ? keptCertKey(join(o.keep, "device-cert-key.json")) : certKey;
    this.certPub = this.cert.publicKey.export({ format: "jwk" }).x!;
  }

  run(cmd: string, args: string[], env: Record<string, string> = {}): ChildProcess {
    const child = spawn(cmd, args, { cwd: CLOUD_DIR, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
    const tag = cmd.split("/").pop();
    child.stdout?.on("data", (d) => process.env.E2E_VERBOSE && process.stdout.write(`[${tag}] ${d}`));
    child.stderr?.on("data", (d) => process.env.E2E_VERBOSE && process.stderr.write(`[${tag}] ${d}`));
    this.children.push(child);
    return child;
  }

  /** Migrates the local D1 and starts `wrangler dev` with `vars`, accepting test tokens from `token()` or, with
   * `clerk: "dev"`, sessions of the dev Clerk instance (real Codync Dev apps). */
  async start(vars: Record<string, string> = {}, o: { clerk?: "test" | "dev" } = {}) {
    const migrate = spawnSync(WRANGLER, ["d1", "migrations", "apply", "codync-local", "--local", "--env", "local", "--persist-to", this.persist], {
      cwd: CLOUD_DIR,
      encoding: "utf8",
      env: { ...process.env, CI: "1" },
    });
    if (migrate.status !== 0) throw new Error(`migrations failed: ${migrate.stderr}`);
    const clerk = o.clerk === "dev" ? await devClerk() : { CLERK_JWT_KEY: jwtKey };
    const cert = { DEVICE_CERT_KEY: JSON.stringify(this.cert.privateKey.export({ format: "jwk" })) };
    const varArgs = Object.entries({ ...clerk, ...cert, ...vars }).flatMap(([k, v]) => ["--var", `${k}:${v}`]);
    // `--test-scheduled`: `cron()` runs the Worker's cron jobs on demand.
    this.run(WRANGLER, ["dev", "--env", "local", "--ip", "127.0.0.1", "--port", String(this.port), "--persist-to", this.persist, "--test-scheduled", ...varArgs]);
    await waitFor("cloud /v1/health", () => fetch(`${this.url}/v1/health`).then((r) => r.ok || undefined).catch(() => undefined), 60_000);
  }

  /// Async on purpose: a blocking spawn stalls the event loop past wrangler's keep-alive timeout, and
  /// the next `fetch` then reuses a socket the server already closed (ECONNRESET).
  d1(sql: string): Promise<Json[]> {
    const args = ["d1", "execute", "codync-local", "--local", "--env", "local", "--persist-to", this.persist, "--json", "--command", sql];
    return new Promise((resolve, reject) =>
      execFile(WRANGLER, args, { cwd: CLOUD_DIR, encoding: "utf8" }, (err, stdout, stderr) => {
        if (err) return reject(new Error(`d1 failed: ${stderr}`));
        resolve((JSON.parse(stdout) as { results: Json[] }[])[0]?.results ?? []);
      }),
    );
  }

  /** Runs the cron job `cron` (as in `wrangler.toml`) once, now. */
  async cron(cron: string): Promise<void> {
    const res = await fetch(`${this.url}/__scheduled?cron=${encodeURIComponent(cron)}`);
    if (!res.ok) throw new Error(`cron ${cron}: ${res.status}`);
  }

  /** The Worker's latest console lines (wrangler keeps them for agents' sessions), to print when a run fails. */
  async logs(limit = 40): Promise<string> {
    const res = await fetch(`${this.url}/cdn-cgi/local/explorer/api/local/observability/query`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sql: `SELECT * FROM logs ORDER BY rowid DESC LIMIT ${limit}` }),
    }).catch(() => undefined);
    return res ? await res.text() : "(no logs)";
  }

  async token(sub: string): Promise<string> {
    const key = await crypto.subtle.importKey("jwk", privateKey.export({ format: "jwk" }), { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, [
      "sign",
    ]);
    const now = Math.floor(Date.now() / 1000);
    const part = (v: unknown) => ref.b64url(ref.enc.encode(JSON.stringify(v)));
    const head = `${part({ alg: "RS256", typ: "JWT" })}.${part({ iss: ISSUER, sub, sid: `sess_${sub}`, iat: now - 5, nbf: now - 5, exp: now + 3600, email: `${sub}@example.com` })}`;
    return `${head}.${ref.b64url(new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, ref.enc.encode(head))))}`;
  }

  async call(method: string, path: string, o: { token?: string; key?: ref.SignKey; body?: unknown } = {}): Promise<{ status: number; body: Json }> {
    const bytes = o.body === undefined ? new Uint8Array() : ref.enc.encode(JSON.stringify(o.body));
    const headers: Record<string, string> = {};
    if (o.token) headers.Authorization = `Bearer ${o.token}`;
    if (o.body !== undefined) headers["Content-Type"] = "application/json";
    if (o.key) headers["Codync-Sig"] = ref.signRequest(o.key, { method, authority: `127.0.0.1:${this.port}`, pathAndQuery: path, body: bytes });
    const res = await fetch(this.url + path, { method, headers, body: bytes.length ? bytes : undefined });
    return { status: res.status, body: await res.json() };
  }

  async cleanup() {
    await Promise.all(this.children.map(stop));
    rmSync(this.tmp, { recursive: true, force: true });
  }
}
