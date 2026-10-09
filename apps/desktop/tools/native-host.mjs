// Builds the host the app bundles for the same environment as the app: node tools/native-host.mjs
// dev|main compiles it in as CODYNC_ENV. Built on every package so a host made for another
// environment is never shipped. macOS: build/native/codync-host (universal). Windows:
// build/native/codync-host.exe and codync-hostw.exe (runs it at sign-in), plus the computer-use
// driver the host starts from beside itself. Linux apps use the installed host, so there's
// nothing to do there.
import { copyFileSync, mkdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

if (!['darwin', 'win32'].includes(process.platform)) process.exit(0)
const env = process.argv[2]
if (!['dev', 'main'].includes(env)) throw new Error(`unknown environment ${env}`)
const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const host = join(root, '../../host')
const out = join(root, 'build/native')
// The pinned toolchain (rust-toolchain.toml) through rustup, even when another cargo (Homebrew's)
// comes first on PATH: only rustup's has the x86_64 target.
const tool = (name) => execFileSync('rustup', ['which', name], { cwd: host, encoding: 'utf8' }).trim()
const [cargo, rustc] = [tool('cargo'), tool('rustc')]
const run = (cmd, args) => execFileSync(cmd, args, { stdio: 'inherit', env: { ...process.env, CODYNC_ENV: env, RUSTC: rustc } })
const build = (target) => run(cargo, ['build', '--release', '--manifest-path', join(host, 'Cargo.toml'), '--target', target])
mkdirSync(out, { recursive: true })
if (process.platform === 'win32') {
  const target = 'x86_64-pc-windows-msvc'
  build(target)
  for (const exe of ['codync-host.exe', 'codync-hostw.exe']) copyFileSync(join(host, 'target', target, 'release', exe), join(out, env === 'dev' && exe === 'codync-hostw.exe' ? 'codync-dev-hostw.exe' : exe))
  run(process.execPath, [join(root, '../../packaging/cua-driver/fetch.mjs'), 'windows-x86_64', out])
} else {
  const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin']
  for (const target of targets) build(target)
  run('lipo', ['-create', '-output', join(out, 'codync-host'), ...targets.map((t) => join(host, 'target', t, 'release/codync-host'))])
}
console.log(`host: ${env}`)
