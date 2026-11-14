import assert from 'node:assert/strict'
import test from 'node:test'
import type { Network, NetworkComputer, NetworkRule } from '@shared/network'
import { botsTemplate, connectorFor, moved, presenceTitle, ruleLine } from './network-copy.ts'

const computer = (c: Partial<NetworkComputer>): NetworkComputer => ({
  name: 'laptop', role: 'member', me: false, online: false, agents: [], canRemove: false, canPromote: false, ...c,
})

const rule = (r: Partial<NetworkRule>): NetworkRule => ({ fromComputer: '*', fromAgent: '*', toAgent: '*', access: 'deny', ...r })

test('a relay connector needs an address; an empty one would quietly mean public', () => {
  assert.deepEqual(connectorFor('public', 'ignored'), { connector: 'public' })
  assert.deepEqual(connectorFor('private', ''), { connector: 'private' })
  assert.deepEqual(connectorFor('relay', ' https://relay.example.com '), { connector: 'https://relay.example.com' })
  assert.ok('problem' in connectorFor('relay', '  '))
})

test('rows say where each computer is and how many bots it offers', () => {
  assert.equal(presenceTitle(computer({ me: true })), 'This computer')
  assert.equal(presenceTitle(computer({ online: true })), 'Online')
  assert.equal(presenceTitle(computer({})), 'Away')
  assert.equal(botsTemplate(computer({ agents: [{ id: 'a', name: 'A' }] })), '1 bot')
  assert.equal(botsTemplate(computer({})), '{count} bots')
})

test('rules read with the bots by name, and move within the list', () => {
  const net: Network = {
    id: 'network', name: 'home', computer: 'desk', role: 'owner', canInvite: true, canRecover: false, connector: 'public', rules: [],
    computers: [
      computer({ name: 'desk', me: true, agents: [{ id: 'reviewer', name: 'Reviewer' }] }),
      computer({ name: 'laptop', agents: [{ id: 'lead', name: 'Lead' }] }),
    ],
  }
  const tr = (s: string) => s
  assert.equal(ruleLine(rule({}), net, tr), 'Deny: any bot on any computer → any bot here')
  assert.equal(ruleLine(rule({ fromComputer: 'Laptop', access: 'allow', toAgent: 'reviewer' }), net, tr), 'Allow: any bot on Laptop → Reviewer')
  assert.equal(ruleLine(rule({ fromComputer: 'laptop', fromAgent: 'lead' }), net, tr), 'Deny: Lead on laptop → any bot here')
  assert.equal(ruleLine(rule({ fromComputer: 'server', fromAgent: 'ci', toAgent: 'gone' }), net, tr), 'Deny: ci on server → gone')
  const rules = [rule({ toAgent: 'a' }), rule({ toAgent: 'b' })]
  assert.deepEqual(moved(rules, 1, -1).map((r) => r.toAgent), ['b', 'a'])
  assert.equal(moved(rules, 0, -1), rules)
})
