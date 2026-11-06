package com.codync.android

/**
 * Runs inside the mini-app page before its own scripts: sends `fetch` calls to the host through [MiniAppMessageBridge]
 * (the WebView cannot hand request bodies to `shouldInterceptRequest`) and replaces the `_live` WebSocket with a
 * poll of the app's collections, since the host's socket is on the computer's loopback.
 */
internal fun miniAppBridgeScript(): String = """
(() => {
  if (window.top !== window) return;
  const pending = new Map();
  let next = 0;
  const nativeFetch = window.fetch.bind(window);
  const nativeSocket = window.WebSocket;

  window.__codyncAnswer = (id, status, type, body) => {
    const call = pending.get(id);
    if (!call) return;
    pending.delete(id);
    const bytes = Uint8Array.from(atob(body), (c) => c.charCodeAt(0));
    try {
      call.resolve(new Response(status === 204 ? null : bytes, { status, headers: { "content-type": type } }));
    } catch (error) { call.reject(error); }
  };

  CodyncBridge.onmessage = ({ data }) => {
    try {
      const answer = JSON.parse(data);
      window.__codyncAnswer(answer.id, answer.status, answer.type, answer.body);
    } catch (error) { console.warn("Could not read a mini-app response", error); }
  };

${miniAppFetchBodyScript()}

  window.fetch = async (input, init) => {
    const request = input instanceof Request ? input : null;
    if (!request && Object.prototype.toString.call(input) === "[object Request]") {
      throw new TypeError("Codync mini-app fetch cannot relay a Request from another frame. Pass its URL and text body explicitly.");
    }
    const url = new URL(request ? request.url : String(input), location.href);
    if (url.origin !== location.origin) return nativeFetch(input, init);
    const method = String((init && init.method) ?? (request && request.method) ?? "GET").toUpperCase();
    const body = miniAppFetchBody(request, init);
    if ((method === "GET" || method === "HEAD") && init && init.body != null) {
      throw new TypeError("Request with GET/HEAD method cannot have a body.");
    }
    return new Promise((resolve, reject) => {
      const id = ++next;
      pending.set(id, { resolve, reject });
      try { CodyncBridge.postMessage(JSON.stringify({ id, method, path: url.pathname + url.search, body })); }
      catch (error) { pending.delete(id); reject(error); }
    });
  };

  window.WebSocket = function (address, protocols) {
    const socketUrl = new URL(address, location.href);
    const base = location.pathname.split("/").slice(0, 5).join("/") + "/";
    const localSocket = (socketUrl.protocol === "wss:" || socketUrl.protocol === "https:") &&
      socketUrl.host === location.host && socketUrl.pathname === base + "_live";
    if (!localSocket) return new nativeSocket(address, protocols);
    const socket = { readyState: 1, close() { stopped = true; }, send() {} };
    const seen = new Map();
    let stopped = false;
    let primed = false;
    const poll = async () => {
      if (stopped) return;
      try {
        const collections = (await (await window.fetch(base + "_data")).json()).collections || [];
        for (const { collection, count, updatedAt } of collections) {
          const stamp = count + ":" + updatedAt;
          if (primed && seen.get(collection) !== stamp && socket.onmessage) {
            socket.onmessage({ data: JSON.stringify({ type: "change", collection }) });
          }
          seen.set(collection, stamp);
        }
        primed = true;
      } catch {}
      setTimeout(poll, 3000);
    };
    setTimeout(() => { if (socket.onopen) socket.onopen({}); poll(); }, 0);
    return socket;
  };
})();
"""
