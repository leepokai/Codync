import { useState } from 'react'
import { Dialog, DialogField } from '../components/Overlay'
import { limitSectionName, sectionName } from '../lib/roster-sections'
import { useTr } from '../lib/use-language'
import type { SectionNaming } from './section-menus'

/** Asks for a section's name: a new section for a bot, or a section's new name. */
export function SectionNameDialog({ naming, onClose }: { naming: SectionNaming | null; onClose: () => void }) {
  const tr = useTr()
  const [name, setName] = useState('')
  const [opened, setOpened] = useState<SectionNaming | null>(null)
  // Each opening starts from its own name: empty for a new section, the current one for a rename.
  if (naming && naming !== opened) {
    setOpened(naming)
    setName(naming.name)
  }
  const save = () => {
    const typed = sectionName(name)
    if (naming && typed) naming.save(typed)
  }
  return (
    <Dialog open={naming !== null} title={naming ? tr(naming.title) : ''} actions={[{ title: 'Save', action: save }]} onClose={onClose}>
      {/* The host refuses longer names; the field stops there first. */}
      <DialogField value={name} placeholder={tr('Section name')} label={tr('Section name')} onChange={(text) => setName(limitSectionName(text))}
        onSubmit={() => {
          onClose()
          save()
        }} />
    </Dialog>
  )
}
