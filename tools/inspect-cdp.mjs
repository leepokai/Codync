// Screenshot and text of the running app from its CDP port. Usage: node tools/inspect-cdp.mjs <port>
import { writeFileSync } from 'node:fs'

const port = process.argv[2] ?? '9333'
const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
const target = pages.find((p) => p.type === 'page')
if (!target) throw new Error('no page')
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
const send = (method, params = {}) => new Promise((resolve) => {
    const n = ++id
    pending.set(n, resolve)
    ws.send(JSON.stringify({ id: n, method, params }))
})
await send('Page.enable')
const text = await send('Runtime.evaluate', { expression: 'document.body.innerText', returnByValue: true })
console.log(text.result?.result?.value ?? '')
const shot = await send('Page.captureScreenshot', { format: 'png' })
writeFileSync(`build/inspect-${Date.now()}.png`, Buffer.from(shot.result.data, 'base64'))
console.log('screenshot saved')
ws.close()
