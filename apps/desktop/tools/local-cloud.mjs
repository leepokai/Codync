// A worktree sandbox's own local cloud (docs/guides/cloudflare-testing.md#which-cloud-a-change-is-tested-on): a branch
// that changes cloud/ tests it on a local Worker, never on the shared dev cloud. It runs cloud/'s `npm run cloud:stack`
// without provider keys, its D1 and device certificate key kept in the sandbox folder across runs.
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, openSync, readFileSync, rmSync } from 'node:fs'
import { createServer } from 'node:net'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

// Any of these would turn cloud-stack.ts into a provider stack (a machine lock, tunnels, test machines made and swept);
// the Clerk secret key stays in dev.mjs.
const NOT_FOR_THE_CLOUD = ['BOAT_API_KEY', 'BOAT_API_KEY_FILE', 'INSTABOX_API_KEY', 'INSTABOX_API_KEY_FILE', 'CODYNC_HOST_LINUX_BIN', 'CODYNC_CLERK_SECRET_KEY']

/** Whether this checkout changes cloud/ against origin/dev, committed or not. */
export function changesCloud(root) {
  const git = (...args) => spawnSync('git', ['-C', root, ...args], { encoding: 'utf8' }).stdout?.trim() ?? ''
  const base = git('merge-base', 'origin/dev', 'HEAD')
  return Boolean(base && (git('diff', '--name-only', base, '--', 'cloud') || git('status', '--porcelain', '--', 'cloud')))
}

/** Whether `port` is free where wrangler listens: a check on 0.0.0.0 misses a cloud left on loopback. */
const loopbackFree = (port) => new Promise((done) => {
  const server = createServer().once('error', () => done(false))
  server.listen(port, '127.0.0.1', () => server.close(() => done(true)))
})

/** Starts the local cloud on `port`, keeping its data in `dir`; resolves once it answers, to its URL, the public key
 * hosts check its device certificates with, its log and how to stop it. */
async function startLocalCloud({ root, dir, port }) {
  const cloud = join(root, 'cloud')
  if (!existsSync(join(cloud, 'node_modules'))) {
    if (spawnSync('npm', ['ci'], { cwd: cloud, stdio: 'inherit' }).status !== 0) throw new Error('npm ci in cloud/ failed')
  }
  mkdirSync(dir, { recursive: true })
  const info = join(dir, 'local-cloud.json')
  const log = join(dir, 'local-cloud.log')
  rmSync(info, { force: true })
  const out = openSync(log, 'a')
  // E2E_VERBOSE: wrangler's output, the Worker's errors included, goes to the log too.
  const env = { ...process.env, E2E_VERBOSE: '1', CODYNC_STACK_PORT: String(port), CODYNC_STACK_DIR: dir, CODYNC_STACK_INFO: info }
  for (const name of NOT_FOR_THE_CLOUD) delete env[name]
  // `npm run cloud:stack`, started without npm so a signal reaches it directly and it cleans up.
  const child = spawn(process.execPath, ['--experimental-strip-types', '--no-warnings', 'test/e2e/cloud-stack.ts'], { cwd: cloud, env, stdio: ['ignore', out, out] })
  const stop = () => child.kill('SIGINT')
  // Also when the sandbox exits without its stop (a failed start): the cloud never outlives it.
  process.on('exit', stop)
  for (let tries = 0; !existsSync(info); tries++) {
    if (child.exitCode !== null || child.signalCode !== null || tries === 240) {
      stop()
      throw new Error(`the local cloud didn't start; see ${log}`)
    }
    await delay(500)
  }
  return { ...JSON.parse(readFileSync(info, 'utf8')), log, stop }
}

/** What the sandbox's host and app get for its cloud `mode` (dev-sandbox.ts `sandboxCloudMode`), the lines to print
 * about it, and how to stop it; `local` starts the sandbox's own cloud on its port + 20000. */
export async function sandboxCloud({ mode, root, home, port }) {
  if (mode === 'dev') return { hostEnv: {}, appEnv: {}, note: '', hints: [], stop() {} }
  if (mode === 'off') return { hostEnv: { CODYNC_CLOUD: 'off' }, appEnv: {}, note: ', dev cloud off', hints: [], stop() {} }
  // Beside the CDP port (+10000), so a simulator launched with this URL keeps working across runs.
  const cloudPort = port + 20000
  if (!(await loopbackFree(cloudPort))) throw new Error(`port ${cloudPort} is in use. Stop what holds it, or run with --cloud.`)
  console.log("Starting the sandbox's own local cloud (the default on a branch that changes cloud/; --cloud uses the dev cloud instead)...")
  const { url, certPub, log, stop } = await startLocalCloud({ root, dir: join(home, 'cloud'), port: cloudPort })
  return {
    hostEnv: { CODYNC_CLOUD_URL: url, CODYNC_DEVICE_CERT_KEY: certPub },
    appEnv: { CODYNC_CLOUD_URL: url },
    note: `, its own cloud at ${url} (log: ${log})`,
    hints: [
      `  iPhone on its cloud        SIMCTL_CHILD_CODYNC_CLOUD_URL=${url} xcrun simctl launch <udid> com.pokai.Codync.ios.dev`,
      `  Android on its cloud       adb reverse tcp:${cloudPort} tcp:${cloudPort}; build with -PcodyncCloudUrl=${url} plus -PcodyncClerkPublishableKey and -PcodyncPushRelayUrl`,
    ],
    stop,
  }
}
