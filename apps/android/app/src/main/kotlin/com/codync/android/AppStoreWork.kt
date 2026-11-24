package com.codync.android

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.jsonObject

// The user's hand on a bot's box: order, cutting in, stopping, dropping and trying again. The host decides what runs.

/** A waiting task moves to the front. */
internal fun AppStore.workFirst(botId: String, taskId: String) = changeWork("workFirst", botId, taskId)

/** A waiting task, or one that ended, leaves the box. */
internal fun AppStore.dropWork(botId: String, taskId: String) = changeWork("dropWork", botId, taskId)

/** A working task's thread stops. */
internal fun AppStore.stopWork(botId: String, taskId: String) = changeWork("stopWork", botId, taskId)

/** A stopped or failed task goes back to waiting, last. */
internal fun AppStore.retryWork(botId: String, taskId: String) = changeWork("retryWork", botId, taskId)

/** Starts [taskId] now in the place of the working task [insteadOf], which returns to the front and starts again later. */
internal fun AppStore.workCutIn(botId: String, taskId: String, insteadOf: String) =
    command("workCutIn", body("botId" to botId, "taskId" to taskId, "insteadOf" to insteadOf), workKey(botId))

private fun AppStore.changeWork(method: String, botId: String, taskId: String) =
    command(method, body("botId" to botId, "taskId" to taskId), workKey(botId))

/** Busy while one of [botId]'s work changes is out. */
internal fun workKey(botId: String) = "work:$botId"

/** How long a started thread may take to arrive through sync with its thread before its bot's details open instead (iOS `threadStarted`). */
private const val THREAD_START_MILLIS = 10_000L

/**
 * Starts a thread with [botId] on [text], on the bot's own settings (`startThread`); [started] closes what it was written
 * in. Its task arrives through sync: once it has a thread, its bot's details open at that thread; one with none by then (it
 * waits behind others, or never came) opens its bot's details.
 */
internal fun AppStore.startThread(botId: String, text: String, started: () -> Unit) = action(startThreadKey(botId)) {
    val reply = ready().call("startThread", body("botId" to botId, "text" to text.trim())).jsonObject
    val owner = reply.text("botId") ?: botId
    val taskId = reply.text("taskId")
    started()
    val threaded = taskId != null && withTimeoutOrNull(THREAD_START_MILLIS) {
        state.first { now -> now.bots.any { bot -> bot.id == owner && bot.work.any { it.id == taskId && it.thread != null } } }
    } != null
    openDetails(owner, taskId.takeIf { threaded })
}

/** Busy while a thread with [botId] is being started. */
internal fun startThreadKey(botId: String) = "startThread:$botId"
