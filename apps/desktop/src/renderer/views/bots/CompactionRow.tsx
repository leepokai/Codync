import type { Bot, BotDraft } from '@shared/models'
import { DropdownMenu, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { compactionLabel, compactionMenu } from '../../lib/compaction-options'
import { useTr } from '../../lib/use-language'

/** When Codync summarizes the chat: a preset or a size the window allows. Only for a bot whose chat it summarizes. */
export function CompactionRow({ draft, bot, onChange }: {
  draft: BotDraft
  bot: Bot | null
  onChange: (update: (d: BotDraft) => BotDraft) => void
}) {
  const tr = useTr()
  if (!bot || bot.compaction === null) return null
  const selection = draft.compaction ?? bot.compaction
  const { title, verbatim } = compactionLabel(selection)
  const label = verbatim ? title : tr(title)
  return (
    <ValueRow label="Summarize long chats" detail="When Codync summarizes this chat so it can go on. If the chat fills up first, the agent summarizes it itself.">
      <DropdownMenu
        title={label}
        className="choice-picker outlined"
        items={() => compactionMenu(selection, bot.compactionTokens, (compaction) => onChange((d) => ({ ...d, compaction })))}
      >
        <span className="choice-label">{label}</span>
        <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
      </DropdownMenu>
    </ValueRow>
  )
}
