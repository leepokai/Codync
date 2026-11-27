package com.codync.android

/** Release builds have no history-paging test hook. Debug and releaseAcceptance supply the seam. */
@Suppress("UNUSED_PARAMETER")
internal suspend fun AppStore.runBeforeHistoryCommit(botId: String) = Unit
