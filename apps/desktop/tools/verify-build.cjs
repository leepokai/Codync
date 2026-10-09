// Fail before packaging if a stale account-config.json belongs to the other app variant.
const { accessSync, readFileSync } = require('node:fs')
const { join } = require('node:path')
module.exports = async (context) => {
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
