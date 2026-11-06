import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

function kotlinTemplate(name) {
  const source = readFileSync(new URL(`../../main/kotlin/com/codync/android/${name}.kt`, import.meta.url), 'utf8');
  return source.slice(source.indexOf('"""') + 3, source.lastIndexOf('"""')).replaceAll("${'$'}", '$');
}

const script = kotlinTemplate('MiniAppBridgeScript')
  .replace('${miniAppFetchBodyScript()}', kotlinTemplate('MiniAppFetchBodyScript'));

function page({ subframe = false } = {}) {
  const requests = [];
  const native = [];
  const sockets = [];
  const location = new URL('https://codync-app.invalid/apps/bot/tracker/owner/');
  const originalFetch = (...args) => { native.push(args); return Promise.resolve(new Response('external')); };
  const window = { fetch: originalFetch, WebSocket: class { constructor(...args) { sockets.push(args); } } };
  window.top = subframe ? {} : window;
  const bridge = {
    postMessage(payload) {
      const request = JSON.parse(payload);
      requests.push(request);
      queueMicrotask(() => bridge.onmessage({ data: JSON.stringify({
        id: request.id, status: 200, type: 'application/json', body: Buffer.from('{"ok":true}').toString('base64'),
      }) }));
    },
  };
  vm.runInNewContext(script, {
    window, location, CodyncBridge: bridge, Request, Response, URL, Uint8Array, atob, console, setTimeout: () => 0,
  });
  return { window, requests, native, sockets, originalFetch };
}

test('same-origin requests use structured messages and resolve the matching reply', async () => {
  const p = page();
  const answer = await p.window.fetch('_data/notes', { method: 'POST', body: '{"title":"Hello"}' });
  assert.deepEqual(await answer.json(), { ok: true });
  assert.deepEqual(p.requests, [{ id: 1, method: 'POST', path: '/apps/bot/tracker/owner/_data/notes', body: '{"title":"Hello"}' }]);
  assert.equal(p.native.length, 0);
});

test('subframes do not install the fetch bridge', () => {
  const p = page({ subframe: true });
  assert.equal(p.window.fetch, p.originalFetch);
  assert.equal(p.requests.length, 0);
});

test('external fetch and live sockets keep their native transport', async () => {
  const p = page();
  const body = new Blob(['binary']);
  assert.equal(await (await p.window.fetch('https://example.com/path', { method: 'POST', body })).text(), 'external');
  assert.equal(p.native[0][1].body, body);
  new p.window.WebSocket('wss://example.com/_live');
  assert.equal(p.sockets.length, 1);
  assert.equal(p.requests.length, 0);
});

test('the virtual app live socket uses channel polling without intercepting another app', () => {
  const p = page();
  const socket = new p.window.WebSocket('wss://codync-app.invalid/apps/bot/tracker/owner/_live');
  assert.equal(socket.readyState, 1);
  assert.equal(p.sockets.length, 0);
  socket.close();
  new p.window.WebSocket('wss://codync-app.invalid/apps/bot/other/owner/_live');
  assert.equal(p.sockets.length, 1);
});

test('nontext bodies reject clearly before sending any host request', async () => {
  const p = page();
  const form = new FormData();
  form.append('name', 'value');
  for (const body of [form, new Blob(['body']), new URLSearchParams({ name: 'value' }), new Uint8Array([1, 2]),
    new ArrayBuffer(2), new ReadableStream()]) {
    await assert.rejects(p.window.fetch('_data', { method: 'POST', body }), /supports string bodies only/);
  }
  assert.equal(p.requests.length, 0);
});

test('Request bodies are rejected rather than silently discarded', async () => {
  const p = page();
  for (const init of [undefined, {}, { body: null }, { body: undefined }]) {
    const request = new Request('https://codync-app.invalid/apps/bot/tracker/owner/_data', { method: 'POST', body: 'saved text' });
    await assert.rejects(p.window.fetch(request, init), /cannot relay a Request body/);
    assert.equal(request.bodyUsed, false);
  }
  assert.equal(p.requests.length, 0);
});

test('a Request from a different realm is rejected instead of stringified into a GET', async () => {
  const p = page();
  const foreign = {
    [Symbol.toStringTag]: 'Request',
    url: 'https://codync-app.invalid/apps/bot/tracker/owner/_data',
    method: 'POST',
    body: new ReadableStream(),
  };
  await assert.rejects(p.window.fetch(foreign), /cannot relay a Request from another frame/);
  assert.equal(p.requests.length, 0);
});

test('an explicit string body and method override the input Request', async () => {
  const p = page();
  const request = new Request('https://codync-app.invalid/apps/bot/tracker/owner/_data', { method: 'POST', body: 'old' });
  await p.window.fetch(request, { method: 'PUT', body: 'replacement' });
  assert.equal(p.requests[0].method, 'PUT');
  assert.equal(p.requests[0].body, 'replacement');
  await p.window.fetch(request, { body: '' });
  assert.equal(p.requests[1].body, '');
});

test('GET and HEAD cannot silently discard a provided body', async () => {
  const p = page();
  for (const method of ['GET', 'HEAD']) {
    await assert.rejects(p.window.fetch('_data', { method, body: '' }), /cannot have a body/);
    await assert.rejects(p.window.fetch('_data', { method, body: 'value' }), /cannot have a body/);
  }
  assert.equal(p.requests.length, 0);
});
