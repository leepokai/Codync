// Fetching JSON from Discord and GitHub, and the small pages the member sees.

export async function json<T>(url: string, init: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(`${url} ${res.status}: ${await res.text()}`);
  return res.json<T>();
}

const escape = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

export function page(status: number, title: string, body: string, link?: { href: string; label: string }): Response {
  const html = `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Codync Discord</title>
<style>body{font:16px system-ui;display:grid;place-content:center;min-height:90vh;margin:0 16px;text-align:center;color:#111;background:#fff}
@media(prefers-color-scheme:dark){body{color:#eee;background:#111}}p{opacity:.7}
a{display:inline-block;margin-top:8px;padding:10px 20px;border-radius:8px;background:#5865f2;color:#fff;font-weight:600;text-decoration:none}</style>
<h1>${escape(title)}</h1><p>${escape(body)}</p>${link ? `<div><a href="${escape(link.href)}">${escape(link.label)}</a></div>` : ""}`;
  return new Response(html, { status, headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } });
}
