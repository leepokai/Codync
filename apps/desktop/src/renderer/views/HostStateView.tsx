import type { ReactElement } from 'react'
import { CharacterAvatar, type Mood } from '../components/Avatar'
import { Button } from '../components/Controls'
import { Icon } from '../components/Icon'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { palette, rgbHex, useDark } from '../lib/theme'
import { useApp } from '../store/context'

const isMac = window.codync.platform === 'darwin'

/** What stands between this computer and the chat, with the one action that fixes it. */
export function HostStateView({ compact = false }: { compact?: boolean }) {
  const app = useApp()
  const state = app.host.state
  switch (state.kind) {
    case 'notInstalled':
      return <EmptyState compact={compact} icon={<HostBot compact={compact} mood="idle" />} title={`Set up this ${isMac ? 'Mac' : 'computer'}`} message={`Codync runs your coding agents through the host, a small background service on this ${isMac ? 'Mac' : 'computer'}.`} action={['Install host', () => window.codync.host.install()]} />
    case 'missingBinary':
      return <EmptyState compact={compact} icon={<HostBot compact={compact} mood="failed" />} title="The host is missing" message={window.codync.appScheme === 'codync-dev' ? 'Rebuild Codync Dev with its development host, then open it again.' : "This copy of Codync doesn't include codync-host. Download Codync again from codync.dev or GitHub."} />
    case 'failed':
      return <FailedHost compact={compact} message={state.message} />
    default:
      return <EmptyState compact={compact} icon={<HostBot compact={compact} mood="working" />} title="Starting the host…" message="This takes a few seconds." />
  }
}

/** launchctl prints this in English, with the plist path in the host's own error line. */
function hostFailure(message: string, tr: (text: string) => string): string {
  const loaded = /launchctl bootstrap \S+ (\S+) failed/.exec(message)
  if (loaded || message.includes('Bootstrap failed:')) {
    const path = loaded?.[1]
    return path
      ? tr("The background service didn't start. macOS couldn't load {path}.").replaceAll('{path}', path)
      : tr("The background service didn't start.")
  }
  return tr(message)
}

function FailedHost({ compact, message }: { compact: boolean; message: string }) {
  const tr = useTr()
  return <EmptyState compact={compact} icon={<HostBot compact={compact} mood="failed" />} title="The host isn't running" message={hostFailure(message, tr)} action={['Try again', () => window.codync.host.restart()]} />
}

/** The app-icon bot expresses progress or failure without replacing the recovery instructions. */
function HostBot({ compact, mood }: { compact: boolean; mood: Mood }) {
  return (
    <div className={`host-state-bot is-${mood}`} aria-hidden>
      <CharacterAvatar shape="squircle" color="" tint={rgbHex(palette(useDark()).text)} size={compact ? 32 : 112} mood={mood} />
    </div>
  )
}

/** A quiet placeholder for an empty screen: icon (an SF Symbol name or artwork), title, a line of help and an optional action. */
export function EmptyState({ icon, title, message, action, compact = false }: { icon: string | ReactElement; title: string; message: string; action?: [string, () => void]; compact?: boolean }) {
  const tr = useTr()
  return (
    <div className={`empty-state ${compact ? 'local-recovery' : ''}`}>
      {typeof icon === 'string' ? <Icon name={icon} size={compact ? 22 : 36} weight="light" color="var(--tertiary)" /> : icon}
      <Text as="div" size={compact ? 'callout' : 'title3'} weight="semibold" tone="text">{tr(title)}</Text>
      <Text as="div" size="callout" tone="secondary" className="empty-state-message">{tr(message)}</Text>
      {action ? (
        <Button kind={compact ? 'secondary' : 'primary'} onClick={action[1]} className="empty-state-action">
          {action[0]}
        </Button>
      ) : null}
    </div>
  )
}

