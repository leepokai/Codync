import { useEffect, useState } from 'react'
import { CardSection, ValueRow } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { fill } from '@shared/fill'

/** Where an agent comes from on the computer (`agentInstall`): its CLI, and the ACP registry build Codync runs it through. */
export interface AgentInstall {
  cli: { path: string; version: string | null; addedBy: 'codync' | 'computer' } | null
  build: { id: string; version: string | null; launch: 'download' | 'npx' | 'uvx' | null; downloaded: boolean | null } | null
  /** What Codync starts: the CLI's own ACP mode, the registry build, or nothing yet. */
  runs: 'cli' | 'build' | null
}

/**
 * Under an agent's install step: the version of its CLI on the computer and whether it is the user's own or Codync
 * added it, and the ACP registry build Codync runs (its version, downloaded or not yet). Older hosts answer nothing,
 * and it shows nothing.
 */
export function AgentSource({ backendId, installed }: { backendId: string; installed: boolean }) {
  const store = useStore()
  const tr = useTr()
  const [info, setInfo] = useState<AgentInstall | null>(null)
  const online = store.connection.kind === 'online'

  // Again after an install, which changes what is on the computer.
  useEffect(() => {
    const client = store.client
    if (!online || !client) return
    let cancel = false
    client.call<AgentInstall>('agentInstall', { backend: backendId }, 30_000)
      .then((loaded) => { if (!cancel) setInfo(loaded) })
      .catch(() => { if (!cancel) setInfo(null) })
    return () => { cancel = true }
  }, [store, backendId, online, installed])

  if (!info || (!info.cli && !info.build)) return null
  const host = store.hostName
  const { cli, build } = info
  const footer = info.runs === 'cli' ? tr("Codync runs the CLI's own ACP mode.")
    : info.runs === 'build' && cli ? tr('Codync runs the ACP build, which uses this CLI.')
    : null
  return (
    <CardSection title="Where it comes from" footer={footer}>
      {cli ? (
        <ValueRow label={tr('CLI')} detail={cli.path}>
          <Text tone="secondary">{[cli.version ?? tr('Version unknown'), cli.addedBy === 'codync' ? tr('Added by Codync') : fill(tr('Your own, on {name}'), { name: host })].join(' · ')}</Text>
        </ValueRow>
      ) : null}
      {build ? (
        <ValueRow
          label={tr('ACP build')}
          detail={build.downloaded ? tr('Downloaded by Codync from the ACP registry') : tr('From the ACP registry. Codync downloads it the first time it runs.')}
        >
          <Text tone="secondary">{build.version ? `${build.id} ${build.version}` : build.id}</Text>
        </ValueRow>
      ) : null}
    </CardSection>
  )
}
