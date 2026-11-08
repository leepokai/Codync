import { useState } from 'react'
import type { Bot, BotDraft, CompactionSizes } from '@shared/models'
import { DropdownMenu, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, DialogField } from '../../components/Overlay'
import { compactionLabel, compactionMenu, fitSize, parseTypedSize } from '../../lib/compaction-options'
import { useTr } from '../../lib/use-language'

/** When Codync summarizes the chat: a preset or a size the window allows. For a bot whose chat it summarizes. */
export function CompactionRow({ draft, bot, onChange }: {
  draft: BotDraft
  bot: Bot
  onChange: (update: (d: BotDraft) => BotDraft) => void
}) {
  const tr = useTr()
  const [typing, setTyping] = useState<string | null>(null)
  const selection = draft.compaction ?? bot.compaction ?? 'off'
  const { title, verbatim } = compactionLabel(selection)
  const label = verbatim ? title : tr(title)
  const pick = (compaction: BotDraft['compaction']) => onChange((d) => ({ ...d, compaction }))
  return (
    <ValueRow label="Summarize long chats" detail="Auto leaves it to the agent, which summarizes once the chat fills up. The others have Codync do it sooner.">
      <DropdownMenu
        title={label}
        className="choice-picker outlined"
        items={() => compactionMenu(selection, bot.compactionTokens, pick, () => setTyping(typeof selection === 'number' ? String(selection / 1000) : ''))}
      >
        <span className="choice-label">{label}</span>
        <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
      </DropdownMenu>
      {bot.compactionTokens ? (
        <CustomSizeDialog text={typing} sizes={bot.compactionTokens} onText={setTyping} onClose={() => setTyping(null)} onSave={pick} />
      ) : null}
    </ValueRow>
  )
}

/** "Custom size": a number of thousands of tokens, held to what the window takes. Open while `text` is set. */
function CustomSizeDialog({ text, sizes, onText, onClose, onSave }: {
  text: string | null
  sizes: CompactionSizes
  onText: (text: string) => void
  onClose: () => void
  onSave: (tokens: number) => void
}) {
  const tr = useTr()
  const typed = parseTypedSize(text ?? '')
  const save = () => {
    if (typed !== null) onSave(fitSize(sizes, typed))
  }
  return (
    <Dialog open={text !== null} title={tr('Custom size')} message={tr('Thousands of tokens, rounded to 10K, up to what the model holds.')}
      actions={[{ title: 'Save', disabled: typed === null, action: save }]} onClose={onClose}>
      <DialogField value={text ?? ''} placeholder={`${sizes.min / 1000}–${sizes.max / 1000}`} label={tr('Custom size')} inputMode="decimal"
        onChange={onText}
        onSubmit={() => {
          if (typed === null) return
          onClose()
          save()
        }} />
    </Dialog>
  )
}
