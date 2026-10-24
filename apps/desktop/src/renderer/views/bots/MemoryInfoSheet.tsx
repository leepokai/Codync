import { CardForm, CardSection } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

interface Point { icon: string; title: string; detail: string }

/** How the bot reaches its harness's own memory: a switch with its notes shown here (Claude Code's are a file), a switch only, or one memory every bot on the harness shares, with no switch (Hermes Agent, Gemini CLI). */
type HarnessAccess = 'readable' | 'hidden' | 'shared'

/** What the harness's own memory means for the bot. The harness name is filled in after the sentence is translated. */
export function harnessInfo(harness: string, access: HarnessAccess, tr: (text: string) => string): Point[] {
  const fill = (template: string) => tr(template).replaceAll('{name}', harness)
  const shared = access === 'shared'
  return [
    { icon: 'brain', title: tr('A memory of its own'), detail: fill(shared
      ? "{name} keeps notes apart from Codync's memory, and Codync can't turn them off for one bot."
      : "{name} keeps notes apart from Codync's memory. It's off by default, so this bot has one memory and you see all of it here.") },
    ...(shared ? [{ icon: 'person.2', title: tr('Shared by every bot'), detail: fill('Every bot that runs {name} reads and writes the same notes, and so does {name} when you use it outside Codync.') }] : []),
    { icon: 'square.on.square', title: tr('Remembered twice'), detail: tr('With both on, the bot may save the same thing twice, and the two can disagree after something changes.') },
    { icon: 'trash', title: tr('Forgetting'), detail: fill("What the bot forgets in Codync stays in {name}'s notes. Codync can't change them.") },
    { icon: 'eye', title: tr('Where to see it'), detail: fill(access === 'readable' ? 'Its notes show read-only under the {name} tab.' : "Codync can't show it; look in {name} itself.") },
    ...(shared ? [{ icon: 'gearshape', title: tr('Turning it off'), detail: fill("Where {name} has a setting for it, that setting turns it off for every session at once.") }] : []),
  ]
}

/** What dreaming does, beside its read-only log. */
export const dreamsInfo: Point[] = [
  { icon: 'moon', title: 'Tidying between chats', detail: 'After five quiet minutes or eight messages, or once memory is nearly full, the bot reads what you said since its last run and updates its memory.' },
  { icon: 'plus.circle', title: 'Adds and takes out', detail: 'Green lines are memories it added. Red lines are ones it took out because they went out of date or moved into a note. It can also add a line to a note.' },
  { icon: 'text.bubble', title: 'Only your words', detail: "What it adds quotes something you typed yourself; pasted text, the bot's replies and other bots don't count. It never rewrites what you wrote. It may also put a passed date in the past tense, and before memory fills up it moves detail, word for word, into notes by topic, keeping one line per topic that points to its note." },
  { icon: 'lock.shield', title: 'Kept apart', detail: 'It runs in its own session, away from the chat, and fixed checks decide what it may save.' },
  { icon: 'eye', title: 'A record', detail: 'This tab only shows what it did. To change a memory, tell the bot.' },
]

/** The memory screen's info buttons: a title and what to know, one point per row. The same text as the kit's; the terminal UI says it short in place. */
export function MemoryInfoSheet({ icon, title, points }: { icon: string; title: string; points: Point[] }) {
  const tr = useTr()
  return (
    <SheetPage>
      <ModalHeader icon={icon} title={tr(title)} />
      <CardForm>
        <CardSection>
          {points.map((d) => (
            <HStack key={d.title} align="flex-start" gap={12}>
              <IconSlot width={20} tone="secondary" className="memory-info-icon">
                <Icon name={d.icon} size={15} />
              </IconSlot>
              <VStack gap={2} grow>
                <Text>{tr(d.title)}</Text>
                <Text size="footnote" tone="secondary">{tr(d.detail)}</Text>
              </VStack>
            </HStack>
          ))}
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}
