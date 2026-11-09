import { Button, CardSection, ValueRow } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'
import { useModel } from '../../lib/observable'

/**
 * The first things a computer needs, with what is missing: the host answers, an agent is installed and signed in,
 * a bot exists, and computer access is granted. It disappears once every step is done.
 */
export function GettingStartedSection({ onExtensions, onAccess }: { onExtensions?: () => void; onAccess: () => void }) {
  const tr = useTr()
  const app = useApp()
  const local = useModel(app.local)
  const ready = local !== null
  const backends = local?.hello?.backends ?? []
  const agent = backends.some((backend) => backend.available || backend.installed === true)
  const signedIn = backends.some((backend) => backend.signedIn === true)
  const bots = app.local?.roster.length ?? 0
  const steps = [
    { id: 'host', title: tr('This computer’s host'), done: ready, action: null as null | (() => void), note: ready ? tr('Answering') : tr('Not running') },
    { id: 'agent', title: tr('A coding agent'), done: agent && signedIn, action: onExtensions, note: agent ? (signedIn ? tr('Installed and signed in') : tr('Installed; sign in')) : tr('None installed') },
    { id: 'bot', title: tr('A bot'), done: bots > 0, action: onExtensions, note: bots > 0 ? tr('{n} ready').replace('{n}', String(bots)) : tr('None yet') },
    { id: 'access', title: tr('Computer access'), done: false, action: onAccess, note: tr('Grant it to let bots use apps and the screen') },
  ]
  const pending = steps.filter((step) => !step.done && step.action !== null)
  if (pending.length === 0) return null
  return (
    <CardSection
      title={tr('Getting started')}
      footer={tr('Each step is set on this computer; the same pages stay in Settings and Extensions.')}
    >
      {steps.map((step) => (
        <ValueRow key={step.id} label={step.title}>
          <HStack gap={8}>
            <Text size="caption" tone={step.done ? 'secondary' : 'tertiary'}>{step.note}</Text>
            {!step.done && step.action ? (
              <Button kind="secondary" onClick={() => step.action?.()}>{tr('Open')}</Button>
            ) : null}
          </HStack>
        </ValueRow>
      ))}
    </CardSection>
  )
}
