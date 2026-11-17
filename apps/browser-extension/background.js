const NATIVE_HOSTS = ['codync.browser', 'codync.browser.dev']
const EXTENSION_ID = 'dgejialhpalhoigbkmgeabgpphlipiek'
const PROTOCOL = 1
const SESSION_KEY = 'codyncBrowserSession'
const MAX_TEXT = 16 * 1024
const MAX_URL = 8 * 1024
const attachTimeout = 10_000
const navigateTimeout = 20_000
const settleTimeout = 200
const PERMANENT_BRIDGE_ERROR =
  /(?:Opened Codync Browser in its dedicated profile|belongs to the other Codync variant)/i

const connections = new Map()
const reconnects = new Map()
const reconnectSuppressed = new Set()
const activeRequests = new Map()
const completedStarts = new Map()
const tabOperationQueues = new Map()
const attachedTabs = new Set()
const sessions = new Map()
let storageUpdateQueue = Promise.resolve()

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

function describeError(error) {
  return String(chrome.runtime.lastError?.message ?? error?.message ?? error ?? 'Unknown Chrome error')
}

function isMissingTabError(error) {
  return /No tab with id|Invalid tab ID|Tab not found/i.test(describeError(error))
}

function isPermanentBridgeError(detail) {
  return PERMANENT_BRIDGE_ERROR.test(detail)
}

async function removeTab(tabId) {
  try {
    await chrome.tabs.remove(tabId)
  } catch (error) {
    if (!isMissingTabError(error)) throw new Error(`Could not close the dedicated browser tab: ${describeError(error)}`)
  }
}

function checkedUrl(value) {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_URL) {
    throw new Error(`URL must be 1-${MAX_URL} bytes`)
  }
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('Only http and https pages can be opened')
  if (url.username || url.password) throw new Error('URLs with credentials are not allowed')
  return url.toString()
}

function targetFrom(params, connection, requireTab = true) {
  const target = params?.target
  if (!target || typeof target !== 'object') throw new Error('Missing browser target')
  const required = ['hostId', 'botId', 'messageId', 'operationId', 'browserId', 'generation']
  if (requireTab) required.push('tabId')
  for (const key of required) {
    if (typeof target[key] !== 'string' || target[key].length === 0) throw new Error(`Missing browser target ${key}`)
  }
  if (!connection.hostId || target.hostId !== connection.hostId) {
    throw new Error('Browser target belongs to another Codync host')
  }
  return target
}

function sessionFor(target) {
  const session = sessions.get(target.browserId)
  if (!session) {
    const error = new Error('Browser operation is no longer running in Codync Browser')
    error.code = 'browser_session_closed'
    throw error
  }
  for (const key of ['hostId', 'botId', 'messageId', 'operationId']) {
    if (session[key] !== target[key]) throw new Error('Browser target changed')
  }
  if (session.tabId !== String(target.tabId)) throw new Error('Browser tab changed')
  if (session.generation !== target.generation) throw new Error('Browser target changed')
  return session
}

async function persistSessions() {
  await chrome.storage.session.set({ [SESSION_KEY]: [...sessions.values()] })
}

async function restoreSessions() {
  const stored = await chrome.storage.session.get(SESSION_KEY)
  for (const session of stored[SESSION_KEY] ?? []) sessions.set(session.browserId, session)
}

function queueStorageUpdate(update) {
  const pending = storageUpdateQueue.then(update, update)
  storageUpdateQueue = pending.catch(() => {})
  return pending
}

async function setBridgeStatus(status, detail = '', id = null) {
  await queueStorageUpdate(async () => {
    const stored = await chrome.storage.local.get(['bridgeConnections'])
    await chrome.storage.local.set({
      bridgeStatus: { status, detail, hostId: id, at: Date.now() },
      bridgeConnections: stored.bridgeConnections ?? {}
    })
  })
}

