import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { BotStore } from '../store/bot-store'
import { makeLeadItems, offersDelete, voiceTarget } from './lead-menus.ts'

/** A computer's store as the menus read it, logging the bots made lead. */
function computer(hasLeadChoice = true) {
  const made: string[] = []
  const store = { hasLeadChoice, setLead: (bot: Bot) => made.push(bot.id) } as unknown as BotStore
  return { store, made }
}

const lead = { id: 'lead', lead: true } as Bot
const bot = { id: 'bot', lead: false } as Bot
const group = { id: 'group', lead: false, kind: 'group' } as Bot

test('any other bot can be made the lead, and the lead deleted', () => {
  const { store, made } = computer()
  const items = makeLeadItems(bot, store)
  assert.deepEqual(items.map((i) => [i.title, i.icon]), [['Make lead agent', 'star']])
  items[0]!.action()
  assert.deepEqual(made, ['bot'])
  assert.deepEqual(makeLeadItems(lead, store), [], 'the lead already is')
  assert.deepEqual(makeLeadItems(group, store), [], 'a group can\'t lead')
  assert.ok(offersDelete(lead, store) && offersDelete(bot, store))
})

test('a host older than 2.33.0 keeps its lead', () => {
  const { store } = computer(false)
  assert.deepEqual(makeLeadItems(bot, store), [])
  assert.ok(!offersDelete(lead, store) && offersDelete(bot, store))
})

test('the voice shortcut talks to the lead, else the top bot', () => {
  const row = (b: Bot) => ({ bot: b })
  assert.equal(voiceTarget({ items: [row(bot), row(lead)], navigable: [row(bot)] })?.bot, lead)
  assert.equal(voiceTarget({ items: [row(group), row(bot)], navigable: [row(bot)] })?.bot, bot, 'no lead: the top bot shown')
  assert.equal(voiceTarget({ items: [], navigable: [] }), undefined)
})
