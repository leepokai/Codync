import { useState } from 'react'
import { Dialog } from '../components/Overlay'
import { useTr } from '../lib/use-language'

/** An open name dialog: its title, the name it starts with, and what Save does with the typed one. */
export interface Naming {
  title: string
  name: string
  save: (name: string) => void
}

/**
 * Asks for a name (a section's, a cloud computer's): `limit` keeps the field to what's kept, `clean` reads the typed
 * text as the name to save (null: nothing to save yet).
 */
export function NameDialog({ naming, placeholder, limit, clean, onClose }: {
  naming: Naming | null
  placeholder: string
  limit: (typed: string) => string
  clean: (typed: string) => string | null
  onClose: () => void
}) {
  const tr = useTr()
  const [name, setName] = useState('')
  const [opened, setOpened] = useState<Naming | null>(null)
  // Each opening starts from its own name: empty for a new one, the current one for a rename.
  if (naming && naming !== opened) {
    setOpened(naming)
    setName(naming.name)
  }
  const typed = clean(name)
  const save = () => {
    if (naming && typed) naming.save(typed)
  }
  return (
    <Dialog open={naming !== null} title={naming ? tr(naming.title) : ''} actions={[{ title: 'Save', disabled: !typed, action: save }]} onClose={onClose}>
      <input className="dialog-field" data-autofocus value={name} placeholder={tr(placeholder)} aria-label={tr(placeholder)} spellCheck={false}
        onChange={(e) => setName(limit(e.target.value))}
        onKeyDown={(e) => {
          if (e.key !== 'Enter' || e.nativeEvent.isComposing || e.keyCode === 229 || !typed) return
          e.preventDefault()
          onClose()
          save()
        }} />
    </Dialog>
  )
}
