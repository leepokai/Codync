import { useState } from 'react'
import type { Network, NetworkRule } from '@shared/network'
import { CardSection, ChoiceList, ChoicePicker, IconButton, Reveal, Spinner, Switch, ValueRow } from '../../components/Controls'
import { ModalHeader, useDismiss } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ANY, beforeCeilings, levelsFor } from '../../lib/network-copy'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'
import { network } from '../../store/network'
import { errorText } from './parts'

/** The most trusted first; `auto` and `ask` are ceilings, never freer than the bot is set. */
const ACCESS: { id: NetworkRule['access']; label: string; detail: string }[] = [
  { id: 'allow', label: 'Allow', detail: 'Its requests run as the bot is set.' },
  { id: 'auto', label: 'Auto', detail: 'The bot does by itself only what its agent judges safe, and asks you about the rest.' },
  { id: 'ask', label: 'Ask', detail: 'Every change and command the bot makes for it waits for your approval.' },
  { id: 'deny', label: 'Deny', detail: 'Its messages are refused.' },
]

/** A new rule for `net`: who on its other computers may message which bot here. Saves the whole list. */
export function RuleSheet({ store, net }: { store: BotStore; net: Network }) {
  const dismiss = useDismiss()
  const tr = useTr()
  const [access, setAccess] = useState<NetworkRule['access']>('deny')
  const [fromComputer, setFromComputer] = useState(ANY)
  const [fromAgent, setFromAgent] = useState(ANY)
  const [toAgent, setToAgent] = useState(ANY)
  const [first, setFirst] = useState(false)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const others = net.computers.filter((c) => !c.me)
  const me = net.computers.find((c) => c.me)
  const from = others.find((c) => c.name === fromComputer)
  const sessions = store.network?.sessions ?? []
  const sessionAt = (id: string) => sessions.find((s) => s.address === id)
  const isSession = (id: string) => sessionAt(id) !== undefined
  const accessChoices = ACCESS.filter((a) => levelsFor(sessionAt(toAgent)).includes(a.id))

  const pickTo = (id: string) => {
    setToAgent(id)
    if (!levelsFor(sessionAt(id)).includes(access)) setAccess('deny')
  }

  const pickComputer = (name: string) => {
    setFromComputer(name)
    // A bot belongs to its computer: another computer starts from any of its bots.
    setFromAgent(ANY)
  }

  const save = async () => {
    if (busy) return
    setBusy(true)
    setProblem(null)
    const rule: NetworkRule = { fromComputer, fromAgent, toAgent, access }
    try {
      await network.setRules(store, net.id, first ? [rule, ...net.rules] : [...net.rules, rule])
      dismiss()
    } catch (e) {
      const problem = errorText(e)
      setProblem(beforeCeilings(problem) ? tr('Update Codync on that computer.') : problem)
    } finally {
      setBusy(false)
    }
  }

  return (
    <VStack className="network-sheet">
      <ModalHeader
        icon="list.bullet"
        title="Add rule"
        subtitle={net.name}
        trailing={busy ? <Spinner /> : <IconButton title="Add rule" icon="checkmark" onClick={() => void save()} />}
      />
      <VStack gap={16} className="network-sheet-body">
        <CardSection>
          <ChoiceList selection={access} options={accessChoices} onChange={setAccess} />
        </CardSection>
        <CardSection footer="The first rule that matches decides.">
          <ValueRow label="From computer">
            <ChoicePicker selection={fromComputer} options={[{ id: ANY, label: 'Any computer' }, ...others.map((c) => ({ id: c.name, label: c.name }))]} onChange={pickComputer} />
          </ValueRow>
          <ValueRow label="From bot">
            <ChoicePicker selection={fromAgent} options={[{ id: ANY, label: 'Any bot' }, ...(from?.agents ?? []).map((a) => ({ id: a.id, label: a.name }))]} onChange={setFromAgent} />
          </ValueRow>
          <ValueRow label="To bot">
            <ChoicePicker
              selection={toAgent}
              options={[
                { id: ANY, label: 'Any bot here' },
                ...(me?.agents ?? []).filter((a) => !isSession(a.id)).map((a) => ({ id: a.id, label: a.name })),
                ...sessions.map((s) => ({ id: s.address, label: s.address })),
              ]}
              onChange={pickTo}
            />
          </ValueRow>
          <Switch on={first} onChange={setFirst}>Put it first</Switch>
        </CardSection>
        <Reveal show={problem !== null}>
          <Text size="caption" tone="danger">{problem}</Text>
        </Reveal>
      </VStack>
    </VStack>
  )
}
