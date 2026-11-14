import { approvals, withApproval, type ModeCatalog } from '@shared/approvals'
import type { BotDraft } from '@shared/models'
import { ChoicePicker, ValueRow } from '../../components/Controls'
import { useTr } from '../../lib/use-language'

/** Permissions: the modes the bot's agent offers (its model list), its Auto mode first, then approving everything. */
export function ApprovalRow({ draft, catalog, onChange }: {
  draft: BotDraft
  catalog: ModeCatalog | null
  onChange: (update: (d: BotDraft) => BotDraft) => void
}) {
  const tr = useTr()
  const { choices, selected } = approvals(draft, catalog)
  const detail = [selected.note, tr(selected.detail)].filter(Boolean).join(' ')
  return (
    <ValueRow label="Permissions" detail={detail}>
      <ChoicePicker
        selection={selected.id}
        options={choices.map((c) => ({ id: c.id, label: c.label }))}
        onChange={(id) => onChange((d) => withApproval(d, id))}
      />
    </ValueRow>
  )
}
