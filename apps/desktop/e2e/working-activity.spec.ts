import { test, expect, type Locator } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, setPref } from './sandbox.ts'

// A scripted agent whose turn runs one long command and stays on it, so the chat's working pill shows it
// as its activity (a tool call's title).
const longActivity = 'cd "/Users/example/.codync/bots/0bcd2c28-ac38-4db8-b932-4ec2da5be7a3/workspace/uploads/628f8428-a7bd-48b1-8cfc-806332468f68"'
const longAgent = `import json,sys
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
        upd({"sessionUpdate":"tool_call","toolCallId":"t0","title":${JSON.stringify(longActivity)},"kind":"execute","status":"in_progress"})
    elif meth=="session/cancel" and turn[0] is not None: send({"jsonrpc":"2.0","id":turn[0],"result":{"stopReason":"cancelled"}})
`

/** Whether the pill's activity line ends inside the pill, is ellipsized, and leaves nothing painted outside. */
const fits = (pill: Locator) =>
  pill.evaluate((el) => {
    const activity = el.querySelector('.working-status .truncate')!
    const text = activity.getBoundingClientRect()
    const box = el.getBoundingClientRect()
    return {
      inside: text.right <= box.right + 0.5,
      truncated: activity.scrollWidth > activity.clientWidth,
      contained: el.scrollWidth <= el.clientWidth,
    }
  })

test('a long activity truncates inside the working pill', async () => {
  const { browser, page } = await attach()
  const script = test.info().outputPath('long_agent.py')
  writeFileSync(script, longAgent)
  let botId: string | undefined
  try {
    await setPref(page, 'showThinkingDetails', false)
    const name = `Long activity e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(script), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'go' })

    const pill = page.locator('.working').first()
    await expect(pill).toContainText(longActivity)
    const inside = { inside: true, truncated: true, contained: true }
    expect(await fits(pill)).toEqual(inside)

    // With thinking details on, the status sits inside the toggle button: it must hold the same,
    // unfolded too, where the pill takes the whole column.
    await setPref(page, 'showThinkingDetails', true)
    await expect(pill.locator('.working-toggle')).toHaveCount(1)
    expect(await fits(pill)).toEqual(inside)
    await pill.locator('.working-toggle').click()
    await expect(pill).toHaveClass(/expanded/)
    expect(await fits(pill)).toEqual(inside)
    await page.screenshot({ path: test.info().outputPath('working-activity.png') })
  } finally {
    await setPref(page, 'showThinkingDetails', false).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
