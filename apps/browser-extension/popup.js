const labels = {
  connected: ['Connected to Codync', 'Bots on this computer can use dedicated browser tabs.'],
  connecting: ['Connecting to Codync…', 'The Codync host must be running on this computer.'],
  waiting: ['Waiting for Codync', 'Another Chrome or Edge window is connected first. This window will take over when it closes.'],
  disconnected: ['Codync is not connected', 'Start Codync on this computer, then reopen this window.'],
  error: ['Codync Browser needs attention', 'Open the Codync desktop app and try again.']
}

chrome.runtime.sendMessage({ type: 'status' }, (response) => {
  if (chrome.runtime.lastError || !response) {
    show('error', chrome.runtime.lastError?.message ?? 'The extension did not answer')
    return
  }
  document.querySelector('#version').textContent = `v${response.extensionVersion}`
  document.querySelector('#sessions').textContent = String(response.sessions ?? 0)
  show(response.bridgeStatus?.status ?? 'disconnected', response.bridgeStatus?.detail ?? '')
})

function show(status, detail) {
  const [title, fallback] = labels[status] ?? labels.error
  const dot = document.querySelector('#dot')
  dot.className = `dot ${status}`
  document.querySelector('#title').textContent = title
  document.querySelector('#detail').textContent = detail || fallback
}
