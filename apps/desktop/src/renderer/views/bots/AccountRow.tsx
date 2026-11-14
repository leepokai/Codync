import { useState } from 'react'
import { accountChoices, type Backend } from '@shared/models'
import { ChoicePicker, ValueRow } from '../../components/Controls'
import { AgentSheet } from '../marketplace/AgentSheet'
import { FormSheet } from '../marketplace/pieces'

const OWN = ''
const ADD = '+'

/**
 * Which of the agent's logins a bot runs on: the one the computer already has, or one of its other
 * accounts. "Add account…" opens the agent's setup, where a new one signs in.
 */
export function AccountRow({ backend, account, onChange }: { backend: Backend; account: string | null; onChange: (account: string | null) => void }) {
  const [adding, setAdding] = useState(false)
  const options = [
    { id: OWN, label: `This computer's sign-in` },
    ...accountChoices(backend, account).map((a) => ({ id: a.id, label: a.name ?? 'New account' })),
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
