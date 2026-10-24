import type { VersionMismatch } from '@shared/compat'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useApp, useStore } from '../store/context'
import { UpdateButton, useUpdates } from './UpdateButton'

/** The words every client uses for a version mismatch (iOS, the TUI and this app). */
export const updateNeededText = {
  title(m: VersionMismatch, host: string) {
    return m.kind === 'updateApp' ? 'Update this app' : `Update Codync on ${host}`
  },
  detail(m: VersionMismatch, host: string, hostVersion: string | null | undefined, os: string | null | undefined, instructions = true) {
    if (m.kind === 'updateApp') {
      const runs = hostVersion ? `${host} runs Codync ${hostVersion} and` : host
      return `${runs} needs this app to be ${m.minimum} or newer. Your chats are safe on the computer.`
    }
    const how =
      os === 'macos' ? 'Open Codync on that Mac and choose Check for Updates.'
      : os === 'linux' ? 'Run codync-host update there.'
      : 'Update Codync on that computer.'
    const needs = `${host} runs Codync ${m.version}; this app needs ${m.minimum} or newer.`
    return instructions ? `${needs} ${how}` : needs
  },
}

/**
 * Says which side has to update before this app and a computer can work together, and offers
 * the update where this app can start it. Shown instead of the composer and above the bots.
 */
export function UpdateNeededCard() {
  const app = useApp()
  const store = useStore()
  const updates = useUpdates()
  const mismatch = store.mismatch
  if (!mismatch) return null
  // This app updates itself (its download and restart, as in Settings); on macOS and Windows this
  // computer's own host is put back on the app's bundled copy. Linux has no bundled host, so the
  // card says how to update it instead.
  const updatesApp = mismatch.kind === 'updateApp'
  const restartsHost = !updatesApp && store === app.local && !app.host.dev && window.codync.platform !== 'linux'
  const button = updatesApp
    ? updates && <UpdateButton updates={updates} kind="primary" />
    : restartsHost && <Button onClick={() => window.codync.host.restart()}>{`Update ${store.hostName}`}</Button>
  return (
    <HStack align="flex-start" gap={12} className="update-needed">
      <Icon name="arrow.down.circle" size={15} color="var(--warning)" />
      <VStack gap={6} grow>
        <Text as="div" size="subheadline" weight="semibold" tone="text">{updateNeededText.title(mismatch, store.hostName)}</Text>
        <Text as="div" size="footnote" tone="secondary">
          {updateNeededText.detail(mismatch, store.hostName, store.hostVersion?.version, store.hello?.os, !updatesApp && !restartsHost)}
        </Text>
        {button ? <div className="update-needed-action">{button}</div> : null}
      </VStack>
    </HStack>
  )
}
