import assert from 'node:assert/strict'
import { test } from 'node:test'
import { buildChannelCatalog, matchChannels, stateOf } from './channel-catalog.ts'

const telegramBot = {
  name: 'home',
  username: 'codync_home_bot',
  botId: 'bot-1',
  botName: 'Desk',
  allowed: [42],
  state: 'ok',
  lastReceivedAt: 1_700_000_000_000,
  pairingExpiresAt: null,
}

test('a connected Telegram bot is one channel with its people and bot', () => {
  const [whatsapp, telegram] = buildChannelCatalog({ whatsapp: null, telegram: [telegramBot], slack: null, discord: null })
  assert.equal(whatsapp.connected, false)
  assert.equal(telegram.connected, true)
  assert.equal(telegram.items[0].label, 'home')
  assert.equal(telegram.items[0].botName, 'Desk')
  assert.deepEqual(telegram.items[0].people, [{ id: '42', label: 'Person 42' }])
  assert.equal(telegram.needsAttention, false)
  assert.match(telegram.summary, /home/)
})

test('a failing bot asks for attention and says so', () => {
  const [whatsapp, telegram] = buildChannelCatalog({
    whatsapp: null,
    telegram: [{ ...telegramBot, state: 'error: token revoked' }],
    slack: null,
    discord: null,
  })
  assert.equal(telegram.needsAttention, true)
  assert.equal(telegram.items[0].state, 'error')
  assert.match(telegram.items[0].stateText, /token revoked/)
  assert.equal(whatsapp.items.length, 0)
})

test('Slack and Discord list one row per bound channel, using the connection state', () => {
  const [, , slack, discord] = buildChannelCatalog({
    whatsapp: null,
    telegram: [],
    slack: {
      configured: true,
      socketMode: true,
      workspaceId: 'T1',
      bindings: [{ channelId: 'C1', botId: 'b', allowedUsers: ['u1', 'u2'], memory: 'ship it' }],
    },
    discord: {
      configured: true,
      guildId: 'G',
      botUserId: 'B',
      state: 'starting',
      deliveries: [],
      bindings: [{ channelId: 'D1', botId: 'b', allowedUsers: [], memory: '' }],
    },
  }, (id) => (id === 'b' ? 'Desk' : null))
  assert.equal(slack.connected, true)
  assert.equal(slack.items[0].channel, 'C1')
  assert.equal(slack.items[0].people.length, 2)
  assert.equal(slack.items[0].memory, 'ship it')
  assert.equal(discord.items[0].state, 'starting')
  assert.equal(discord.items[0].botName, 'Desk')
})

test('a linked WhatsApp number is its own channel, and an unlinked one is not listed', () => {
  const connected = buildChannelCatalog({
    whatsapp: { state: 'connected', me: '+34 600', name: 'Work', bot: 'bot-1', allowed: ['Ana'] },
    telegram: [],
    slack: null,
    discord: null,
  })[0]
  assert.equal(connected.connected, true)
  assert.equal(connected.items[0].label, 'Work')
  assert.equal(connected.items[0].subtitle, '+34 600')
  assert.deepEqual(connected.items[0].people.map((p) => p.label), ['Person Ana'])
  assert.equal(buildChannelCatalog({ whatsapp: { state: 'qr' }, telegram: [], slack: null, discord: null })[0].items.length, 0)
})

test('search finds a channel by provider, bot or channel name', () => {
  const entries = buildChannelCatalog({ whatsapp: null, telegram: [telegramBot], slack: null, discord: null })
  assert.deepEqual(matchChannels(entries, 'home').map((e) => e.id), ['telegram'])
  assert.deepEqual(matchChannels(entries, 'desk').map((e) => e.id), ['telegram'])
  assert.deepEqual(matchChannels(entries, 'discord').map((e) => e.id), ['discord'])
  assert.deepEqual(matchChannels(entries, '').map((e) => e.id), ['whatsapp', 'telegram', 'slack', 'discord'])
})

test('state words are the ones the apps show', () => {
  assert.deepEqual(stateOf('ok'), { state: 'ok', text: 'Connected' })
  assert.deepEqual(stateOf('qr'), { state: 'starting', text: 'Waiting for the scan' })
  assert.equal(stateOf('loggedOut').state, 'error')
  assert.equal(stateOf('disconnected').state, 'off')
})
