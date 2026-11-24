import type { ReactNode } from 'react'
import { approvals, withApproval } from '@shared/approvals'
import { withBackend } from '@shared/fallbacks'
import { draftOf, type Bot } from '@shared/models'
import { DropdownMenu } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import type { MenuItem } from '../../components/Menu'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { useAgentModels, type AgentModelsState } from '../bots/agent-models'
import { normalized } from '../bots/drafts'
import { ModelPicker } from '../bots/ModelPicker'
import { readySeats } from '../bots/seats'

export interface Approval { permission: string; mode?: string | null }

/** A way of approving, as an icon: asking, approving everything, or the agent's own mode read from its name. */
const approvalIcon = ({ id, label }: { id: string; label: string }) =>
  id === 'ask' ? 'hand.raised'
    : id === 'approve' ? 'checkmark.shield'
      : /plan/i.test(label) ? 'list.bullet.clipboard'
        : /read/i.test(label) ? 'eye'
          : /edit/i.test(label) ? 'pencil'
            : /bypass|full|danger|yolo/i.test(label) ? 'exclamationmark.shield'
              : 'shield'

/** How it is approved, as a menu: every way the agent offers, each with its icon, and the one in use. */
export function useApprovalMenu(approval: Approval, models: AgentModelsState, set: (approval: Approval) => void) {
  const tr = useTr()
  const { choices, selected } = approvals(approval, models.catalog)
  const items = (): MenuItem[] => choices.map((c) => ({
    title: c.label, icon: approvalIcon(c), detail: c.note ?? undefined, selected: c.id === selected.id, action: () => set(withApproval(approval, c.id)),
  }))
  return { items, label: tr(selected.label), icon: approvalIcon(selected) }
}

/** One thing a conversation runs on, as a quiet chip that opens its choices. */
export function Option({ title, items, leading, label, disabled = false, strong = false, search }: { title: string; items: () => MenuItem[]; leading: ReactNode; label: string; disabled?: boolean; strong?: boolean; search?: string }) {
  const tr = useTr()
  return (
    <DropdownMenu items={items} title={tr(title)} className="thread-option" disabled={disabled} search={search}>
      {leading}
      <Text size="caption" tone={strong ? 'text' : 'secondary'} truncate>{label}</Text>
      {disabled ? null : <Icon name="chevron.down" size={8} color="var(--tertiary)" />}
    </DropdownMenu>
  )
}

/** What a bot's chat runs on, in its message box: its model, with the installed agents' signed-in logins in the panel's
 * rail (`readySeats`; adding one stays in Settings), Fast mode where the agent has it, and how it is approved; each saved to the bot as its settings would. */
export function BotRunOptions({ bot }: { bot: Bot }) {
  const tr = useTr()
  const store = useStore()
  const models = useAgentModels(bot.backend, bot.loginProfile)
  const save = (change: Partial<ReturnType<typeof draftOf>>) => void store.save(normalized({ ...draftOf(bot), ...change })).catch((e: unknown) => store.setError(String(e)))
  const ways = useApprovalMenu({ permission: bot.permission, mode: bot.mode }, models, (a) => save({ permission: a.permission, mode: a.mode ?? null }))
  // Another login is the bot's changed, as in its settings.
  const seats = readySeats({ backends: store.hello?.backends ?? [], on: bot, tr, pick: (b, loginProfile) => save(withBackend(draftOf(bot), { backend: b.id, loginProfile })) })
  return (
    <ModelPicker
      backend={bot.backend}
      model={{ id: bot.model ?? null, options: bot.modelOptions }}
      models={models}
      set={(m) => save({ model: m.id, modelOptions: m.options })}
      fast={models.catalog?.fastMode || bot.fastMode ? { on: bot.fastMode, set: (on) => save({ fastMode: on }) } : null}
      approval={ways}
      seats={seats}
    />
  )
}
