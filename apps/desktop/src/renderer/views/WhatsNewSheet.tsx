import { CardForm, CardSection } from '../components/Controls'
import { ModalHeader, SheetPage } from '../components/Overlay'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { useWhatsNew } from '../lib/use-whats-new'
import { FormSheet } from './marketplace/pieces'

/** The highlights of the releases this device hasn't seen, once, over the chat window (docs/features/whats-new.md). */
export function WhatsNewSheet({ ready }: { ready: boolean }) {
  const tr = useTr()
  const { open, due, close } = useWhatsNew(ready)
  return (
    <FormSheet open={open} onClose={close}>
      <SheetPage>
        <ModalHeader icon="sparkles" title={tr("What's new")} />
        <CardForm>
          {due.map((a) => (
            <CardSection key={a.version} title={a.version}>
              {a.notes.map((note) => <Text key={note} size="body">{tr(note)}</Text>)}
            </CardSection>
          ))}
        </CardForm>
      </SheetPage>
    </FormSheet>
  )
}
