// Fail before packaging if a stale account-config.json belongs to the other app variant, or if
// the bundled Windows host was built for another architecture than the installer.
const { accessSync, readFileSync } = require('node:fs')
const { join } = require('node:path')
const { Arch } = require('electron-builder')

const PE_MACHINES = { 0x8664: 'x64', 0xaa64: 'arm64' }

/** The architecture a Windows executable is built for, from its PE header. */
function peArch(file) {
  const bytes = readFileSync(file)
  const header = bytes.readUInt32LE(0x3c)
  return PE_MACHINES[bytes.readUInt16LE(header + 4)] ?? 'unknown'
}

module.exports = async (context) => {
  if (context.electronPlatformName === 'win32') {
    // tools/native-host.mjs builds the host for the machine it runs on, which a cross-arch package wouldn't match.
    const host = peArch(join(context.packager.projectDir, 'build/native/codync-host.exe'))
    const target = Arch[context.arch]
    if (host !== target) throw new Error(`Packaging the Windows ${target} app with a host built for ${host}; package it on Windows ${target}.`)
  }
  const expected = context.packager.appInfo.id === 'com.pokai.Codync.dev' ? 'dev' : 'main'
  const config = JSON.parse(readFileSync(join(context.packager.projectDir, 'resources/account-config.json'), 'utf8'))
  if (config.environment !== expected) throw new Error(`Packaging ${expected} with ${config.environment} account config; run the matching dist script.`)
  if (context.electronPlatformName === 'darwin') {
    const symbols = join(context.packager.projectDir, 'out/renderer/symbols')
    const manifest = JSON.parse(readFileSync(join(symbols, 'manifest.json'), 'utf8'))
    if (!manifest['apple.logo']) throw new Error('The macOS build is missing the Apple logo; rebuild the app on macOS.')
    for (const weight of ['light', 'regular', 'medium', 'semibold', 'bold', 'black']) {
      accessSync(join(symbols, `apple.logo.${weight}.png`))
    }
  }
}

module.exports.peArch = peArch
