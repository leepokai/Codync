import { useState } from 'react'
import { Button } from '../components/Controls'
import { ModalHeader, Sheet, SheetPage } from '../components/Overlay'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { useModel } from '../lib/observable'
import { useApp } from '../store/context'

type StepId = 'host' | 'agent' | 'bot' | 'access'

const ORDER: StepId[] = ['host', 'agent', 'bot', 'access']

/**
 * A first run, one step at a time: the host, an agent, a bot and computer access. It resumes where it stopped,
 * can skip a step, and stops asking once finished or dismissed.
 */
export function OnboardingSheet({ open, onClose, onExtensions, onAccess }: {
  open: boolean
  onClose: () => void
  onExtensions: () => void
  onAccess: () => void
}) {
  const tr = useTr()
  const app = useApp()
  const local = useModel(app.local)
  const [index, setIndex] = useState(0)
  const [skipped, setSkipped] = useState<StepId[]>([])
  if (!open) return null
  const backends = local?.hello?.backends ?? []
  const done: Record<StepId, boolean> = {
    host: local !== null,
    agent: backends.some((backend) => backend.signedIn === true),
    bot: (app.local?.roster.length ?? 0) > 0,
    access: false,
  }
  const step = ORDER[index]
  const finished = ORDER.every((id) => done[id] || skipped.includes(id))
  const titles: Record<StepId, string> = {
    host: tr('This computer’s host'),
    agent: tr('A coding agent'),
    bot: tr('A bot'),
    access: tr('Computer access'),
  }
  const bodies: Record<StepId, string> = {
    host: tr('The host runs the bots on this computer. If it is not answering, restart Codync; it starts with the app.'),
    agent: tr('Install a coding agent and sign in, so the bots have a model to think with. Claude Code, Codex and others are in Extensions.'),
    bot: tr('A bot is one conversation with your work: its own agent, folder, permissions and memory. Create the first one now.'),
    access: tr('Grant computer access only if you want bots to open apps and use the screen. You can skip it and grant it later in Settings.'),
  }
  const action = () => {
    if (step === 'agent' || step === 'bot') return onExtensions
    if (step === 'access') return onAccess
    return null
  }
  const next = () => {
    if (index + 1 >= ORDER.length) return onClose()
    setIndex(index + 1)
  }
  const skip = () => {
    setSkipped((list) => [...list, step])
    next()
  }
  return (
    <Sheet open onClose={onClose} width={520} maxHeight={520}>
      <SheetPage>
        <ModalHeader title={tr('Getting started')} subtitle={tr('Step {n} of {total}').replace('{n}', String(index + 1)).replace('{total}', String(ORDER.length))} onClose={onClose} closeTitle={tr('Close')} />
        <VStack gap={14} className="channel-sheet">
          <VStack gap={6}>
            <Text weight="semibold">{titles[step]}</Text>
            <Text size="caption" tone="secondary">{bodies[step]}</Text>
            <Text size="caption" tone={done[step] ? 'secondary' : 'tertiary'}>
              {done[step] ? tr('Done on this computer') : tr('Not set up yet')}
            </Text>
          </VStack>
          <HStack gap={8} wrap>
            {!done[step] && action() ? <Button onClick={action()!}>{tr('Open')}</Button> : null}
            <Button kind="secondary" onClick={next}>{tr(done[step] || skipped.includes(step) ? 'Next' : 'Skip for now')}</Button>
            {!done[step] ? <Button kind="secondary" onClick={skip}>{tr('Skip this step')}</Button> : null}
            <Button kind="secondary" onClick={onClose}>{tr('Not now')}</Button>
          </HStack>
          {finished ? <Text size="caption" tone="secondary">{tr('You can reopen this in Settings → Getting started.')}</Text> : null}
        </VStack>
      </SheetPage>
    </Sheet>
  )
}
