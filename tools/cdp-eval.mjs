// Evaluates an expression in the running app's page. Usage: node tools/cdp-eval.mjs <port> "<expression>"
const port = process.argv[2]
const expression = process.argv[3]
const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
const target = pages.find((p) => p.type === 'page')
const ws = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((res, rej) => { ws.addEventListener('open', res); ws.addEventListener('error', rej) })
ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }))
ws.addEventListener('message', (event) => {
  const msg = JSON.parse(event.data)
  if (msg.id !== 1) return
  console.log(JSON.stringify(msg.result?.result?.value ?? msg.result, null, 2))
  ws.close()
})
