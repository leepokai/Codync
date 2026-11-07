import { isGroup, type Entry } from '@shared/models'
import { Button } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'

/** A bot asking the people of a group a short question; they answer by replying to it. */
export function MemberQuestionCard({ entry, reply }: { entry: Entry; reply?: () => void }) {
  const store = useStore()
  const tr = useTr()
  const question = entry.data.memberQuestion!
  const asked = (entry.data.author && store.bots.get(entry.data.author)?.name) || tr('A bot')
  const group = store.bots.get(entry.botId)
  return (
    <VStack gap={6} className="outlined-card member-question">
      <HStack gap={6}>
        <Icon name="person.2" size={12} />
        <Text size="caption" weight="semibold">{asked} · {tr('Needs a member')}</Text>
      </HStack>
      <Text>{entry.data.text}</Text>
      {question.answers.map((answer, index) => <Text key={index} size="callout" tone="secondary">“{answer}”</Text>)}
      {question.status === 'pending' && reply ? <Button kind="secondary" onClick={reply}>{tr('Answer')}</Button> : null}
      {group && isGroup(group) ? null : null}
    </VStack>
  )
}