async function setConnectionStatus(hostName, status, detail = '', id = null) {
  await queueStorageUpdate(async () => {
    const stored = await chrome.storage.local.get(['bridgeConnections'])
    const bridgeConnections = { ...(stored.bridgeConnections ?? {}) }
    bridgeConnections[hostName] = { status, detail, hostId: id, at: Date.now() }
    const entries = Object.values(bridgeConnections)
    const connected = entries.find((entry) => entry.status === 'connected')
    const waiting = entries.find((entry) => entry.status === 'waiting')
    const error = entries.find((entry) => entry.status === 'error')
    const bridgeStatus = connected
      ? { status: 'connected', detail: '', hostId: connected.hostId ?? null, at: Date.now() }
      : waiting
        ? { status: 'waiting', detail: waiting.detail ?? '', hostId: null, at: Date.now() }
        : error
          ? { status: 'error', detail: error.detail ?? '', hostId: null, at: Date.now() }
          : { status: 'disconnected', detail, hostId: null, at: Date.now() }
    await chrome.storage.local.set({ bridgeStatus, bridgeConnections })
  })
}

function connectNative(hostName) {
  if (connections.get(hostName)?.port) return
  void setConnectionStatus(hostName, 'connecting')
  let port
  try {
    port = chrome.runtime.connectNative(hostName)
    connections.set(hostName, { name: hostName, port, hostId: null })
  } catch (error) {
    void setConnectionStatus(hostName, 'disconnected', String(error?.message ?? error))
    scheduleReconnect(hostName)
    return
  }
  port.onMessage.addListener((message) => {
    if (!message || typeof message !== 'object') return
    if (message.type === 'ready') {
      const connection = connections.get(hostName)
      if (!connection || connection.port !== port) return
      connection.hostId = String(message.hostId)
      void setConnectionStatus(hostName, 'connected', '', connection.hostId)
      return
    }
    if (message.type === 'bridgeError') {
      const detail = String(message.error ?? 'Browser bridge error')
      const connection = connections.get(hostName)
      if (connection?.port === port && isPermanentBridgeError(detail)) {
        connection.stopReconnect = true
        connection.stopReason = detail
        reconnectSuppressed.add(hostName)
      }
      void setConnectionStatus(hostName, 'error', detail)
      return
    }
    if (message.type === 'queued') {
      void setConnectionStatus(
        hostName,
        'waiting',
        String(message.detail ?? 'Another browser window is connected')
      )
      return
    }
    if (typeof message.id !== 'string' || typeof message.method !== 'string') return
    const connection = connections.get(hostName)
    if (connection?.port === port && connection.hostId) void runRequest(message, connection)
  })
  port.onDisconnect.addListener(() => {
    const detail = chrome.runtime.lastError?.message ?? 'Codync Browser bridge disconnected'
    const connection = connections.get(hostName)
    if (connection?.port !== port) return
    const stopReconnect = connection.stopReconnect === true
    const disconnectDetail = stopReconnect ? connection.stopReason : detail
    connections.delete(hostName)
    for (const request of activeRequests.values()) {
      if (request.connection?.port === port) request.cancelled = true
    }
    void closeSessionsForHost(hostName).catch((error) =>
      setBridgeStatus('error', `Could not close browser tabs: ${describeError(error)}`)
    )
    if (stopReconnect) {
      void setConnectionStatus(hostName, 'error', disconnectDetail)
    } else {
      void setConnectionStatus(hostName, 'disconnected', detail)
      scheduleReconnect(hostName)
    }
  })
  port.postMessage({
    type: 'hello',
    protocol: PROTOCOL,
    extensionId: EXTENSION_ID,
    version: chrome.runtime.getManifest().version,
    browserVersion: navigator.userAgent
  })
}

function scheduleReconnect(hostName) {
  if (reconnectSuppressed.has(hostName) || reconnects.has(hostName)) return
  reconnects.set(
    hostName,
    setTimeout(() => {
      reconnects.delete(hostName)
      connectNative(hostName)
    }, 2000)
  )
}

