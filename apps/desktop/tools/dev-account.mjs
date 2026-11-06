// A Clerk test account for `npm run dev` sandboxes: finds or creates `sandbox-<name>+clerk_test@example.com`
// in the Clerk *development* instance and mints a one-time sign-in token the app trades for a session
// (account.ts, `CODYNC_DEV_SIGN_IN_TICKET`). The secret key comes from CODYNC_CLERK_SECRET_KEY or the git-ignored
// `.env.development` in the main checkout's root (every worktree reads that one file); only `sk_test_` keys are used,
// so production is never touched.
import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { devClerkSecretKey } from '../src/shared/dev-sandbox.ts'

const API = 'https://api.clerk.com/v1'

export function secretKey() {
  // A linked worktree's git common dir is the main checkout's `.git`, so its parent holds the shared `.env.development`.
  const here = dirname(fileURLToPath(import.meta.url))
  const git = spawnSync('git', ['rev-parse', '--git-common-dir'], { cwd: here, encoding: 'utf8' })
  const file = git.status === 0 ? resolve(here, git.stdout.trim(), '..', '.env.development') : null
  return devClerkSecretKey(process.env.CODYNC_CLERK_SECRET_KEY, file && existsSync(file) ? readFileSync(file, 'utf8') : undefined)
}

async function clerk(key, method, path, body) {
  const res = await fetch(`${API}${path}`, {
    method,
    headers: { Authorization: `Bearer ${key}`, 'Content-Type': 'application/json' },
    body: body && JSON.stringify(body),
    signal: AbortSignal.timeout(20_000),
  })
  const json = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(json.errors?.[0]?.long_message ?? `Clerk ${method} ${path}: ${res.status}`)
  return json
}

const emailOf = (sandbox) => `sandbox-${sandbox}+clerk_test@example.com`
const findUser = (key, email) => clerk(key, 'GET', `/users?email_address=${encodeURIComponent(email)}`).then((found) => found[0])

/** A fresh sign-in token for the sandbox's test account, or null without a secret key. */
export async function mintTicket(sandbox) {
  const key = secretKey()
  if (!key) return null
  const email = emailOf(sandbox)
  const user = (await findUser(key, email)) ?? (await clerk(key, 'POST', '/users', { email_address: [email], skip_password_requirement: true }))
  const { token } = await clerk(key, 'POST', '/sign_in_tokens', { user_id: user.id, expires_in_seconds: 600 })
  return { token, email }
}

/** Deletes the sandbox's test account; true when there was one. Without a secret key there is nothing to delete. */
export async function deleteTestAccount(sandbox) {
  const key = secretKey()
  const user = key && (await findUser(key, emailOf(sandbox)))
  if (!user) return false
  await clerk(key, 'DELETE', `/users/${user.id}`)
  return true
}
