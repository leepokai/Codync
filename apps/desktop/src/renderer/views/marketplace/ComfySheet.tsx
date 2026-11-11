import { useEffect, useState } from 'react'
import { bytesText } from '@shared/health'
import { installLine, installing, type ToolJob } from '@shared/tools'
import { Button, CardForm, CardSection, Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { useTool } from '../../lib/use-tool'
import { useTr } from '../../lib/use-language'
import { ErrorText } from './pieces'

/** The job line: what ComfyUI is doing for a bot, with a thumbnail once it finishes. */
function JobRow({ job, image, onCancel }: { job: ToolJob; image: string | null; onCancel: () => void }) {
  const tr = useTr()
  const label = { queued: 'Waiting in the queue', running: 'Making an image…', done: 'Done', cancelled: 'Cancelled', error: 'Failed' }[job.phase]
  return (
    <VStack gap={6}>
      <HStack gap={8}>
        {job.phase === 'running' || job.phase === 'queued' ? <Spinner size={14} /> : null}
        <Text grow tone={job.phase === 'error' ? 'attention' : 'text'}>{tr(label)}</Text>
        {job.phase === 'running' || job.phase === 'queued' ? <Button kind="secondary" onClick={onCancel}>{tr('Cancel')}</Button> : null}
      </HStack>
      {job.error ? <Text size="caption" tone="attention">{job.error}</Text> : null}
      {job.phase === 'done' && image ? <img className="comfy-thumb" src={image} alt={tr('Last image')} /> : null}
    </VStack>
  )
}

/**
 * ComfyUI's card in Extensions › Apps: open the editor, stop it, see its log and models folder, the
 * space it uses and the version bundled, and the job a bot is running. The host owns it all.
 */
export function ComfySheet() {
  const tr = useTr()
  const { state, hostKnown, call, setState } = useTool('comfy')
  const [error, setError] = useState<string | null>(null)
  const [image, setImage] = useState<string | null>(null)
  const card = state?.comfy
  const job = state?.job ?? null
  const running = state?.state === 'running'
  const active = running || state?.state === 'starting'

  useEffect(() => {
    if (job?.phase !== 'done') return setImage(null)
    let alive = true
    void call<{ image: string | null }>('comfyJobImage').then((r) => alive && setImage(r.image)).catch(() => {})
    return () => {
      alive = false
    }
  }, [job?.id, job?.phase, call])

  const act = (run: () => Promise<unknown>) => {
    setError(null)
    void run().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }

  return (
    <SheetPage>
      <ModalHeader title="ComfyUI" subtitle={tr('Make images on this computer. Bots can use it when you turn it on for them.')} />
      <CardForm>
        <CardSection>
          <VStack gap={10}>
            {!hostKnown || state === null ? (
              <HStack gap={8}><Spinner size={18} /><Text tone="secondary">{tr('Checking {name}…').replace('{name}', 'ComfyUI')}</Text></HStack>
            ) : card && !card.bundled ? (
              <Text tone="secondary">{tr('This build of Codync does not include ComfyUI.')}</Text>
            ) : (
              <>
                <Text tone="text" weight="medium">
                  {running ? tr('Running') : state.state === 'starting' ? tr('Starting {name}… the first start can take a few minutes.').replace('{name}', 'ComfyUI') : state.state === 'error' ? tr('Stopped with an error') : tr('Stopped')}
                </Text>
                {card ? (
                  <Text size="caption" tone="secondary">
                    {tr('Version {version} · {size} used').replace('{version}', card.version).replace('{size}', bytesText(card.usedBytes))}
                    {running ? ` · ${tr('Port {port}').replace('{port}', String(state.port))}` : ''}
                  </Text>
                ) : null}
                {card?.notice ? <Text size="caption" tone="attention">{tr(card.notice)}</Text> : null}
                {state.error ? <Text size="caption" tone="attention">{state.error}</Text> : null}
                {installing(state.install) ? (
                  <HStack gap={8}><Spinner size={14} /><Text size="caption" tone="secondary">{installLine('ComfyUI', state.install, tr)}</Text></HStack>
                ) : null}
                {state.install?.phase === 'failed' && state.install.error ? <Text size="caption" tone="attention">{state.install.error}</Text> : null}
                {card && !card.installed && !installing(state.install) ? (
                  <Text size="caption" tone="secondary">
                    {tr('{name} downloads about {size} the first time it starts.').replace('{name}', 'ComfyUI').replace('{size}', bytesText(card.downloadBytes))}
                  </Text>
                ) : null}
                <HStack gap={8}>
                  {running ? (
                    <Button onClick={() => act(() => window.codync.tools.open('comfy'))}>{tr('Open')}</Button>
                  ) : (
                    <Button disabled={active || installing(state.install)} onClick={() => act(() => call('toolStart').then(setState))}>
                      {card && !card.installed ? tr('Download and start') : tr('Start')}
                    </Button>
                  )}
                  <Button kind="secondary" disabled={!state.enabled && !active} onClick={() => act(() => call('toolStop').then(setState))}>{tr('Stop')}</Button>
                  <Button kind="secondary" onClick={() => act(() => window.codync.tools.openLog('comfy'))}>{tr('Log')}</Button>
                  <Button kind="secondary" onClick={() => act(() => window.codync.tools.openModels('comfy'))}>{tr('Models')}</Button>
                </HStack>
                {job ? <JobRow job={job} image={image} onCancel={() => act(() => call('comfyCancel'))} /> : null}
              </>
            )}
            {error ? <ErrorText text={error} /> : null}
          </VStack>
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}
