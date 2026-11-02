import assert from 'node:assert/strict'
import { test } from 'node:test'
import { chosen, detectSource, hermesEntries, initialTicks, isCandidate, isSecretFile, looksLikeCredential, memoryFacts, parseIdentity, parseSkill, planImport } from './agent-import.ts'

const KEY = 'sk-proj-' + 'a1B2c3D4e5F6g7H8i9J0k1L2m3N4'

const openclaw = {
  'SOUL.md': 'You are calm and precise.',
  'AGENTS.md': 'Always run the tests.',
  'IDENTITY.md': '- Name: Nova\n- Creature: A friendly lighthouse\n- Emoji: 🌟\n- Vibe: pick something you like',
  'MEMORY.md': 'Prefers short answers.\n\nLives in Lisbon.',
  'USER.md': 'Name: Ada',
  'memory/2026-01-02.md': 'Shipped the release.',
  'HEARTBEAT.md': 'check every hour',
  'skills/deploy/SKILL.md': '---\nname: deploy\ndescription: Ship it\n---\nRun `make release`.',
  'skills/empty/SKILL.md': '---\nname: empty\n---\n',
}

const hermes = {
  'SOUL.md': 'Be helpful.',
  'HERMES.md': 'Use the terminal.',
  'memories/MEMORY.md': 'Likes tea\n§\nWorks nights\n§\n\n§\nHas a dog',
  'memories/USER.md': 'Ada',
  'skills/.bundled_manifest': 'web-search:abc123\nplanner:def456',
  'skills/web-search/SKILL.md': '---\nname: web-search\n---\nSearches.',
  'skills/mine/SKILL.md': '---\nname: mine\ndescription: Mine\n---\nDo my thing.',
}

test('an OpenClaw folder becomes a named plan with instructions, memory and skills', () => {
  const plan = planImport(openclaw)
  assert.ok(plan)
  assert.equal(plan.source, 'openclaw')
  assert.equal(plan.name, 'Nova')
  assert.equal(plan.description, 'A friendly lighthouse')
  assert.deepEqual(plan.instructions.map((i) => i.id), ['SOUL.md', 'AGENTS.md'])
  assert.deepEqual(plan.memory.map((i) => i.id), ['MEMORY.md', 'USER.md', 'memory/2026-01-02.md'])
  assert.deepEqual(plan.skills.map((s) => [s.name, s.description]), [['deploy', 'Ship it']])
  assert.deepEqual(plan.skipped.map((s) => [s.path, s.reason]), [['HEARTBEAT.md', 'runtime'], ['skills/empty/SKILL.md', 'empty']])
})

test('a Hermes folder keeps its own skills and memory paragraphs, not the ones it installed itself', () => {
  const plan = planImport(hermes)
  assert.ok(plan)
  assert.equal(plan.source, 'hermes')
  assert.equal(plan.name, 'Hermes')
  assert.deepEqual(plan.instructions.map((i) => i.id), ['SOUL.md', 'HERMES.md'])
  assert.equal(plan.memory[0]?.text, 'Likes tea\n\nWorks nights\n\nHas a dog')
  assert.deepEqual(plan.skills.map((s) => s.name), ['mine'])
  assert.deepEqual(plan.skipped.map((s) => [s.path, s.reason]), [['skills/web-search/SKILL.md', 'builtin']])
})

test('a folder with only an AGENTS.md is a plain agent document and anything else is not an agent', () => {
  assert.equal(planImport({ 'AGENTS.md': 'Rules.' })?.source, 'agents')
  assert.equal(planImport({ 'README.md': 'hello' }), null)
  assert.equal(detectSource([]), null)
})

test('a credential-shaped text is omitted, ticked or not; the rest starts ticked', () => {
  const plan = planImport({ ...openclaw, 'MEMORY.md': `The key is ${KEY}`, 'USER.md': 'Name: Ada' })
  assert.ok(plan)
  const memory = Object.fromEntries(plan.memory.map((m) => [m.id, m]))
  assert.equal(memory['MEMORY.md']?.credential, true)
  assert.equal(memory['MEMORY.md']?.selected, false)
  assert.equal(memory['USER.md']?.credential, false)
  const ticks = initialTicks(plan)
  assert.ok(!ticks.has('MEMORY.md') && ticks.has('USER.md') && ticks.has('SOUL.md'))
  // Even ticked, it is omitted: a credential never travels.
  assert.equal(chosen(plan, ticks).memory.some((m) => m.credential), false)
  assert.equal(chosen(plan, new Set([...ticks, 'MEMORY.md'])).memory.some((m) => m.credential), false)
})

test('keys of the usual providers are recognised and ordinary words are not', () => {
  for (const secret of [KEY, 'ghp_' + 'a'.repeat(36), 'xoxb-1234567890-abcdefghij', 'AKIA' + 'ABCDEFGHIJKLMNOP', '-----BEGIN RSA PRIVATE KEY-----', '123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_', 'Bearer abcdEFGH1234abcdEFGH1234abcd']) {
    assert.ok(looksLikeCredential(`token ${secret} here`), secret)
  }
  for (const fine of ['sk-learn is a library', 'sk-your-key-here', 'Bearer <token>', 'We use Postgres 16.', 'the AKIA prefix']) {
    assert.ok(!looksLikeCredential(fine), fine)
  }
})

test('settings and secret files are named and never read', () => {
  for (const secret of ['.env', 'config/.env.local', 'auth.json', 'openclaw.json', 'credentials', 'id_rsa.pem', 'secrets.yaml']) {
    assert.ok(isSecretFile(secret), secret)
    assert.ok(!isCandidate(secret), secret)
  }
  const plan = planImport(openclaw, ['openclaw.json', '.env'])
  assert.deepEqual(plan?.skipped.filter((s) => s.reason === 'secret').map((s) => s.path), ['.env', 'openclaw.json'])
  assert.ok(!JSON.stringify(plan).includes('apiKey'))
})

test('the unrelated files of a folder are not candidates', () => {
  for (const path of ['SOUL.md', 'memory/2026-01-02.md', 'skills/a/SKILL.md', 'skills/a/b/SKILL.md', 'memories/MEMORY.md']) assert.ok(isCandidate(path), path)
  for (const path of ['src/index.ts', 'notes.md', 'memory/sub/x.md', 'skills/a/script.sh', 'node_modules/x/SKILL.md']) assert.ok(!isCandidate(path), path)
})

test('pieces: hermes entries, skill front matter, identity hints and memory facts', () => {
  assert.equal(hermesEntries('a\n§\n\n§\nb'), 'a\n\nb\n')
  assert.equal(hermesEntries(''), '')
  assert.deepEqual(parseSkill('---\nname: "x"\ndescription: Does x\n---\nBody'), { name: 'x', description: 'Does x', body: 'Body' })
  assert.equal(parseSkill('# Title\nbody').name, 'Title')
  assert.deepEqual(parseIdentity('- **Name:** Zed\n- Vibe: (how do you come across? sharp? warm?)'), { name: 'Zed', description: '' })
  const facts = memoryFacts([{ id: 'm', title: 'm', text: '- Likes tea\n\nWorks\nnights', selected: true, credential: false, stock: false }])
  assert.deepEqual(facts, ['Likes tea', 'Works nights'])
})
