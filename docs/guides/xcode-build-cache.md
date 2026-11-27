# Xcode build cache

Every Apple build from the command line goes through `tools/xcodebuild.sh`. It takes `xcodebuild`'s own arguments and adds Xcode's compilation cache, so a new worktree, whose `build/dd` starts empty, reuses whatever any other checkout on the machine already compiled.

```sh
tools/xcodebuild.sh build -project apps/Codync.xcodeproj -scheme "iOS Dev" -configuration DevDebug \
  -derivedDataPath build/dd -destination 'generic/platform=iOS Simulator'
cd apps/ios/Kit && ../../../tools/xcodebuild.sh test -scheme CodyncKit-Package -derivedDataPath ../../../build/dd \
  -destination 'platform=iOS Simulator,id=<sim-id>'
```

## How it works

- **DerivedData stays per checkout.** `build/dd` holds intermediates, products and SwiftPM checkouts, and only one build may write it at a time, so each worktree keeps its own.
- **The CAS is shared.** `COMPILATION_CACHE_ENABLE_CACHING` stores each compile job's output in a content-addressed store keyed by a hash of all its inputs (sources, flags, compiler). Entries are only added, never changed, so any number of builds read and write it at once. It lives at `~/Library/Developer/Xcode/DerivedData/CompilationCache.noindex` (`CODYNC_CAS_PATH` overrides). It holds cache entries, not app builds, so it is the one thing of ours allowed in Xcode's DerivedData.
- **Prefix mapping makes keys path-independent.** Absolute paths are part of a key, and every worktree has its own. `*_ENABLE_PROJECT_PREFIX_MAPPING` maps `PROJECT_DIR`, `PROJECT_TEMP_DIR` and `BUILT_PRODUCTS_DIR`; the wrapper also maps the checkout's root to `/^wt`, which covers `build/dd/SourcePackages` (Clerk, SwiftTerm, …).
- **Not for release builds.** CI builds plainly, and the production Codync Screen build (`native-screen.mjs main`, which `dist:mac:main` and release CI run) uses plain `xcodebuild`, so shipped binaries keep their real paths.
- **No index store.** Command-line builds skip indexing (`COMPILER_INDEX_STORE_ENABLE=NO`); the Xcode IDE builds as before, without the cache.

## Limits

- Only compile jobs are cached. Linking, signing, asset catalogs, scripts and the SwiftPM fetch run every time, and files a branch changed (and what depends on them) compile again.
- The CAS is capped at 40 GB (`COMPILATION_CACHE_LIMIT_SIZE`, `CODYNC_CAS_LIMIT` overrides); Xcode prunes it after a build.
- Debug info holds the mapped paths (`/^wt/…`), so lldb in a cached build may need `settings set target.source-map /^wt <checkout>` to show sources.
- A checkout under `/tmp` misses part of the cache: Xcode writes `/private/tmp` as `/tmp`, which the root mapping doesn't match. Worktrees live in `.claude/worktrees/`.

## Checking it

A build log ends with `CompilationCacheMetrics` → `note: <hits> hits / <tasks> cacheable tasks (<n>%)`. Add `COMPILATION_CACHE_ENABLE_DIAGNOSTIC_REMARKS=YES` to see each job's hit or miss.

Measured on Xcode 27.0, `iOS Dev` / `DevDebug` for the simulator, a second worktree at the same commit with an empty `build/dd`: 950 of 950 jobs hit, 26 s instead of the first build's 90 s.

Suspect a stale result (changed code, unchanged behavior)? Build once with `CODYNC_CAS_PATH` pointing at an empty folder to rule the cache out, or delete the CAS folder.