function reply(connection, message, result) {
  if (connections.get(connection.name)?.port !== connection.port) return
  connection.port.postMessage({ id: message.id, ok: true, result })
}

function fail(connection, message, error) {
  if (connections.get(connection.name)?.port !== connection.port) return
  const reply = {
    id: message.id,
    ok: false,
    error: String(error?.message ?? error ?? 'Codync Browser request failed')
  }
  if (typeof error?.code === 'string') reply.code = error.code
  connection.port.postMessage(reply)
}

async function runRequest(message, connection) {
  const state = { cancelled: false, connection }
  activeRequests.set(message.id, state)
  let completedTarget = null
  try {
    const result = await handle(message.method, message.params ?? {}, connection, state)
    const startTarget =
      message.method === 'start' && message.params?.target && result?.tabId
        ? {
            ...message.params.target,
            tabId: String(result.tabId)
          }
        : null
    if (state.cancelled) {
      if (startTarget?.tabId) {
        try {
          await stop({ target: startTarget }, connection)
        } catch (stopError) {
          if (!/no longer running/i.test(String(stopError?.message ?? stopError))) throw stopError
        }
      }
      throw new Error('Codync Browser request was cancelled')
    }
    if (startTarget?.tabId) completedTarget = startTarget
    reply(connection, message, result)
  } catch (error) {
    fail(connection, message, error)
  } finally {
    activeRequests.delete(message.id)
    if (completedTarget) {
      completedStarts.set(message.id, { target: completedTarget, connection })
      setTimeout(() => completedStarts.delete(message.id), 60_000)
    }
  }
}

function ensureRequestActive(state) {
  if (state?.cancelled) throw new Error('Codync Browser request was cancelled')
}

async function runInTabOrder(tabId, requestState, operation) {
  const previous = tabOperationQueues.get(tabId) ?? Promise.resolve()
  let release
  const current = new Promise((resolve) => {
    release = resolve
  })
  tabOperationQueues.set(tabId, current)
  try {
    await previous
    ensureRequestActive(requestState)
    return await operation()
  } finally {
    release()
    if (tabOperationQueues.get(tabId) === current) tabOperationQueues.delete(tabId)
  }
}

async function closeSessionsForHost(hostName) {
  const stale = []
  for (const [browserId, session] of sessions) {
    if (session.hostName === hostName || !session.hostName) {
      stale.push(session)
      sessions.delete(browserId)
    }
  }
  if (stale.length === 0) return

  await persistSessions()
  const errors = []
  for (const session of stale) {
    const tabId = Number(session.tabId)
    try {
      await detach(tabId)
      await removeTab(tabId)
    } catch (error) {
      errors.push(`${session.browserId}: ${describeError(error)}`)
    }
  }
  if (errors.length > 0) {
    await setBridgeStatus('error', `Could not close browser tabs after Codync disconnected: ${errors.join('; ')}`)
  }
}

async function waitForTab(tabId, predicate, timeoutMs, requestState = null) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (requestState) ensureRequestActive(requestState)
    let tab
    try {
      tab = await chrome.tabs.get(tabId)
    } catch (error) {
      throw new Error(`The browser tab closed while loading: ${describeError(error)}`)
    }
    if (predicate(tab)) return tab
    await sleep(100)
  }
  throw new Error('The tab did not finish loading in time')
}

async function attach(tabId) {
  if (attachedTabs.has(tabId)) return
  await withTimeout(
    chrome.debugger.attach({ tabId }, '1.3'),
    attachTimeout,
    'Could not attach to the browser tab'
  )
  attachedTabs.add(tabId)
  await chrome.debugger.sendCommand({ tabId }, 'Page.enable')
  await chrome.debugger.sendCommand({ tabId }, 'Runtime.enable')
}

async function detach(tabId) {
  if (!attachedTabs.delete(tabId)) return
  try {
    await chrome.debugger.detach({ tabId })
  } catch (error) {
    if (!isMissingTabError(error)) {
      await setBridgeStatus('error', `Could not release the browser tab: ${describeError(error)}`)
    }
  }
}

