import type { Network, NetworkComputer, NetworkRule } from '@shared/network'
import { Button, CardSection, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { botsTemplate, moved, presenceTitle, roleTitle, ruleLine } from '../../lib/network-copy'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'

/** What a network card asks its section to do. */
export interface NetworkActions {
  invite: () => void
  recover: () => void
  refresh: () => void
  leave: () => void
  promote: (computer: NetworkComputer) => void
  remove: (computer: NetworkComputer) => void
  /** Puts a computer this one manages over SSH in the network. */
  add: (computer: BotStore) => void
  addRule: (of: RulesOf) => void
  setRules: (of: RulesOf, rules: NetworkRule[]) => void
}

/** A computer's own copy of a network, whose rules say who may message its bots. */
export interface RulesOf {
  store: BotStore
  net: Network
}

/**
 * One of this computer's networks: who is in it, the computers it manages over SSH that aren't
 * (`outside`), and who may message the bots on each computer here that is (`rulesOf`, this one first).
 */
export function NetworkCard({ net, outside, rulesOf, several, disabled, act }: { net: Network; outside: BotStore[]; rulesOf: RulesOf[]; several: boolean; disabled: boolean; act: NetworkActions }) {
  const tr = useTr()
  const title = (
    <HStack as="span" gap={8}>
      {net.name}
      <Text as="span" size="caption" tone="secondary">{`${net.computer} · ${tr(roleTitle(net.role))}`}</Text>
    </HStack>
  )
  const accessory = (
    <HStack gap={2}>
      {net.canInvite ? <IconButton title="Invite a computer" icon="person.crop.circle.badge.plus" disabled={disabled} onClick={act.invite} /> : null}
      {net.canRecover ? <IconButton title="Recover ownership" icon="key" disabled={disabled} onClick={act.recover} /> : null}
      <IconButton title="Refresh" icon="arrow.clockwise" disabled={disabled} onClick={act.refresh} />
      <IconButton title="Leave network" icon="rectangle.portrait.and.arrow.right" disabled={disabled} onClick={act.leave} />
    </HStack>
  )
  const footer = several
    ? tr('Bots message the bots on these computers as bot@computer.{network}.').replaceAll('{network}', net.name)
    : 'Bots message the bots on these computers as bot@computer.'
  return (
    <VStack gap={14}>
      <CardSection title={title} accessory={accessory} footer={footer}>
        {net.computers.map((c) => (
          <ComputerRow key={c.name} computer={c} disabled={disabled} promote={() => act.promote(c)} remove={() => act.remove(c)} />
        ))}
        {outside.map((o) => (
          <HStack key={o.computer.id} gap={8} className="settings-row settings-appear">
            <span className="network-dot" aria-hidden />
            <VStack grow className="settings-stack">
              <Text>{o.hostName}</Text>
              <Text size="caption" tone="secondary">{`${tr('Over SSH')} · ${tr('Not in the network')}`}</Text>
            </VStack>
            <IconButton title="Add to the network" icon="plus" disabled={disabled || o.connection.kind !== 'online' || o.mismatch !== null} onClick={() => act.add(o)} />
          </HStack>
        ))}
      </CardSection>
      {rulesOf.map((of, n) => (
        <CardSection
          key={of.store.computer.id}
          title={rulesOf.length > 1 ? tr('Who may message bots on {name}').replaceAll('{name}', of.net.computer) : 'Who may message bots here'}
          accessory={<IconButton title="Add rule" icon="plus" disabled={disabled} onClick={() => act.addRule(of)} />}
          footer={
            n === rulesOf.length - 1
              ? "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in."
              : null
          }
        >
          {of.net.rules.map((rule, i) => (
            <HStack key={i} gap={4} className="settings-row settings-appear">
              <Text grow>{ruleLine(rule, of.net, tr)}</Text>
              <IconButton title="Move up" icon="chevron.up" disabled={disabled || i === 0} onClick={() => act.setRules(of, moved(of.net.rules, i, -1))} />
              <IconButton title="Move down" icon="chevron.down" disabled={disabled || i === of.net.rules.length - 1} onClick={() => act.setRules(of, moved(of.net.rules, i, 1))} />
              <IconButton title="Delete rule" icon="trash" disabled={disabled} onClick={() => act.setRules(of, of.net.rules.filter((_, j) => j !== i))} />
            </HStack>
          ))}
        </CardSection>
      ))}
    </VStack>
  )
}

function ComputerRow({ computer: c, disabled, promote, remove }: { computer: NetworkComputer; disabled: boolean; promote: () => void; remove: () => void }) {
  const tr = useTr()
  const bots = tr(botsTemplate(c)).replaceAll('{count}', String(c.agents.length))
  const detail = [tr(roleTitle(c.role)), tr(presenceTitle(c)), bots].join(' · ')
  return (
    <HStack gap={8} className="settings-row settings-appear">
      <span className={`network-dot${c.me || c.online ? ' online' : ''}`} aria-hidden />
      <VStack grow className="settings-stack">
        <Text>{c.name}</Text>
        <Text size="caption" tone="secondary" truncate title={c.agents.map((a) => a.name).join(', ') || undefined}>{detail}</Text>
      </VStack>
      {c.canPromote ? <Button kind="secondary" disabled={disabled} onClick={promote}>Make admin</Button> : null}
      {c.canRemove ? <IconButton title="Remove from the network" icon="trash" disabled={disabled} onClick={remove} /> : null}
    </HStack>
  )
}
