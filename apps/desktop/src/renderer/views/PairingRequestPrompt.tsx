import { useEffect } from 'react'
import { Dialog } from '../components/Overlay'
import { pairingAskText } from '../lib/account-copy'
import { useModel, useModels } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useApp } from '../store/context'

/**
 * A signed-in phone scanned a computer this app manages that is in no account: let it in, and the computer joins
 * the phone's account (docs/plans/require-sign-in.md, phase 2). The phone shows the same code. Unanswered, the
 * host turns the phone away after 90 seconds.
 */
export function PairingRequestPrompt() {
  const tr = useTr()
  const app = useModel(useApp())
  useModels(app.managedStores)
  const ask = app.pairingAsks[0] ?? null
  // The phone is in someone's hand at this computer, maybe scanning the menu bar's pairing window.
  useEffect(() => {
    if (ask) window.codync.app.showWindow()
  }, [ask?.id])

  const decide = (approve: boolean) => {
    if (!ask) return
    void app.decidePairing(ask, approve)
  }
  const text = ask ? pairingAskText(tr, ask.request) : null
  return (
    <Dialog
      open={ask !== null}
      title={text?.title ?? ''}
      message={text?.message}
      // A choice either way: closing would leave the phone waiting.
      cancel={null}
      actions={[
        { title: "Don't allow", action: () => decide(false) },
        { title: 'Let in', action: () => decide(true) },
      ]}
      onClose={() => {}}
    />
  )
}
