import { useState } from 'react'
import type { Entry, HandoffCard } from '@shared/models'
import { Button, CardForm, CardSection, Disclosure, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useStore as useLiveStore } from '../../store/context'
import { Field } from '../marketplace/pieces'
import './handoff.css'
import { useTr } from '../../lib/use-language'


function Line({ icon, text }: { icon: string; text?: string }) {
  return (
    <span className="handoff-line">
      <Icon name={icon} size={11} />
      <span>{text}</span>
    </span>
  )
}

/** Another agent of the bot's chain carried on, or its own is back: one line that unfolds to what the agent was told. */
export function AgentSwitchNotice({ entry, carried }: { entry: Entry; carried: string | null }) {
  const [expanded, setExpanded] = useState(false)
  const line = <Line icon="arrow.turn.down.right" text={entry.data.text} />
  return (
    <div className="handoff-folded">
      {carried ? (
        <Disclosure expanded={expanded} onToggle={() => setExpanded(!expanded)} label={line}>
          <pre className="handoff-text agent-switch-carried fade-in">{carried}</pre>
        </Disclosure>
      ) : line}
    </div>
  )
}

/** The bot's chat starts over: what the new conversation carries, which the user can edit, or turn down to carry on. */
export function HandoffNotice({ entry, card }: { entry: Entry; card: HandoffCard }) {
  const tr = useTr()
  const store = useLiveStore()
  const [expanded, setExpanded] = useState(false)
  const [draft, setDraft] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const carried = <pre className="handoff-text handoff-carried">{card.text}</pre>
  if (card.status !== 'draft') {
    return (
      <div className="handoff-folded">
        <Disclosure expanded={expanded} onToggle={() => setExpanded(!expanded)} label={<Line icon="arrow.triangle.branch" text={entry.data.text} />}>
          {carried}
        </Disclosure>
      </div>
    )
  }
  const save = async () => {
    if (draft === null || !draft.trim()) return
    setSaving(true)
    if (await store.editHandoff(entry, draft.trim())) setDraft(null)
    setSaving(false)
  }
  return (
    <div className="handoff-card">
      <Line icon="arrow.triangle.branch" text={entry.data.text} />
      <Disclosure expanded={expanded} onToggle={() => setExpanded(!expanded)} label={<Text size="footnote" weight="medium">{tr(`What it carries`)}</Text>}>
        {carried}
      </Disclosure>
      <div className="handoff-actions">
        {store.answering.has(entry.id) ? <Spinner /> : <>
          <Button kind="secondary" onClick={() => store.keepConversation(entry)}>Keep this conversation</Button>
          <IconButton title="Edit what it carries" icon="pencil" onClick={() => setDraft(card.text ?? '')} />
        </>}
      </div>
      <Sheet open={draft !== null} onClose={() => setDraft(null)} width={620}>
        {draft !== null && <SheetPage>
          <ModalHeader
            title={tr(`Edit the handoff`)}
            trailing={saving ? <Spinner /> : <IconButton title="Save" icon="checkmark" disabled={!draft.trim()} onClick={() => { void save() }} />}
          />
          <CardForm>
            <CardSection footer="Your next message starts a fresh conversation that begins with this.">
              <Field value={draft} onChange={setDraft} label="Handoff" lines={[10, 24]} mono />
            </CardSection>
          </CardForm>
        </SheetPage>}
      </Sheet>
    </div>
  )
}
