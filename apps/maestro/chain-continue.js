// Checks on the host (HOST_PORT, HOST_TOKEN, and the bots `chain-continue-setup.mjs` printed, passed with `-e`) that
// this phone's Continue sent its bot one "Continue" as the user and took the notice's button away.
const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/history`, {
  headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
  body: JSON.stringify({ botId: maestro.platform === 'ios' ? CHAIN_IOS : CHAIN_ANDROID, limit: 100 }),
})
if (res.status !== 200) throw new Error(`history: ${res.status} ${res.body}`)
const entries = json(res.body).entries
const sent = entries.filter((e) => e.kind === 'user' && e.data.text === 'Continue').length
const open = entries.filter((e) => e.data.continueChain === true).length
if (sent !== 1 || open !== 0) throw new Error(`Continue reached the bot ${sent} times; ${open} notices still offer it.`)
