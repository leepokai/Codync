// Drives the running desktop app over CDP through the read-only paths: rail, Settings, Extensions and the palette.
// Usage: node tools/app-smoke.mjs [port]
const port = Number(process.argv[2] ?? 9397)

const pages = async () => (await fetch(`http://127.0.0.1:${port}/json`)).json()
const chatPage = async () => {
  const all = await pages()
  const chat = all.find((page) => page.type === 'page' && /window=chat/.test(page.url))
  if (!chat) throw new Error('No chat window; open the app with --remote-debugging-port')
  return chat
}

const evaluate = async (expression) => {
  const page = await chatPage()
  const socket = new WebSocket(page.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true })
    socket.addEventListener('error', reject, { once: true })
  })
  const answer = new Promise((resolve) => {
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data)
      if (message.id !== 1) return
      socket.close()
      resolve(message.result?.result?.value)
    })
  })
  socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }))
  return answer
}

const waitFor = async (expression, label, timeout = 6000) => {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    if (await evaluate(expression)) return
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  throw new Error(`Timed out waiting for ${label}`)
}

const results = []
const step = async (name, work) => {
  try {
    await work()
    results.push([name, true])
  } catch (error) {
    results.push([name, false, error instanceof Error ? error.message : String(error)])
  }
}

await step('the first-run guide can be dismissed', async () => {
  await evaluate(`(() => {
    const button = [...document.querySelectorAll('.modal-card button')].find((b) => /^(Not now|Ahora no)$/.test((b.textContent ?? '').trim()))
    if (button) { button.click(); return true }
    return false
  })()`)
})

await step('the roster is on screen', async () => {
  await waitFor("document.querySelectorAll('.app-rail-button').length > 0", 'the rail')
})

await step('Settings opens and closes', async () => {
  await evaluate(`(() => {
    const label = [...document.querySelectorAll('.app-rail-button')].map((b) => b.getAttribute('aria-label'))
    const name = label.find((l) => l === 'Ajustes' || l === 'Settings')
    if (!name) return false
    [...document.querySelectorAll('.app-rail-button')].find((b) => b.getAttribute('aria-label') === name).click()
    return true
  })()`)
  await waitFor("document.querySelector('.settings') !== null", 'the Settings page')
  await evaluate("document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); true")
  await waitFor("document.querySelector('.settings') === null", 'Settings to close')
})

await step('Extensions lists Channels', async () => {
  await evaluate(`(() => {
    const labels = [...document.querySelectorAll('.app-rail-button')].map((b) => b.getAttribute('aria-label'))
    const name = labels.find((l) => l === 'Extensiones' || l === 'Extensions')
    if (!name) return false
    [...document.querySelectorAll('.app-rail-button')].find((b) => b.getAttribute('aria-label') === name).click()
    return true
  })()`)
  await waitFor("[...document.querySelectorAll('.market-tab')].some((b) => /Canales|Channels/.test(b.textContent))", 'the Extensions tabs')
  await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.market-tab')].find((b) => /Canales|Channels/.test(b.textContent))
    tab.click()
    return true
  })()`)
  await waitFor("document.querySelectorAll('.channel-card').length >= 4", 'the four channel cards')
})

await step('Ctrl+K opens the command palette', async () => {
  await evaluate("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true })); true")
  await waitFor("document.querySelector('.modal-card .palette-search input, .modal-card input') !== null", 'the palette')
})

let failed = 0
for (const [name, ok, detail] of results) {
  if (!ok) failed += 1
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ` (${detail})` : ''}`)
}
console.log(failed === 0 ? `\n${results.length} UI checks passed.` : `\n${failed} of ${results.length} UI checks failed.`)
process.exit(failed === 0 ? 0 : 1)
