import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { CATEGORIES } from './market-models.ts'

test('every bundled Hermes card belongs to a category all clients display', () => {
  const skills = JSON.parse(readFileSync(new URL('../../../../../../host/src/market/hermes/skills.json', import.meta.url), 'utf8'))
  const apps = JSON.parse(readFileSync(new URL('../../../../../../host/src/market/hermes/apps.json', import.meta.url), 'utf8'))
  assert.equal(skills.skills.length, 185)
  assert.equal(apps.apps.length, 66)
  for (const item of [...skills.skills, ...apps.apps]) {
    assert.ok((CATEGORIES as readonly string[]).includes(item.category), `${item.id}: ${item.category} would disappear from the marketplace`)
  }
  for (const app of apps.apps) assert.ok(app.origin.includes(apps.commit))
})
