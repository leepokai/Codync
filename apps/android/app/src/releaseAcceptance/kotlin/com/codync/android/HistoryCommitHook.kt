package com.codync.android

import java.util.Collections
import java.util.WeakHashMap

/**
 * Test seam for the history-paging race. Debug and the R8 acceptance build only.
 * The release source set has the no-op of the same name, so a release APK has no hook.
 * Keep this file the same as src/debug/.../HistoryCommitHook.kt.
 */
private val historyCommitHooks =
    Collections.synchronizedMap(WeakHashMap<AppStore, suspend (String) -> Unit>())

internal var AppStore.beforeHistoryCommit: (suspend (String) -> Unit)?
    get() = historyCommitHooks[this]
    set(value) {
        if (value == null) historyCommitHooks.remove(this) else historyCommitHooks[this] = value
    }

internal suspend fun AppStore.runBeforeHistoryCommit(botId: String) {
    beforeHistoryCommit?.invoke(botId)
}
