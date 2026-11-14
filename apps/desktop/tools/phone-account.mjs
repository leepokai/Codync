// `npm run phone-account -- --ios <udid> | --android <serial> [--name <sandbox>]`: signs an iOS Simulator's or Android
// emulator's Codync Dev into this worktree sandbox's Clerk test account, the one `npm run dev` signs the desktop app
// into, so a phone and the sandbox share an account. The app trades the one-time token only while it is signed out
// (iOS: the CODYNC_DEV_SIGN_IN_TICKET launch variable; Android: a file only adb's run-as can write). The main checkout's
// Codync Dev keeps your own account: there this needs an explicit --name.
import { spawnSync } from 'node:child_process'
import { existsSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { mintTicket } from './dev-account.mjs'
import { SANDBOX_PREFIX, sandboxName } from '../src/shared/dev-sandbox.ts'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
const { values: flags } = parseArgs({ options: { ios: { type: 'string' }, android: { type: 'string' }, name: { type: 'string' } } })
const IOS_APP = 'com.pokai.Codync.ios.dev'
const ANDROID_APP = 'com.pokai.Codync.android.dev'

function fail(message) {
  console.error(message)
  process.exit(1)
}

function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, { encoding: 'utf8', ...options })
  if (result.status !== 0) fail(`${cmd} ${args.join(' ')} failed: ${(result.stderr || result.stdout || '').trim()}`)
}

if (!flags.ios === !flags.android) fail('Pick one device: --ios <simulator udid> or --android <adb serial>.')
if (statSync(join(root, '.git')).isDirectory() && !flags.name) {
  fail("The main checkout's Codync Dev keeps your own account. Run this from a worktree, or name a sandbox with --name <name>.")
}
const name = sandboxName(flags.name ?? basename(root))
// `npm run dev -- --prune` finds a test account through its sandbox folder, so an account is only made for one.
if (!existsSync(join(homedir(), SANDBOX_PREFIX + name, 'sandbox.json'))) fail(`No sandbox ${name}: start it first with npm run dev.`)
const account = await mintTicket(name)
if (!account) fail('No Clerk test key: set CODYNC_CLERK_SECRET_KEY in the main checkout\'s .env.development (see docs/guides/dev-app.md).')

if (flags.ios) {
  run('xcrun', ['simctl', 'launch', '--terminate-running-process', flags.ios, IOS_APP],
    { env: { ...process.env, SIMCTL_CHILD_CODYNC_DEV_SIGN_IN_TICKET: account.token } })
} else {
  const adb = (...args) => ['-s', flags.android, ...args]
  run('adb', adb('shell', 'run-as', ANDROID_APP, 'sh', '-c', "'mkdir -p files && cat > files/dev-sign-in-ticket'"), { input: account.token })
  run('adb', adb('shell', 'am', 'force-stop', ANDROID_APP))
  run('adb', adb('shell', 'am', 'start', '-n', `${ANDROID_APP}/com.codync.android.MainActivity`))
}
console.log(`Signing ${flags.ios ? 'the simulator' : 'the emulator'} into ${account.email} (sandbox ${name}).`)
