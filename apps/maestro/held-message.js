// Asks the host (HOST_PORT, HOST_TOKEN, BOT_ID, passed with `-e`) what reached it for held-message-delivered.yaml: this
// phone's held message once, and nothing it took back.
const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/history`, {
  headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
  body: JSON.stringify({ botId: BOT_ID, limit: 100 }),
})
if (res.status !== 200) throw new Error(`history: ${res.status} ${res.body}`)
const count = (text) => json(res.body).entries.filter((entry) => entry.data.text === text).length
const held = count(`held ${maestro.platform}`)
const takenBack = count(`take back ${maestro.platform}`)
if (held !== 1 || takenBack !== 0) throw new Error(`The host has the held message ${held} times and the taken back one ${takenBack} times.`)