function withTimeout(promise, timeoutMs, message) {
  return Promise.race([
    promise,
    sleep(timeoutMs).then(() => {
      throw new Error(`${message} in ${Math.round(timeoutMs / 1000)} seconds`)
    })
  ])
}

async function evaluate(tabId, expression) {
  const answer = await chrome.debugger.sendCommand(
    { tabId },
    'Runtime.evaluate',
    { expression, returnByValue: true, awaitPromise: true }
  )
  if (answer.exceptionDetails) {
    throw new Error(answer.exceptionDetails.exception?.description ?? 'The page script failed')
  }
  return answer.result?.value
}

async function pageState(tabId) {
  const tab = await chrome.tabs.get(tabId)
  const value = await evaluate(tabId, `(() => ({
    url: location.href,
    title: document.title,
    text: (document.body?.innerText ?? '').slice(0, 12000)
  }))()`)
  return {
    url: String(value?.url || tab.url || ''),
    title: String(value?.title || tab.title || ''),
    text: String(value?.text || '')
  }
}

async function syncSessionUrl(session, nextState) {
  if (nextState?.url && nextState.url !== session.currentUrl) {
    session.currentUrl = nextState.url
    await persistSessions()
  }
  return nextState
}

async function start(params, connection, state) {
  const target = targetFrom(params, connection, false)
  const url = checkedUrl(params.url)
  let tabId = null
  try {
    ensureRequestActive(state)
    const tab = await chrome.tabs.create({ url, active: true })
    if (!Number.isInteger(tab.id)) throw new Error('Chrome did not return a tab id')
    tabId = tab.id
    ensureRequestActive(state)
    await waitForTab(
      tabId,
      (current) => current.status === 'complete' && /^https?:/i.test(current.url ?? ''),
      navigateTimeout,
      state
    )
    ensureRequestActive(state)
    await attach(tabId)
    ensureRequestActive(state)
    const initialState = await pageState(tabId)
    ensureRequestActive(state)
    if (!/^https?:/i.test(initialState.url)) {
      throw new Error('The browser did not open an HTTP or HTTPS page')
    }
    const session = {
      hostId: target.hostId,
      hostName: connection.name,
      botId: target.botId,
      messageId: target.messageId,
      operationId: target.operationId,
      browserId: target.browserId,
      tabId: String(tabId),
      generation: target.generation,
      initialUrl: url,
      currentUrl: initialState.url || url
    }
    sessions.set(target.browserId, session)
    await persistSessions()
    ensureRequestActive(state)
    return {
      tabId: String(tabId),
      browserVersion: navigator.userAgent,
      browserExecutable: 'unknown',
      extensionId: EXTENSION_ID,
      extensionVersion: chrome.runtime.getManifest().version,
      state: initialState
    }
  } catch (error) {
    if (tabId != null) {
      try {
        await detach(tabId)
        await removeTab(tabId)
      } catch (cleanupError) {
        await setBridgeStatus('error', `Could not clean up a failed browser start: ${describeError(cleanupError)}`)
      }
    }
    throw error
  }
}

async function navigate(params, connection, state) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  const url = checkedUrl(params.url)
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, state, async () => {
    ensureRequestActive(state)
    sessionFor(target)
    const beforeTab = await chrome.tabs.get(tabId)
    const previousUrl = beforeTab.url || session.currentUrl
    const updated = await chrome.tabs.update(tabId, { url })
    ensureRequestActive(state)
    if (updated.status !== 'loading' && updated.url === previousUrl) {
      await waitForTab(
        tabId,
        (current) => current.status === 'loading' || current.url !== previousUrl,
        navigateTimeout,
        state
      )
      ensureRequestActive(state)
    }
    const tab = await waitForTab(
      tabId,
      (current) => current.status === 'complete' && /^https?:/i.test(current.url ?? ''),
      navigateTimeout,
      state
    )
    ensureRequestActive(state)
    session.currentUrl = tab.url ?? url
    await persistSessions()
    return syncSessionUrl(session, await pageState(tabId))
  })
}

