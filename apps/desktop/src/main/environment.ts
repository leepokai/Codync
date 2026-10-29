import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import { sandboxLabel } from '../shared/dev-sandbox'
import { environmentProfile } from '../shared/environment'

interface BuildConfig {
  environment: 'main' | 'dev'
  clerkPublishableKey: string | null
  cloudURL: string | null
}

const path = app.isPackaged
  ? join(process.resourcesPath, 'account-config.json')
  : join(__dirname, '../../resources/account-config.json')
export const buildConfig = JSON.parse(readFileSync(path, 'utf8')) as BuildConfig
if (buildConfig.environment !== 'main' && buildConfig.environment !== 'dev') throw new Error('The app build environment is missing.')
export const identity = environmentProfile(buildConfig.environment)

// Before the single-instance lock, preferences, account restore or Chromium session is created.
// A development run against its own host data (`CODYNC_HOME`, a worktree's sandbox) keeps all of
// that there too, under its own name, so it runs beside the main checkout's Codync Dev.
if (identity.environment === 'dev') {
  const sandbox = app.isPackaged ? undefined : process.env.CODYNC_HOME
  app.setName(sandbox ? sandboxLabel(identity.name, sandbox) : identity.name)
  app.setPath('userData', sandbox ? join(sandbox, 'desktop') : join(app.getPath('appData'), identity.name))
}
