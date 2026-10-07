// Puts the pinned computer-use driver (cua-driver, MIT, https://github.com/trycua/cua) for one
// target into a directory: node packaging/cua-driver/fetch.mjs <target> <dir> [<license dir>],
// where <target> is a key of driver.json. The archive is checked against the pinned SHA-256 and
// cached under build/; only the listed executables are copied, and the license goes beside them
// unless another directory is given (a Mac bundle's code folders hold only code). Updating the
// driver: change driver.json (version, tag, assets and their sums from the release's SHA256SUMS).
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const pin = JSON.parse(readFileSync(join(here, 'driver.json'), 'utf8'))
const [target, out, licenseDir = out] = process.argv.slice(2)
const spec = pin.targets[target]
if (!spec || !out) throw new Error(`usage: fetch.mjs <${Object.keys(pin.targets).join('|')}> <dir> [<license dir>]`)

const sha256 = (data) => createHash('sha256').update(data).digest('hex')
const cache = join(here, '../../build/cua-driver')
const archive = join(cache, spec.asset)
if (!existsSync(archive) || sha256(readFileSync(archive)) !== spec.sha256) {
  const url = `https://github.com/trycua/cua/releases/download/${pin.tag}/${spec.asset}`
  const res = await fetch(url)
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`)
  const data = Buffer.from(await res.arrayBuffer())
  if (sha256(data) !== spec.sha256) throw new Error(`${spec.asset} doesn't match its pinned SHA-256`)
  mkdirSync(cache, { recursive: true })
  writeFileSync(archive, data)
}

const unpacked = mkdtempSync(join(tmpdir(), 'cua-driver-'))
try {
  // Windows' own tar (bsdtar) reads zip; a Git Bash tar earlier on PATH doesn't.
  const tar = process.platform === 'win32' ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar'
  execFileSync(tar, ['-xf', archive, '-C', unpacked], { stdio: 'inherit' })
  mkdirSync(out, { recursive: true })
  for (const file of spec.files) {
    copyFileSync(join(unpacked, file), join(out, file))
    chmodSync(join(out, file), 0o755)
  }
  mkdirSync(licenseDir, { recursive: true })
  copyFileSync(join(here, 'LICENSE'), join(licenseDir, 'cua-driver.LICENSE'))
} finally {
  rmSync(unpacked, { recursive: true, force: true })
}
console.log(`cua-driver ${pin.version} (${target}) → ${out}`)