async function click(params, connection, state) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  if (typeof params.selector !== 'string' || params.selector.length === 0 || params.selector.length > 512) {
    throw new Error('Selector must be 1-512 bytes')
  }
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, state, async () => {
    ensureRequestActive(state)
    sessionFor(target)
    await attach(tabId)
    ensureRequestActive(state)
    const selector = JSON.stringify(params.selector)
    const hit = await evaluate(tabId, `(() => {
      const matches = document.querySelectorAll(${selector});
      if (matches.length === 0) return { found: false, reason: 'selector did not match an element' };
      if (matches.length > 1) return { found: false, reason: 'selector matched more than one element' };
      const el = matches[0];
      el.scrollIntoView({ block: 'center', inline: 'center' });
      const rect = el.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) {
        return { found: false, reason: 'element has a 0x0 hit box' };
      }
      const x = rect.left + rect.width / 2;
      const y = rect.top + rect.height / 2;
      const hit = document.elementFromPoint(x, y);
      if (!hit || !(hit === el || el.contains(hit))) {
        return { found: false, reason: 'element is covered or cannot receive the click' };
      }
      return {
        found: true,
        x,
        y
      };
    })()`)
    ensureRequestActive(state)
    if (!hit?.found) throw new Error(hit?.reason ?? 'Element is not clickable')
    const point = { x: hit.x, y: hit.y }
    await chrome.debugger.sendCommand({ tabId }, 'Input.dispatchMouseEvent', {
      type: 'mouseMoved',
      ...point
    })
    ensureRequestActive(state)
    await chrome.debugger.sendCommand({ tabId }, 'Input.dispatchMouseEvent', {
      type: 'mousePressed',
      ...point,
      button: 'left',
      buttons: 1,
      clickCount: 1
    })
    await chrome.debugger.sendCommand({ tabId }, 'Input.dispatchMouseEvent', {
      type: 'mouseReleased',
      ...point,
      button: 'left',
      buttons: 0,
      clickCount: 1
    })
    await sleep(settleTimeout)
    return syncSessionUrl(session, await pageState(tabId))
  })
}

async function type(params, connection, state) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  if (typeof params.selector !== 'string' || params.selector.length === 0 || params.selector.length > 512) {
    throw new Error('Selector must be 1-512 bytes')
  }
  if (typeof params.text !== 'string' || params.text.length > MAX_TEXT) {
    throw new Error(`Text must be at most ${MAX_TEXT} bytes`)
  }
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, state, async () => {
    ensureRequestActive(state)
    sessionFor(target)
    await attach(tabId)
    ensureRequestActive(state)
    const selector = JSON.stringify(params.selector)
    const editable = await evaluate(tabId, `(() => {
      const matches = document.querySelectorAll(${selector});
      if (matches.length === 0) return { error: 'selector did not match an element' };
      if (matches.length > 1) return { error: 'selector matched more than one element' };
      const el = matches[0];
      if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el.isContentEditable)) return false;
      el.focus();
      el.scrollIntoView({ block: 'center', inline: 'center' });
      return document.activeElement === el || el.contains(document.activeElement)
        ? true
        : { error: 'selector did not match a focusable editable element' };
    })()`)
    ensureRequestActive(state)
    if (editable !== true) throw new Error(editable?.error ?? 'Selector did not match a focusable editable element')
    await chrome.debugger.sendCommand({ tabId }, 'Input.dispatchKeyEvent', {
      type: 'rawKeyDown',
      modifiers: selectAllModifier(),
      windowsVirtualKeyCode: 65,
      nativeVirtualKeyCode: 65,
      code: 'KeyA',
      key: 'a'
    })
    await chrome.debugger.sendCommand({ tabId }, 'Input.dispatchKeyEvent', {
      type: 'keyUp',
      modifiers: selectAllModifier(),
      windowsVirtualKeyCode: 65,
      nativeVirtualKeyCode: 65,
      code: 'KeyA',
      key: 'a'
    })
    ensureRequestActive(state)
    await chrome.debugger.sendCommand({ tabId }, 'Input.insertText', { text: params.text })
    await sleep(settleTimeout)
    return pageState(tabId)
  })
}

