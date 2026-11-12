// Sets the host's "Move active bots to the top" switch straight through its API (HOST_PORT and HOST_TOKEN, passed with
// `-e`), the way another device does, for `auto-order.yaml`. `ENABLED` is the state to set; the first call keeps the
// state the host had in `output.wasAutoOrder`, and `RESTORE` puts it back when the flow completes.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (output.wasAutoOrder === undefined) output.wasAutoOrder = api('hello', {}).autoOrder === true
const enabled = typeof RESTORE !== 'undefined' ? output.wasAutoOrder : ENABLED === 'true'
api('setAutoOrder', { enabled })
