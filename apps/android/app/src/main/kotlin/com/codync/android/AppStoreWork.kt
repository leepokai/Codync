package com.codync.android

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withTimeoutOrNull
import com.codync.android.core.Login
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// The user's hand on a bot's box: order, cutting in, stopping, dropping and trying again. The host decides what runs.

/** A waiting task moves to the front. */
internal fun AppStore.workFirst(botId: String, taskId: String) = changeWork("workFirst", botId, taskId)

/** A waiting task, or one that ended, leaves the box. */
internal fun AppStore.dropWork(botId: String, taskId: String) {
    mutableState.update { it.copy(olderThreads = it.olderThreads?.drop(botId, taskId)) }
    changeWork("dropWork", botId, taskId)
}

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
 * Starts a thread on [text] (`startThread`): with [botId] on the bot's own settings, or with none on [setup]; [started]
 * closes what it was written in. Its task arrives through sync: once it has a thread, its thread chat opens over the
 * Threads tab ([openThread]); one with none by then (it waits behind others, or never came) opens its bot's details, or
 * nothing without a bot.
 */
internal fun AppStore.startThread(botId: String?, text: String, setup: JsonObject?, started: () -> Unit, openThread: (ThreadRef) -> Unit) =
    action(startThreadKey(botId)) {
        val request = buildJsonObject {
            botId?.let { put("botId", it) }
            put("text", text.trim())
            setup?.let { put("setup", it) }
        }
        val reply = ready().call("startThread", request).jsonObject
        val owner = reply.text("botId") ?: botId ?: return@action
        val taskId = reply.text("taskId")
        started()
        val threaded = taskId != null && withTimeoutOrNull(THREAD_START_MILLIS) {
            state.first { now -> now.bots.any { bot -> bot.id == owner && bot.work.any { it.id == taskId && it.thread != null } } }
        } != null
        // Sent without a bot: the box the reply names, which may not have synced in yet.
        when {
            threaded && taskId != null -> openThread(ThreadRef(state.value.computer.id, owner, taskId))
            botId != null -> openDetails(owner)
        }
    }

/** Busy while a thread with [botId] (null: without a bot) is being started. */
internal fun startThreadKey(botId: String?) = "startThread:${botId ?: ""}"

/**
 * What a thread without a bot runs on (desktop `ThreadStart`): [login]'s agent, in [home], on [model], asking before
 * anything it does in the agent's Auto mode, as a new bot does.
 */
internal fun plainSetup(login: Login, home: String, model: String?, options: Map<String, String>): JsonObject = buildJsonObject {
    put("backend", login.backend)
    put("loginProfile", login.loginProfile.orEmpty())
    if (home.isNotEmpty()) put("cwd", home)
    put("model", buildJsonObject {
        model?.let { put("id", it) }
        put("options", buildJsonObject { options.forEach { (key, value) -> put(key, value) } })
    })
    put("approval", buildJsonObject { put("permission", "ask"); put("mode", "auto") })
}
