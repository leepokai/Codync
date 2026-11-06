// Drives the running unpackaged desktop app through its remote debugging port, walking the
// marketplace tabs and saving screenshots. Usage: node tools/e2e-market.mjs [port]
import { writeFileSync } from 'node:fs'

const port = process.argv[2] ?? '9222'
let target
for (let i = 0; i < 120; i++) {
    try {
        const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
        target = pages.find((p) => p.type === 'page' && !p.url.startsWith('devtools'))
        if (target) break
    } catch {}
    await new Promise((r) => setTimeout(r, 1000))
}
if (!target) throw new Error('the app never exposed a page on the debugging port')

const ws = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((res, rej) => {
    ws.addEventListener('open', res)
    ws.addEventListener('error', rej)
})
let id = 0
const pending = new Map()
ws.addEventListener('message', (event) => {
    const msg = JSON.parse(event.data)
    if (msg.id && pending.has(msg.id)) {
        pending.get(msg.id)(msg)
        pending.delete(msg.id)
    }
})
const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
        const n = ++id
        pending.set(n, (msg) => (msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result)))
        ws.send(JSON.stringify({ id: n, method, params }))
    })

const evaluate = async (expression) => {
    const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description ?? ''))
    return r.result.value
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const waitFor = async (expression, label) => {
    for (let i = 0; i < 40; i++) {
        if (await evaluate(expression)) return
        await sleep(500)
    }
    throw new Error(`timed out waiting for ${label}`)
}
const shot = async (name) => {
    const { data } = await send('Page.captureScreenshot', { format: 'png' })
    writeFileSync(`build/e2e-${name}.png`, Buffer.from(data, 'base64'))
    console.log('screenshot', name)
}
const click = async (text, selector = 'button') =>
    evaluate(`(() => {
        const node = [...document.querySelectorAll(${JSON.stringify(selector)})]
            .find((n) => (n.innerText || n.getAttribute('aria-label') || '').toLowerCase().includes(${JSON.stringify(text.toLowerCase())}))
        if (!node) return false
        node.click()
        return true
    })()`)
const bodyText = () => evaluate('document.body.innerText')

await send('Page.enable')
await send('Runtime.enable')
await waitFor(`!!document.body`, 'the window')
// The lead bot or the startup state must be on screen before the sidebar is usable.
await sleep(2500)

if (!(await click('Extensions'))) throw new Error('no Extensions button in the sidebar')
await waitFor(`!!document.querySelector('.market-heading')`, 'the marketplace')
await shot('apps')

const tabs = await evaluate(`[...document.querySelectorAll('.market-tab')].map((t) => t.innerText)`)
if (!tabs.includes('Apps') || !tabs.includes('Agents') || !tabs.includes('Skills')) {
    throw new Error(`tabs are ${JSON.stringify(tabs)}`)
}

// Apps: search one, open its page, and check the setup and prompts are there.
await evaluate(`(() => { const i = document.querySelector('.market-search input'); i.value = 'linear'; i.dispatchEvent(new Event('input', { bubbles: true })) })()`)
await sleep(1200)
await click('Linear', '.market-row')
await waitFor(`document.body.innerText.includes('Copy link') === false && document.body.innerText.includes('Setup')`, 'the app page')
await shot('app-page')
const appPage = await bodyText()
if (!appPage.includes('Connect') || !appPage.includes('Try it')) throw new Error('the app page is missing its connect or prompts')

// Agents: switch, open the first agent, and check its skills, routines and information.
if (!(await click('Agents', '.market-tab'))) throw new Error('no Agents tab')
await sleep(800)
await click('Change Reviewer', '.market-row')
await waitFor(`document.body.innerText.includes('Information')`, 'the agent page')
await shot('agent-page')
const agentPage = await bodyText()
if (!agentPage.includes('Skills') || !agentPage.includes('Add agent')) throw new Error('the agent page is missing its skills or add action')

// Skills: switch and check the grouped catalog and the own-skill row.
await evaluate(`document.querySelector('.modal-header button[aria-label="Close"], .market-close button')?.click()`)
await sleep(600)
if (!(await click('Skills', '.market-tab'))) throw new Error('no Skills tab')
await sleep(1000)
await shot('skills')
const skillsPage = await bodyText()
if (!skillsPage.includes('Your own skill') || !skillsPage.includes('Coding')) throw new Error('the skills catalog is not grouped')

console.log('marketplace e2e passed')
ws.close()
