import { useState } from 'react'
import { Dialog } from '../components/Overlay'
import { limitSectionName } from '../lib/roster-sections'
import type { SectionNaming } from './section-menus'

/** Asks for a section's name: a new section for a bot, or a section's new name. */
export function SectionNameDialog({ naming, onClose }: { naming: SectionNaming | null; onClose: () => void }) {
  const [name, setName] = useState('')
  const [opened, setOpened] = useState<SectionNaming | null>(null)
  // Each opening starts from its own name: empty for a new section, the current one for a rename.
  if (naming && naming !== opened) {
    setOpened(naming)
    setName(naming.name)
  }
  const save = () => {
    if (naming && name.trim()) naming.save(name)
  }
  return (
    <Dialog open={naming !== null} title={naming?.title ?? ''} actions={[{ title: 'Save', action: save }]} onClose={onClose}>
      {/* The host refuses longer names; the field stops there first. */}
      <input className="dialog-field" data-autofocus value={name} placeholder="Section name" aria-label="Section name" spellCheck={false}
        onChange={(e) => setName(limitSectionName(e.target.value))}
        onKeyDown={(e) => {
          if (e.key !== 'Enter' || e.nativeEvent.isComposing || e.keyCode === 229) return
          e.preventDefault()
          onClose()
          save()
        }} />
    </Dialog>
  )
}
