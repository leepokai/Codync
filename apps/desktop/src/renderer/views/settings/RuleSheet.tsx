import { useState } from 'react'
import type { Network, NetworkRule } from '@shared/network'
import { CardSection, ChoiceList, ChoicePicker, IconButton, Reveal, Spinner, Switch, ValueRow } from '../../components/Controls'
import { ModalHeader, useDismiss } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ANY } from '../../lib/network-copy'
import type { BotStore } from '../../store/bot-store'
import { network } from '../../store/network'
import { errorText } from './parts'

const ACCESS: { id: NetworkRule['access']; label: string }[] = [
  { id: 'allow', label: 'Allow' },
  { id: 'deny', label: 'Deny' },
]

/** A new rule for `net`: who on its other computers may message which bot here. Saves the whole list. */
export function RuleSheet({ store, net }: { store: BotStore; net: Network }) {
  const dismiss = useDismiss()
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
      setProblem(errorText(e))
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
          <ChoiceList selection={access} options={ACCESS} onChange={setAccess} />
        </CardSection>
        <CardSection footer="The first rule that matches decides.">
          <ValueRow label="From computer">
            <ChoicePicker selection={fromComputer} options={[{ id: ANY, label: 'Any computer' }, ...others.map((c) => ({ id: c.name, label: c.name }))]} onChange={pickComputer} />
          </ValueRow>
          <ValueRow label="From bot">
            <ChoicePicker selection={fromAgent} options={[{ id: ANY, label: 'Any bot' }, ...(from?.agents ?? []).map((a) => ({ id: a.id, label: a.name }))]} onChange={setFromAgent} />
          </ValueRow>
          <ValueRow label="To bot">
            <ChoicePicker selection={toAgent} options={[{ id: ANY, label: 'Any bot here' }, ...(me?.agents ?? []).map((a) => ({ id: a.id, label: a.name }))]} onChange={setToAgent} />
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
