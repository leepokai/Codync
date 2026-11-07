import { Button, CardSection, ChoicePicker } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { outputKinds, outputProblem, type RoutineOutput } from './routine'
import './routines.css'

const placeholder = (kind: string) =>
  kind === 'githubComment' ? 'owner/name' : kind === 'telegram' ? '-1001234567890' : kind === 'discordReact' ? '👍' : '1234567890'

const valuePlaceholder = (kind: string) =>
  kind === 'githubIssue' ? 'Issue title (optional)' : kind === 'githubLabel' ? 'Labels, separated by commas' : 'People, separated by commas'

const valueLabel = (kind: string) => (kind === 'githubIssue' ? 'Issue title' : kind === 'githubLabel' ? 'Labels' : 'People')

/** Where a workflow's last reply goes: a channel, a chat, or the issue that started it. */
export function RoutineOutputsFields({ outputs, update }: { outputs: RoutineOutput[]; update: (outputs: RoutineOutput[]) => void }) {
  const tr = useTr()
  const change = (index: number, patch: Partial<RoutineOutput>) =>
    update(outputs.map((output, i) => (i === index ? { ...output, ...patch } : output)))
  return (
    <CardSection footer={tr('Sent once, after the last step, with its reply. Empty is fine.')}>
      {outputs.map((output, index) => (
        <VStack key={index} gap={4} className="routine-output">
          <HStack gap={8}>
            <ChoicePicker
              selection={output.kind}
              options={outputKinds.map((k) => ({ id: k.id, label: k.label }))}
              onChange={(kind) => change(index, { kind })}
              fits
            />
            <Button kind="secondary" onClick={() => update(outputs.filter((_, i) => i !== index))}>{tr('Remove')}</Button>
          </HStack>
          <input
            className="schedule-input"
            aria-label={tr('Destination')}
            value={output.target}
            placeholder={placeholder(output.kind)}
            spellCheck={false}
            onChange={(event) => change(index, { target: event.target.value })}
          />
          {output.kind === 'telegram' ? (
            <input
              className="schedule-input"
              aria-label={tr('Telegram bot')}
              value={output.account ?? ''}
              placeholder={tr('The bot it sends as, if there are several')}
              spellCheck={false}
              onChange={(event) => change(index, { account: event.target.value })}
            />
          ) : null}
          {['githubLabel', 'githubAssign', 'githubReview', 'githubIssue'].includes(output.kind) ? (
            <input
              className="schedule-input"
              aria-label={tr(valueLabel(output.kind))}
              value={output.value ?? ''}
              placeholder={tr(valuePlaceholder(output.kind))}
              spellCheck={false}
              onChange={(event) => change(index, { value: event.target.value })}
            />
          ) : null}
          {outputProblem(output) ? <Text size="caption" tone="danger">{tr(outputProblem(output)!)}</Text> : null}
        </VStack>
      ))}
      {outputs.length < 10 ? (
        <HStack>
          <Button kind="secondary" onClick={() => update([...outputs, { kind: 'slack', target: '' }])}>{tr('Add an output')}</Button>
        </HStack>
      ) : null}
    </CardSection>
  )
}
