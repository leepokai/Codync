#!/bin/sh
# xcodebuild with the machine-wide compilation cache, so a fresh worktree reuses what any other
# checkout already compiled: docs/guides/xcode-build-cache.md. Takes xcodebuild's own arguments.
set -eu

# The checkout this script belongs to, mapped to /^wt so cache keys don't depend on where it is.
root=$(cd "$(dirname "$0")/.." && pwd -P)
# Quoted for Xcode's list parsing, so a path with spaces stays one entry.
maps="\"$root=/^wt\""

exec xcodebuild "$@" \
  COMPILATION_CACHE_ENABLE_CACHING=YES \
  COMPILATION_CACHE_CAS_PATH="${CODYNC_CAS_PATH:-$HOME/Library/Developer/Xcode/DerivedData/CompilationCache.noindex}" \
  COMPILATION_CACHE_LIMIT_SIZE="${CODYNC_CAS_LIMIT:-40G}" \
  SWIFT_ENABLE_PREFIX_MAPPING=YES SWIFT_ENABLE_PROJECT_PREFIX_MAPPING=YES \
  CLANG_ENABLE_PREFIX_MAPPING=YES CLANG_ENABLE_PROJECT_PREFIX_MAPPING=YES \
  SWIFT_OTHER_PREFIX_MAPPINGS="$maps" \
  CLANG_OTHER_PREFIX_MAPPINGS="$maps" \
  COMPILER_INDEX_STORE_ENABLE=NO INDEX_ENABLE_DATA_STORE=NO