function selectAllModifier() {
  return /Mac/i.test(navigator.platform) ? 4 : 2
}

async function screenshot(params, connection, state) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, state, async () => {
    ensureRequestActive(state)
    sessionFor(target)
    await attach(tabId)
    ensureRequestActive(state)
    const capture = await chrome.debugger.sendCommand({ tabId }, 'Page.captureScreenshot', {
      format: 'png',
      fromSurface: true,
      captureBeyondViewport: false
    })
    ensureRequestActive(state)
    if (!capture?.data) throw new Error('Chrome returned no screenshot data')
    return { data: capture.data, state: await pageState(tabId) }
  })
}

async function stop(params, connection, state) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, state, async () => {
    await detach(tabId)
    await removeTab(tabId)
    sessions.delete(target.browserId)
    await persistSessions()
    return { closed: true, tabId: session.tabId }
  })
}

async function state(params, connection, requestState) {
  const target = targetFrom(params, connection)
  const session = sessionFor(target)
  const tabId = Number(session.tabId)
  return runInTabOrder(tabId, requestState, async () => {
    sessionFor(target)
    return syncSessionUrl(session, await pageState(tabId))
  })
}

async function handle(method, params, connection, requestState) {
  switch (method) {
    case 'start':
      return start(params, connection, requestState)
    case 'navigate':
      return navigate(params, connection, requestState)
    case 'click':
      return click(params, connection, requestState)
    case 'type':
      return type(params, connection, requestState)
    case 'screenshot':
      return screenshot(params, connection, requestState)
    case 'stop':
      return stop(params, connection, requestState)
    case 'state':
      return state(params, connection, requestState)
    case 'cancel': {
      const request = activeRequests.get(params.requestId)
      if (request) {
        request.cancelled = true
        return { cancelled: true }
      }
      const completed = completedStarts.get(params.requestId)
      if (completed) {
        completedStarts.delete(params.requestId)
        try {
          await stop({ target: completed.target }, completed.connection)
        } catch (error) {
          if (!/no longer running/i.test(String(error?.message ?? error))) throw error
        }
        return { cancelled: true }
      }
      return { cancelled: false }
    }
    default:
      throw new Error(`Unknown Codync Browser method ${method}`)
  }
}

chrome.tabs.onRemoved.addListener((tabId) => {
  let changed = false
  for (const [browserId, session] of sessions) {
    if (session.tabId !== String(tabId)) continue
    sessions.delete(browserId)
    attachedTabs.delete(tabId)
    changed = true
  }
  if (changed) void persistSessions()
})

chrome.debugger.onDetach.addListener((source) => {
  if (!Number.isInteger(source.tabId)) return
  attachedTabs.delete(source.tabId)
  let changed = false
  for (const [browserId, session] of sessions) {
    if (session.tabId !== String(source.tabId)) continue
    sessions.delete(browserId)
    changed = true
  }
  if (changed) void persistSessions()
})

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== 'status') return false
  void chrome.storage.local.get(['bridgeStatus']).then(({ bridgeStatus }) => {
    sendResponse({
      bridgeStatus: bridgeStatus ?? { status: connections.size ? 'connecting' : 'disconnected' },
      extensionVersion: chrome.runtime.getManifest().version,
      sessions: sessions.size,
      connections: [...connections.entries()].map(([hostName, connection]) => ({
        hostName,
        hostId: connection.hostId
      }))
    })
  })
  return true
})

chrome.runtime.onStartup.addListener(() => NATIVE_HOSTS.forEach(connectNative))
chrome.runtime.onInstalled.addListener(() => NATIVE_HOSTS.forEach(connectNative))

void restoreSessions().finally(() => NATIVE_HOSTS.forEach(connectNative))
