import { useState } from 'react'
import { accountChoices, type Backend } from '@shared/models'
import { ChoicePicker, ValueRow } from '../../components/Controls'
import { AgentSheet } from '../marketplace/AgentSheet'
import { FormSheet } from '../marketplace/pieces'

const OWN = ''
const ADD = '+'

/**
 * Which of the agent's accounts a bot runs on: the default one (the login the computer already has), or one of its other
 * accounts. "Add account…" opens the agent's setup, where a new one signs in.
 */
export function AccountRow({ backend, account, free, onChange }: {
  backend: Backend
  account: string | null
  /** The accounts that can be picked (`null`: the default one), when not all can: a fallback's, whose chain has the others. */
  free?: (string | null)[]
  onChange: (account: string | null) => void
}) {
  const [adding, setAdding] = useState(false)
  const open = (id: string | null) => !free || free.includes(id)
  const options = [
    ...(open(null) ? [{ id: OWN, label: backend.ownAccount?.name ?? 'Default account' }] : []),
    ...accountChoices(backend, account).filter((a) => open(a.id)).map((a) => ({ id: a.id, label: a.name ?? 'New account' })),
    { id: ADD, label: 'Add account…' },
  ]
  return (
    <>
      <ValueRow label="Account">
        <ChoicePicker selection={account ?? OWN} options={options} onChange={(id) => (id === ADD ? setAdding(true) : onChange(id || null))} />
      </ValueRow>
      <FormSheet open={adding} onClose={() => setAdding(false)} width={560} height={560} fixed>
        {adding ? <AgentSheet initial={backend} /> : null}
      </FormSheet>
    </>
  )
}
