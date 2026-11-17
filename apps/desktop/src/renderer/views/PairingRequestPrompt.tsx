import { useEffect } from 'react'
import { Dialog } from '../components/Overlay'
import { pairingAnswers, pairingAskText } from '../lib/account-copy'
import { useModel, useModels } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useApp } from '../store/context'

/**
 * A phone asks about a computer this app manages that is in no account: let it in, and join its account when it has one
 * (docs/plans/optional-sign-in.md). The phone shows the same code. Unanswered, the host turns the phone away after 90
 * seconds.
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

  const decide = (approve: boolean, join?: boolean) => {
    if (!ask) return
    void app.decidePairing(ask, approve, join)
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
        ...(ask ? pairingAnswers(ask.request).map(({ title, join }) => ({ title, action: () => decide(true, join) })) : []),
      ]}
      onClose={() => {}}
    />
  )
}
