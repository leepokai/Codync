import type { VersionMismatch } from '@shared/compat'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import type { HostUpdate } from '../lib/host-update'
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
  /** A computer updating at this app's request. */
  updating(host: string) {
    return `Updating Codync on ${host}. This takes a few minutes.`
  },
  /** Why the last update asked of the computer didn't take. */
  notUpdated(last: HostUpdate, host: string) {
    return last.kind === 'failed' ? last.message : `${host} didn't finish updating. Try again, or update Codync on that computer.`
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
  const host = store.hostName
  const offer = mismatch.kind === 'updateHost' ? store.hostUpdateOffer : null
  // This app updates itself (its download and restart, as in Settings); on macOS and Windows this
  // computer's own host is put back on the app's bundled copy. Any other computer that can update
  // itself does when asked from here; otherwise the card says how to update it there.
  const action =
    mismatch.kind === 'updateApp' ? 'updateApp'
    : store === app.local && !app.host.dev && window.codync.platform !== 'linux' ? 'restartHost'
    : offer?.kind === 'available' ? 'askHost'
    : null
  const button =
    action === 'updateApp' ? updates && <UpdateButton updates={updates} kind="primary" />
    : action === 'restartHost' ? <Button onClick={() => window.codync.host.restart()}>Update now</Button>
    : action === 'askHost' ? <Button onClick={() => store.requestHostUpdate()}>Update now</Button>
    : null
  // With a button here, instructions for that computer would only confuse.
  const needs = updateNeededText.detail(mismatch, host, store.hostVersion?.version, store.hello?.os, action === null)
  const detail =
    offer?.kind === 'updating' ? updateNeededText.updating(host)
    : offer?.kind === 'available' && offer.last ? `${needs} ${updateNeededText.notUpdated(offer.last, host)}`
    : needs
  return (
    <HStack align="flex-start" gap={12} className="update-needed">
      <Icon name="arrow.down.circle" size={15} color="var(--warning)" />
      <VStack gap={6} grow>
        <Text as="div" size="subheadline" weight="semibold" tone="text">{updateNeededText.title(mismatch, host)}</Text>
        <Text as="div" size="footnote" tone="secondary">{detail}</Text>
        {button ? <div className="update-needed-action">{button}</div> : null}
      </VStack>
    </HStack>
  )
}
