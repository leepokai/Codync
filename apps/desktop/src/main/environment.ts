import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
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
if (identity.environment === 'dev') {
  app.setName(identity.name)
  app.setPath('userData', join(app.getPath('appData'), identity.name))
}
