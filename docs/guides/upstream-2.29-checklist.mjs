import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const check = (path, ...patterns) => {
  assert(existsSync(resolve(root, path)), `Missing ${path}`);
  const source = readFileSync(resolve(root, path), 'utf8');
  for (const pattern of patterns) assert(pattern.test(source), `${path}: missing ${pattern}`);
};
const host = 'host/src/';
check(`${host}api/apps_bridge.rs`, /fn request/);
check(`${host}api/bots.rs`, /"appRequest".*apps_bridge::request/);
check(`${host}api/mod.rs`, /mod apps_bridge/);
check(`${host}proactive.rs`, /proactive:/);
check(`${host}main.rs`, /mod proactive;/);
check(`${host}background.rs`, /proactive::run/);
check(`${host}api/apps_runtime.rs`, /pub\(crate\) fn dispatch_bot_turn/, /pub\(super\) fn handle_request/);
check(`${host}store/mod.rs`, /ProactiveSettings/);
for (const path of ['chat/apps.rs', 'store/apps.rs', 'chat/open_items.rs']) check(host + path);
const android = 'apps/android/app/src/main/kotlin/com/codync/android/';
const core = 'apps/android/core/src/main/kotlin/com/codync/android/core/';
check(`${core}MiniAppBridge.kt`);
check(`${android}MiniAppWebClient.kt`);
check(`${android}MiniAppBridgeScript.kt`);
check(`${android}AppStoreBots.kt`, /appRequest/);
check(`${android}BotAppSheet.kt`, /WebView/, /MiniAppWebClient/);
check(`${android}BotAppsCard.kt`, /DecisionActions/);
check(`${android}ProactivePolicyCard.kt`, /everyHours/, /quiet/);
check(`${core}BdotsModels.kt`, /collectPendingDecisions/, /allowOptionId/, /denyOptionId/, /takeoverRequest/);
check('apps/android/core/src/test/kotlin/com/codync/android/core/BdotsModelsTest.kt', /collectPendingDecisions/);
check(`${android}CodyncApp.kt`, /NewBotView/, /NewGroupView/);
check(`${android}NewBotView.kt`, /catalog|Catalog/);
check(`${android}PairingView.kt`, /Windows|windows/);
const desktop = 'apps/desktop/src/';
check(`${desktop}renderer/views/bots/ProactiveCheckIns.tsx`);
check(`${desktop}renderer/views/bots/BotEditorView.tsx`, /ProactiveCheckIns/);
check(`${desktop}renderer/views/bots/BotAppsView.tsx`, /Approve/, /Deny/);
check(`${desktop}renderer/components/ProactivePolicySection.tsx`);
check(`${desktop}shared/bdots-models.ts`, /collectPendingDecisions/);
check(`${desktop}shared/models.test.ts`, /collectPendingDecisions/);
const baseline = 'af33eda40df9eeef45720eb7fe8c8f3530608bf3';
const docs = git('ls-tree', '-rz', '--name-only', baseline, '--', 'docs/nuevo thndr/').split('\0').filter(Boolean);
assert(docs.length > 0, 'Missing original research documents');
for (const path of docs) {
  assert.equal(git('hash-object', '--', path), git('rev-parse', `${baseline}:${path}`), `Changed original document: ${path}`);
}
git('merge-base', '--is-ancestor', baseline, 'HEAD');
const moved = new Map([
  ['apps/android/core/src/main/kotlin/com/codync/android/core/CallTranscript.kt',
    'apps/android/core/src/main/kotlin/com/codync/android/core/CallReceiptTranscript.kt'],
  // Upstream's rail/panel Extensions window: the catalog is one panel now, per section.
  ['apps/desktop/src/renderer/views/marketplace/CatalogSections.tsx',
    'apps/desktop/src/renderer/views/marketplace/CatalogPanel.tsx'],
]);
const added = git('diff', '--name-only', '--diff-filter=A', '-z', 'e770dbd9', baseline).split('\0').filter(Boolean);
for (const path of added) assert(existsSync(resolve(root, moved.get(path) ?? path)), `Lost fork-added file: ${path}`);
console.log(`PASS: fork feature assertions; ${docs.length} research documents unchanged; ${added.length} fork-added files retained or explicitly moved; fork history retained`);
