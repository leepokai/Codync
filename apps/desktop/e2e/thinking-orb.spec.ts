import { test, expect, type Locator, type Page } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, setPref } from './sandbox.ts'

// A scripted agent whose turn runs a search, a fetch and an edit and stays on them until it is stopped.
const busyAgent = `import json,sys
def send(m): sys.stdout.write(json.dumps(m)+"\\n"); sys.stdout.flush()
def upd(u): send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":u}})
turn=[None]
for line in sys.stdin:
    m=json.loads(line)
    meth=m.get("method")
    if meth=="initialize": send({"jsonrpc":"2.0","id":m["id"],"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":False}}})
    elif meth=="session/new": send({"jsonrpc":"2.0","id":m["id"],"result":{"sessionId":"s1"}})
    elif meth=="session/prompt":
        turn[0]=m["id"]
        for i,(kind,title) in enumerate([("search","Search the docs"),("fetch","Fetch the page"),("edit","Edit a.txt")]):
            upd({"sessionUpdate":"tool_call","toolCallId":"t%d"%i,"title":title,"kind":kind,"status":"in_progress"})
    elif meth=="session/cancel" and turn[0] is not None: send({"jsonrpc":"2.0","id":turn[0],"result":{"stopReason":"cancelled"}})
`
// A scripted agent that asks before editing a.txt (host/tests/fake_agent.py).
const askAgent = join(resolve(__dirname, '../../..'), 'host/tests/fake_agent.py')
// An orb's canvas (the avatars draw on a canvas too, without an ink color).
const ORB = 'canvas[style*="color"]'

/** What an orb's canvas shows: its CSS size, ink color, a fingerprint of its pixels, whether it has ink, and whether
 * its corners are empty (a disc, not a square of dots). */
const orb = (canvas: Locator) =>
  canvas.evaluate((el: HTMLCanvasElement) => {
    const data = el.getContext('2d')!.getImageData(0, 0, el.width, el.height).data
    const alpha = (x: number, y: number) => data[(y * el.width + x) * 4 + 3]!
    const c = Math.max(1, Math.floor(el.width / 8))
    let ink = 0, print = 0
    for (let i = 3; i < data.length; i += 4) { if (data[i]! > 0) ink++; print = (print * 31 + data[i]!) | 0 }
    let corners = 0
    for (let y = 0; y < c; y++) for (let x = 0; x < c; x++)
      corners += alpha(x, y) + alpha(el.width - 1 - x, y) + alpha(x, el.height - 1 - y) + alpha(el.width - 1 - x, el.height - 1 - y)
    return { size: el.getBoundingClientRect().width, color: getComputedStyle(el).color, print, ink, corners }
  })

/** True when the orb's picture changes within a second (it animates). */
async function animates(canvas: Locator) {
  const first = (await orb(canvas)).print
  for (let i = 0; i < 10; i++) {
    await canvas.page().waitForTimeout(100)
    if ((await orb(canvas)).print !== first) return true
  }
  return false
}

const cssColor = (page: Page, value: string) =>
  page.evaluate((value) => {
    const probe = document.createElement('span')
    probe.style.color = value
    document.body.append(probe)
    const color = getComputedStyle(probe).color
    probe.remove()
    return color
  }, value)

test('thinking orbs are halftone discs that animate, and hold still with Reduce Motion', async () => {
  const { browser, page } = await attach()
  const ids: string[] = []
  const script = test.info().outputPath('busy_agent.py')
  writeFileSync(script, busyAgent)
  try {
    await setPref(page, 'showThinkingDetails', true)
    const busy = `Orb busy ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: busy, backend: 'custom', command: fixtureCommand(script), connectors: [], skills: [], notify: false,
    })
    ids.push(bot.id)
    const waiting = `Orb waiting ${Date.now()}`
    const { bot: asker } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name: waiting, backend: 'custom', command: fixtureCommand(askAgent), connectors: [], skills: [], notify: false, permission: 'ask',
    })
    ids.push(asker.id)
    await api(page, 'send', { botId: bot.id, text: 'go' })
    await api(page, 'send', { botId: asker.id, text: 'go' })

    // The working bot's row: a 13 pt disc in the secondary ink, moving.
    const busyRow = page.locator('.roster-row', { hasText: busy })
    const rowOrb = busyRow.locator(ORB)
    await expect(rowOrb).toBeVisible()
    const row = await orb(rowOrb)
    expect(row.size).toBe(13)
    expect(row.ink).toBeGreaterThan(0)
    expect(row.corners).toBe(0)
    expect(row.color).toBe(await cssColor(page, 'var(--secondary)'))
    expect(await animates(rowOrb)).toBe(true)

    // The needs-you row: a 16 pt listening disc in amber, moving.
    const waitRow = page.locator('.roster-row', { hasText: waiting })
    await expect(waitRow).toContainText('Needs approval')
    const amber = await orb(waitRow.locator(ORB))
    expect(amber.size).toBe(16)
    expect(amber.ink).toBeGreaterThan(0)
    expect(amber.corners).toBe(0)
    expect(amber.color).toBe(await cssColor(page, 'var(--warning)'))
    expect(await animates(waitRow.locator(ORB))).toBe(true)

    // The chat's working indicator, and its steps: search, fetch and edit each with their own moving orb.
    await busyRow.click()
    const indicator = page.locator('.working').first()
    await expect(indicator.locator(ORB).first()).toBeVisible()
    expect((await orb(indicator.locator(ORB).first())).size).toBe(16)
    expect(await animates(indicator.locator(ORB).first())).toBe(true)
    await page.locator('.working-toggle').click()
    const steps = page.locator('.working-steps')
    for (const title of ['Search the docs', 'Fetch the page', 'Edit a.txt']) {
      await expect(steps).toContainText(title)
      const canvas = steps.locator('*', { hasText: title }).filter({ has: page.locator(ORB) }).last().locator(ORB)
      const picture = await orb(canvas)
      expect(picture.size).toBe(16)
      expect(picture.ink).toBeGreaterThan(0)
      expect(picture.corners).toBe(0)
      expect(await animates(canvas)).toBe(true)
    }
    await page.screenshot({ path: test.info().outputPath('orbs-working.png') })

    // Reduce Motion: every orb holds its still pose.
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.waitForTimeout(100)
    expect(await animates(indicator.locator(ORB).first())).toBe(false)
    expect(await animates(rowOrb)).toBe(false)
    expect(await animates(waitRow.locator(ORB))).toBe(false)
    const still = await orb(rowOrb)
    expect(still.ink).toBeGreaterThan(0)
    expect(still.corners).toBe(0)
    await page.screenshot({ path: test.info().outputPath('orbs-still.png') })
  } finally {
    await page.emulateMedia({ reducedMotion: null }).catch(() => {})
    await setPref(page, 'showThinkingDetails', false).catch(() => {})
    for (const botId of ids) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
