import { useState } from 'react'
import { Button, CardSection } from '../../components/Controls'
import { Dialog } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'

type Question = { title: string; message: string; button: string }

/** A dangerous, final action, asked twice: the first answer only opens the second question. */
function ConfirmTwice({ open, ask, final, run, onClose }: { open: boolean; ask: Question; final: Question; run: () => void; onClose: () => void }) {
  const [asked, setAsked] = useState(false)
  return (
    <>
      {/* Its own close follows the answer too, so only a cancel ends it. */}
      <Dialog open={open && !asked} title={ask.title} message={ask.message} actions={[{ title: ask.button, destructive: true, action: () => setAsked(true) }]} onCancel={onClose} onClose={() => {}} />
      <Dialog
        open={open && asked}
        title={final.title}
        message={final.message}
        actions={[{ title: final.button, destructive: true, action: run }]}
        onClose={() => {
          setAsked(false)
          onClose()
        }}
      />
    </>
  )
}

/** Erases Codync on this computer (the main process's `resetAllData`), from Settings or the menu bar's Reset all data…. */
export function ResetAllDataDialogs({ open, onClose }: { open: boolean; onClose: () => void }) {
  const tr = useTr()
  return (
    <ConfirmTwice
      open={open}
      onClose={onClose}
      ask={{ title: tr('Reset all data?'), message: tr("Signs out every account and deletes this computer's bots, conversations, keys and settings. Codync then starts over from the welcome screen."), button: 'Reset everything' }}
      final={{ title: tr("Delete all of Codync's data for good?"), message: tr("This can't be undone."), button: 'Delete everything' }}
      run={() => void window.codync.app.resetAllData()}
    />
  )
}

/** This computer's resets together, each asked twice (only the question's own button is red): its saved credentials, and all of Codync's data. */
export function ResetSection({ store }: { store: BotStore }) {
  const tr = useTr()
  useModel(store)
  const [asking, setAsking] = useState<'credentials' | 'everything' | null>(null)
  const [busy, setBusy] = useState(false)
  const name = store.hostName
  // Hosts before resetCredentials don't offer it.
  const credentials = store.connection.kind === 'online' && !store.mismatch && store.hello?.credentialsReset === true
  const resetCredentials = () => {
    setBusy(true)
    void store.resetCredentials().finally(() => setBusy(false))
  }
  return (
    <>
      <CardSection title="Reset">
        {[
          credentials ? (
            <HStack key="credentials" gap={8} className="settings-row settings-appear">
              <VStack grow className="settings-stack">
                <Text>{tr('Saved credentials')}</Text>
                <Text size="caption" tone="secondary">{tr('Keys and tokens saved in Codync for agents, connectors and voice chat.')}</Text>
              </VStack>
              <Button kind="secondary" onClick={() => setAsking('credentials')} disabled={busy}>
                Reset
              </Button>
            </HStack>
          ) : null,
          <HStack key="everything" gap={8} className="settings-row">
            <VStack grow className="settings-stack">
              <Text>{tr('All data')}</Text>
              <Text size="caption" tone="secondary">{tr('Every account, bot, conversation, key and setting on this computer.')}</Text>
            </VStack>
            <Button kind="secondary" onClick={() => setAsking('everything')}>
              Reset everything
            </Button>
          </HStack>,
        ]}
      </CardSection>
      <ConfirmTwice
        open={asking === 'credentials'}
        onClose={() => setAsking(null)}
        ask={{
          title: tr('Reset saved credentials on {name}?').replaceAll('{name}', () => name),
          message: tr("Deletes the keys and tokens Codync saved on {name}: agent keys, connectors, connected apps, voice chat keys and saved logins. Use it when they can't be read anymore, as after moving to a new computer, then enter them again. Agents signed in on their own stay signed in.").replaceAll('{name}', () => name),
          button: 'Reset',
        }}
        final={{
          title: tr('Delete saved credentials on {name} for good?').replaceAll('{name}', () => name),
          message: tr("Codync can't bring them back: you'll enter each key and connect each connector again. This can't be undone."),
          button: 'Delete saved credentials',
        }}
        run={resetCredentials}
      />
      <ResetAllDataDialogs open={asking === 'everything'} onClose={() => setAsking(null)} />
    </>
  )
}
