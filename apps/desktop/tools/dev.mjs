// `npm run dev`. The main checkout runs the default Codync Dev (its background host on 19223,
// ~/.codync-dev). A linked worktree runs a sandbox beside it instead: host data in
// ~/.codync-dev-<name>, a port kept in that folder's sandbox.json, its own app data, and a host
// this run builds, starts and stops. Flags (`npm run dev -- --list`): --shared takes the default
// Codync Dev from a worktree, --sandbox runs a sandbox from the main checkout, --name <name> runs
// the sandbox of that name, --cloud lets the sandbox host use the dev cloud, --cdp opens the
// DevTools protocol on the sandbox port + 10000, --onboarding shows the welcome flow (a sandbox
// starts past it), --list shows every sandbox, --prune deletes the ones whose worktree is gone
// (and their Clerk test accounts).
import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, openSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { homedir } from 'node:os'
import { basename, dirname, join, resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { deleteTestAccount, mintTicket, secretKey } from './dev-account.mjs'
import { SANDBOX_PREFIX, sandboxName, sandboxPorts } from '../src/shared/dev-sandbox.ts'

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const root = resolve(desktop, '../..')
const windows = process.platform === 'win32'
const { values: flags } = parseArgs({
  options: {
    shared: { type: 'boolean' }, sandbox: { type: 'boolean' }, name: { type: 'string' },
    cloud: { type: 'boolean' }, onboarding: { type: 'boolean' }, cdp: { type: 'boolean' }, list: { type: 'boolean' }, prune: { type: 'boolean' },
  },
})

/** The environment for the host and the app: the Clerk secret key stays in this script, since the host hands its environment to every agent it runs. */
function childEnv() {
  const { CODYNC_CLERK_SECRET_KEY: _key, ...env } = process.env
  return env
}

function fail(message) {
  console.error(message)
  process.exit(1)
}

function run(cmd, args, env = process.env) {
  if (spawnSync(cmd, args, { stdio: 'inherit', env }).status !== 0) fail(`${basename(cmd)} ${args.join(' ')} failed.`)
}

/** Every sandbox on this computer: a `~/.codync-dev-<name>` folder with a sandbox.json. */
function sandboxes() {
  return readdirSync(homedir())
    .filter((folder) => folder.startsWith(SANDBOX_PREFIX) && existsSync(join(homedir(), folder, 'sandbox.json')))
    .map((folder) => {
      const home = join(homedir(), folder)
      return { name: folder.slice(SANDBOX_PREFIX.length), home, ...JSON.parse(readFileSync(join(home, 'sandbox.json'), 'utf8')) }
    })
}

const answers = (port) => fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(1000) }).then((r) => r.ok, () => false)

/** The host listens on every IPv4 interface, so that is where the port must be free (`::` and loopback can bind beside it). */
const bindable = (port) => new Promise((done) => {
  const server = createServer().once('error', () => done(false))
  server.listen(port, '0.0.0.0', () => server.close(() => done(true)))
})
const isFree = async (port) => (await bindable(port)) && !(await answers(port))

async function firstFreePort() {
  for (const port of sandboxPorts(sandboxes().map((s) => s.port))) if (await isFree(port)) return port
  return fail('Every sandbox port (19300-19399) is taken. Delete old sandboxes: npm run dev -- --prune')
}

/** The host this worktree's code builds (or `CODYNC_HOST_BIN`), compiled for Codync Dev. */
function buildHost() {
  if (process.env.CODYNC_HOST_BIN) return process.env.CODYNC_HOST_BIN
  if (process.platform === 'linux') {
    run('cargo', ['build', '--manifest-path', join(root, 'host/Cargo.toml')], { ...process.env, CODYNC_ENV: 'dev' })
    return join(process.env.CARGO_TARGET_DIR ?? join(root, 'host/target'), 'debug/codync-host')
  }
  run(process.execPath, [join(desktop, 'tools/native-host.mjs'), 'dev'])
  return join(desktop, 'build/native', windows ? 'codync-host.exe' : 'codync-host')
}

/** electron-vite leaves Electron running when it stops alone, so the app gets its own process group and stops as a whole. */
function startApp(env = process.env) {
  const child = spawn('electron-vite', ['dev'], { env, stdio: 'inherit', shell: windows, detached: !windows })
  const stop = () => {
    if (windows) return void spawnSync('taskkill', ['/pid', String(child.pid), '/t', '/f'])
    try {
      process.kill(-child.pid, 'SIGTERM')
    } catch (e) {
      // ESRCH: the group is gone. EPERM: macOS answers this for a group left with only zombies (Electron quit at once).
      if (e.code !== 'ESRCH' && e.code !== 'EPERM') throw e
    }
  }
  return { child, stop }
}

