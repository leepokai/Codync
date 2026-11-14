import { strict as assert } from 'node:assert';
import type { Json } from './phone.ts';

type Call = (method: string, body?: unknown) => Promise<Json>;

/** Real host importer/encryption against Wrangler's R2 binding, over the encrypted phone channel. */
export async function exerciseIcons(call: Call, target: string): Promise<string> {
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="red"/></svg>';
  const uploaded = await call('setIcon', { target, data: Buffer.from(svg).toString('base64'), tinted: false });
  const png = uploaded.icons[target].dataUrl as string;
  assert.match(png, /^data:image\/png;base64,/);
  assert.equal(uploaded.icons[target].tinted, false);
  // A valid PNG with trailing padding exercises authenticated inbound chunking above 1 MiB.
  const padded = Buffer.concat([Buffer.from(png.split(',')[1], 'base64'), Buffer.alloc(1100 * 1024)]);
  const large = await call('setIcon', { target, data: padded.toString('base64') });
  assert.equal(large.icons[target].dataUrl, png);
  const tinted = await call('setIcon', { target, tinted: true });
  assert.equal(tinted.icons[target].dataUrl, png);
  assert.ok(tinted.iconsRevision > large.iconsRevision, "snapshot revisions advance for color-only changes");
  assert.equal(tinted.icons[target].tinted, true);
  await assert.rejects(call('setIcon', { target, data: Buffer.from('bad image').toString('base64') }));
  assert.equal((await call('hello')).icons[target].dataUrl, png, 'failed import preserves the local image');
  assert.equal((await call('setIcon', { target, remove: true })).icons[target], undefined);
  await call('setIcon', { target, data: Buffer.from(svg).toString('base64') });
  console.log('  ✓ icon upload, large encrypted request, tint, failed replacement, reset and re-upload');
  return png;
}
