import { useState } from 'react'
import { isGroup, isWorkingIn } from '@shared/models'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { currentChecklist } from './task-checklist'
import './task-checklist.css'

/**
 * Above the message box: the checklist the bot is working through, with how far it is. A click folds it to the
 * step under way. It leaves once every step is done and the bot is idle; the full conversation keeps it.
 */
export function TaskChecklist({ botId }: { botId: string }) {
  const store = useStore()
  const tr = useTr()
  const bot = store.bots.get(botId) ?? null
  const [folded, setFolded] = useState(false)
  if (!bot || isGroup(bot)) return null
  const list = currentChecklist(store.allEntries(botId), isWorkingIn(bot, botId))
  if (!list) return null
  const active = list.items.find((i) => i.status === 'in_progress') ?? list.items.find((i) => i.status !== 'completed')
  return (
    <div className="task-checklist">
      <button className="task-checklist-head" aria-expanded={!folded} onClick={() => setFolded((f) => !f)}>
        <Icon name="checklist" size={12} weight="bold" />
        <span className="task-checklist-title">{tr('Tasks')}</span>
        <span className="task-checklist-count">{list.done}/{list.items.length}</span>
        {folded && active ? <span className="task-checklist-active">{active.content}</span> : <span className="task-checklist-spacer" />}
        <Icon name={folded ? 'chevron.down' : 'chevron.up'} size={10} />
      </button>
      {folded ? null : (
        <ul className="task-checklist-list">
          {list.items.map((item, i) => (
            <li key={i} className={`task-checklist-item ${item.status}`}>
              <Icon
                name={item.status === 'completed' ? 'checkmark.circle.fill' : item.status === 'in_progress' ? 'circle.dotted' : 'circle'}
                size={11}
                color={item.status === 'completed' ? 'var(--accent)' : 'var(--tertiary)'}
              />
              <span className={item.status === 'completed' ? 'task-checklist-done' : undefined}>{item.content}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