async function listAndPrune() {
  for (const s of sandboxes()) {
    const running = await answers(s.port)
    const gone = !existsSync(s.worktree)
    if (flags.prune && gone && !running) {
      let account = false
      try {
        account = await deleteTestAccount(s.name)
      } catch (e) {
        // The sandbox folder is how a later --prune finds the account again.
        console.error(`Kept the sandbox ${s.name}: its Clerk test account wasn't deleted (${e.message}). Fix that, then run --prune again.`)
        continue
      }
      rmSync(s.home, { recursive: true, force: true })
      console.log(`Deleted the sandbox ${s.name} (${s.home})${account ? ' and its Clerk test account' : ''}`)
      if (!account && !secretKey()) console.log(`  Without CODYNC_CLERK_SECRET_KEY its Clerk test account stays; delete ${s.name}'s sandbox-*+clerk_test user in the Clerk dashboard.`)
    } else if (flags.list) {
      console.log(`${s.name.padEnd(28)} ${String(s.port).padEnd(6)} ${(running ? 'running' : 'stopped').padEnd(8)} ${s.worktree}${gone ? ' (gone)' : ''}`)
    }
  }
}

async function sandbox() {
  const name = sandboxName(flags.name ?? basename(root))
  const home = join(homedir(), SANDBOX_PREFIX + name)
  const recordFile = join(home, 'sandbox.json')
  const record = existsSync(recordFile) ? JSON.parse(readFileSync(recordFile, 'utf8')) : null
  if (record && resolve(record.worktree) !== root) fail(`The sandbox ${name} belongs to ${record.worktree}. Pick another one: npm run dev -- --name <name>`)
  const port = record?.port ?? (await firstFreePort())
  if (!(await isFree(port))) {
    fail(`Port ${port} of the sandbox ${name} is in use: another npm run dev runs this sandbox, or an earlier run left its host running. Stop that process, or run another sandbox with --name <name>.`)
  }
  mkdirSync(home, { recursive: true })
  if (!record) writeFileSync(recordFile, `${JSON.stringify({ worktree: root, port }, null, 2)}\n`)

  const host = buildHost()
  const log = join(home, 'host.log')
  const hostEnv = { ...childEnv(), CODYNC_HOME: home, ...(flags.cloud ? {} : { CODYNC_CLOUD: 'off' }) }
  const out = openSync(log, 'a')
  const hostProcess = spawn(host, ['serve', '--port', String(port)], { env: hostEnv, stdio: ['ignore', out, out] })
  hostProcess.on('error', (e) => fail(`Couldn't start ${host}: ${e.message}`))
  let app = null
  let stopping = false
  const stop = (code) => {
    stopping = true
    hostProcess.kill()
    app?.stop()
    process.exitCode = code
  }
  hostProcess.on('exit', (code) => {
    if (stopping) return
    console.error(`The sandbox host stopped (exit ${code}). See ${log}`)
    stop(1)
  })
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, () => stop(130))
  for (let tries = 0; !(await answers(port)); tries++) {
    if (stopping) return
    if (tries === 60) {
      console.error(`The sandbox host didn't answer within 30 seconds. See ${log}`)
      return stop(1)
    }
    await delay(500)
  }

  const cdp = flags.cdp ? port + 10000 : null
  const command = `CODYNC_HOME="${home}" "${host}"`
  console.log(`\nCodync Dev (${name}): a sandbox on port ${port}, data in ${home}${flags.cloud ? '' : ', dev cloud off'}`)
  console.log(`  pair a phone or simulator  ${command} pair --port ${port}`)
  console.log(`  terminal UI                ${command} tui --port ${port}`)
  if (cdp) console.log(`  DevTools protocol          127.0.0.1:${cdp}`)
  console.log('')
  const appEnv = {
    ...childEnv(), CODYNC_HOME: home, CODYNC_PORT: String(port),
    ...(cdp ? { CODYNC_REMOTE_DEBUG: String(cdp) } : {}),
    CODYNC_DEV_ONBOARDING: flags.onboarding ? 'reset' : 'skip',
  }
  // The app trades the token only while it has no active session, so a signed-out or expired sandbox signs in again.
  const account = await mintTicket(name).catch((e) => console.error(`Test account skipped: ${e.message}`))
  if (stopping) return
  if (account) {
    appEnv.CODYNC_DEV_SIGN_IN_TICKET = account.token
    console.log(`  test account               ${account.email}`)
    console.log(`  sign a phone into it       npm run phone-account -- --ios <simulator udid> | --android <adb serial> --name ${name}\n`)
  }
  app = startApp(appEnv)
  app.child.on('exit', (code) => stop(code ?? 0))
  app.child.on('error', (e) => {
    console.error(`Couldn't start electron-vite: ${e.message}`)
    stop(1)
  })
}

if (flags.list || flags.prune) {
  await listAndPrune()
} else {
  if (flags.shared && (flags.sandbox || flags.name)) fail('--shared runs the default Codync Dev: leave out --sandbox and --name.')
  run(process.execPath, [join(desktop, 'tools/account-config.mjs'), 'dev'])
  // A fresh `npm ci` (a new worktree) can leave Electron's binary undownloaded.
  if (!existsSync(join(desktop, 'node_modules/electron/path.txt'))) run(process.execPath, [join(desktop, 'node_modules/electron/install.js')])
  const mainCheckout = statSync(join(root, '.git')).isDirectory()
  if (flags.sandbox || flags.name || (!mainCheckout && !flags.shared)) {
    await sandbox()
  } else {
    const { child, stop } = startApp()
    for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, stop)
    child.on('error', (e) => fail(`Couldn't start electron-vite: ${e.message}`))
    child.on('exit', (code) => {
      stop()
      process.exitCode = code ?? 1
    })
  }
}
