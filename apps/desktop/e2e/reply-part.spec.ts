import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot, typeMessage } from './sandbox.ts'

type Entry = { kind: string; data: { text?: string; replyTo?: { text: string; part?: boolean } } }

/** Selects `part` inside the last bot reply that holds it, the way dragging over it would. */
const select = (page: Page, part: string) => page.evaluate((part) => {
  const bubble = [...document.querySelectorAll('.message.agent .bubble')].filter((b) => b.textContent?.includes(part)).at(-1)!
  const walker = document.createTreeWalker(bubble, NodeFilter.SHOW_TEXT)
  let node = walker.nextNode()
  while (node && !node.textContent!.includes(part)) node = walker.nextNode()
  const start = node!.textContent!.indexOf(part)
  const range = document.createRange()
  range.setStart(node!, start)
  range.setEnd(node!, start + part.length)
  const selection = window.getSelection()!
  selection.removeAllRanges()
  selection.addRange(range)
}, part)

test('Reply quotes the part of a message that is selected, and the whole message when nothing is', async () => {
  const { browser, page } = await attach()
  const name = `Reply part e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await api(page, 'send', { botId, text: 'alpha beta gamma' })
    await page.locator('.roster-row', { hasText: name }).click()
    const answer = page.locator('.message.agent', { hasText: 'alpha beta gamma' }).last()
    await expect(answer).toBeVisible({ timeout: 20_000 })
    const quote = page.locator('.composer-reply-quote')
    const replyFromFooter = async () => {
      await answer.hover()
      await answer.getByRole('button', { name: 'Reply' }).click()
    }
    const cancel = async () => {
      await page.locator('.composer-reply').getByRole('button', { name: 'Cancel reply' }).click()
      await expect(quote).toHaveCount(0)
    }

    // A part is selected: Reply quotes only that.
    await select(page, 'beta')
    await replyFromFooter()
    await expect(quote).toHaveText('“beta”')
    await typeMessage(page, 'why this one?')
    await expect(page.locator('.message.user .bubble-quote').last()).toHaveText('“beta”')
    // The agent is told which part the reply is about (the scripted agent answers with what it was sent).
    await expect(page.locator('.message.agent', { hasText: 'this replies to this part of your message: "beta"' })).toBeVisible({ timeout: 20_000 })
    const { entries } = await api<{ entries: Entry[] }>(page, 'history', { botId, limit: 10 })
    expect(entries.find((e) => e.kind === 'user' && e.data.text === 'why this one?')?.data.replyTo).toMatchObject({ text: 'beta', part: true })

    // The right-click menu's Reply quotes the part too: the press on the menu keeps what the right-click found.
    await select(page, 'gamma')
    await answer.locator('.bubble').click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Reply' }).click()
    await expect(quote).toHaveText('“gamma”')
    await cancel()

    // A selection that reaches into another message is no part of this one: the whole message.
    await page.evaluate(() => {
      const bubbles = [...document.querySelectorAll('.message .bubble')]
      const from = bubbles.findIndex((b) => b.closest('.message.agent') && b.textContent?.includes('alpha beta gamma'))
      window.getSelection()!.setBaseAndExtent(bubbles[from], 0, bubbles[from + 1], 1)
    })
    await replyFromFooter()
    await expect(quote).toContainText('mode')
    await cancel()

    // Nothing selected: the whole message, as before.
    await page.evaluate(() => window.getSelection()!.removeAllRanges())
    await replyFromFooter()
    // The quote shows the message's first line (the scripted agent opens with its mode).
    await expect(quote).toContainText('mode')
    await cancel()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('A bot message quotes the message it answers (reply_to), above its bubble', async () => {
  const { browser, page } = await attach()
  const name = `Bot quote e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    const { entry } = await api<{ entry: { id: string } }>(page, 'send', { botId, text: 'quote this one' })
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.message.agent', { hasText: 'quote this one' }).last()).toBeVisible({ timeout: 20_000 })
    // A long message in between pushes the quoted one out of sight.
    await api(page, 'chatCall', { botId, name: 'send_message', arguments: { text: Array.from({ length: 60 }, (_, i) => `Line ${i}.`).join('\n\n') } })
    // A late answer, as a background result sends it: the bot's agent is still connected.
    await api(page, 'chatCall', { botId, name: 'send_message', arguments: { text: 'About that one.', reply_to: entry.id } })
    const answer = page.locator('.message.agent', { hasText: 'About that one.' })
    const quote = answer.locator('.agent-quote')
    await expect(quote).toHaveText('“quote this one”')
    // Clicking the quote brings the quoted message into view.
    const original = page.locator(`[data-entry-id="${entry.id}"]`)
    await expect(original).not.toBeInViewport()
    await quote.click()
    await expect(original).toBeInViewport()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
