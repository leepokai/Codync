import { useState } from 'react'
import { loginProfileChoices, type Backend } from '@shared/models'
import { ChoicePicker, ValueRow } from '../../components/Controls'
import { AgentSheet } from '../marketplace/AgentSheet'
import { FormSheet } from '../marketplace/pieces'

const DEFAULT = ''
const ADD = '+'

/**
 * Which of the agent's login profiles a bot runs on: the default one (the login the computer already has), or one of
 * its others. "Add login profile…" opens the agent's setup, where a new one signs in or gets its keys.
 */
export function LoginProfileRow({ backend, loginProfile, free, onChange }: {
  backend: Backend
  loginProfile: string | null
  /** The login profiles that can be picked (`null`: the default one), when not all can: a fallback's, whose chain has the others. */
  free?: (string | null)[]
  onChange: (loginProfile: string | null) => void
}) {
  const [adding, setAdding] = useState(false)
  const open = (id: string | null) => !free || free.includes(id)
  const options = [
    ...(open(null) ? [{ id: DEFAULT, label: backend.defaultLoginProfile?.name ?? 'Default login profile' }] : []),
    ...loginProfileChoices(backend, loginProfile).filter((p) => open(p.id)).map((p) => ({ id: p.id, label: p.name ?? 'New login profile' })),
    { id: ADD, label: 'Add login profile…' },
  ]
  return (
    <>
      <ValueRow label="Login profile">
        <ChoicePicker selection={loginProfile ?? DEFAULT} options={options} onChange={(id) => (id === ADD ? setAdding(true) : onChange(id || null))} />
      </ValueRow>
      <FormSheet open={adding} onClose={() => setAdding(false)} width={560} height={560} fixed>
        {adding ? <AgentSheet initial={backend} /> : null}
      </FormSheet>
    </>
  )
}
