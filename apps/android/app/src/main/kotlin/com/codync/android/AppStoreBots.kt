package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Bots and groups, their routines, usage, and the host queries the screens make.

fun AppStore.command(method: String, body: JsonObject = JsonObject(emptyMap()), key: String = method, onSuccess: () -> Unit = {}) = action(key) {
    val active = ready()
    val response = active.call(method, body)
    response.jsonObject["bot"]?.let { value ->
        withContext(Dispatchers.IO) { requireNotNull(database).receive(HostClient.decode(value.jsonObject)) }
    }
    response.jsonObject["entry"]?.let { value ->
        withContext(Dispatchers.IO) { requireNotNull(database).receive(listOf(Entry.decode(value.jsonObject))) }
    }
    refresh()
    onSuccess()
}

fun AppStore.saveBot(draft: BotDraft, onSaved: () -> Unit) = action("editor") {
    val active = ready()
    val result = active.call(if (draft.id == null) "createBot" else "updateBot", draft.body())
    val bot = HostClient.decode(result.jsonObject.getValue("bot").jsonObject)
    withContext(Dispatchers.IO) { requireNotNull(database).receive(bot) }
    refresh()
    openBot(bot.id)
    onSaved()
}

/** A saved bot's settings from its details, saved as they're edited: the chat stays where it is. */
fun AppStore.saveSettings(draft: BotDraft) = command("updateBot", draft.body(), "settings:" + draft.id)

fun AppStore.saveGroup(draft: GroupDraft, onSaved: () -> Unit) = action("editor") {
    val payload = draft.body(state.value.bots)
    val active = ready()
    val result = active.call(if (draft.id == null) "createBot" else "updateBot", payload)
    val bot = HostClient.decode(result.jsonObject.getValue("bot").jsonObject)
    withContext(Dispatchers.IO) { requireNotNull(database).receive(bot) }
    refresh()
    openBot(bot.id)
    onSaved()
}

/** `/fast`: a bot's own Fast mode, or a group's members' together (each member's own setting, so it holds in their chats too). */
fun AppStore.toggleFastMode(bot: Bot) {
    if (bot.kind != "group") return setFastMode(bot, !bot.fastMode)
    val group = GroupFastMode(bot.memberBots(state.value.bots))
    for (member in group.members) setFastMode(member, group.state == FastModeState.Off)
}
private fun AppStore.setFastMode(bot: Bot, on: Boolean) {
    command("updateBot", buildJsonObject { put("id", bot.id); put("fastMode", on) }, "roster:" + bot.id)
}

/** The bot's own agent again while it runs on a fallback: now if idle, else from its next turn. The host posts the chat line. */
fun AppStore.switchBack(bot: Bot) = command("switchBack", body("botId" to bot.id), "switchBack:" + bot.id)

fun AppStore.deleteBot(bot: Bot, onDeleted: () -> Unit) = action("delete:" + bot.id) {
    val active = ready()
    active.call("deleteBot", body("botId" to bot.id))
    mutableState.update { it.copy(selectedBot = it.selectedBot.takeUnless { id -> id == bot.id },
        // Its chat's draft and its artifacts' (`artifactDraftKey`).
        drafts = it.drafts.filterKeys { key -> key != bot.id && !key.startsWith("${bot.id}/") }) }
    persistDrafts()
    onDeleted()
}

/** A host call for the screens that keep their own state (memory): waits for the connection first. */
internal suspend fun AppStore.hostCall(method: String, body: JsonObject, timeoutMillis: Long = 240_000): JsonElement = ready().call(method, body, timeoutMillis)
internal suspend fun AppStore.hostFiles(): HostFiles = HostFiles(ready())

fun AppStore.loadRoutines(botId: String) = action("routines:$botId") { receiveRoutines(botId) }
private suspend fun AppStore.receiveRoutines(botId: String) {
    val active = ready()
    val response = active.call("routines", body("botId" to botId))
    val listing = WireJson.decodeFromJsonElement<RoutineListing>(response)
    mutableState.update { it.copy(routines = it.routines + (botId to listing)) }
}
fun AppStore.saveRoutine(botId: String, draft: RoutineDraft, saved: (Routine) -> Unit) = action("routines:$botId") {
    val active = ready()
    val response = active.call("saveRoutine", draft.body(botId))
    val routine = WireJson.decodeFromJsonElement<Routine>(response.jsonObject.getValue("routine"))
    receiveRoutines(botId)
    saved(routine)
}
fun AppStore.routineAction(botId: String, id: String, method: String, enabled: Boolean? = null, done: () -> Unit = {}) = action("routines:$botId") {
    require(method in listOf("setRoutineEnabled", "deleteRoutine", "runRoutine"))
    val active = ready()
    active.call(method, buildJsonObject { put("botId", botId); put("id", id); enabled?.let { put("enabled", it) } })
    receiveRoutines(botId)
    done()
}
/** Runs a failed routine run's work again, from its notice; the notice's update drops the button. */
fun AppStore.retryRoutineRun(entry: Entry) = action("retry:" + entry.id) {
    val runId = requireNotNull(entry.data.text("runId")) { "This run can't be retried." }
    ready().call("retryRoutineRun", body("botId" to entry.botId, "runId" to runId))
}
/** Queries belong to the account runtime as well as their screen; retiring either cancels them. */
internal suspend fun AppStore.hostQuery(method: String, payload: JsonObject, timeoutMillis: Long = 20_000,
    expectedGeneration: Int = state.value.generation): JsonElement {
    val request = contextScope.async {
        check(state.value.generation == expectedGeneration) { "The active account changed." }
        val active = ready()
        check(state.value.generation == expectedGeneration) { "The active account changed." }
        active.call(method, payload, timeoutMillis = timeoutMillis)
    }
    try { return request.await() } finally { request.cancel() }
}
suspend fun AppStore.agentModels(backend: String): AgentModels {
    val response = hostQuery("agentModels", body("backend" to backend), timeoutMillis = 300_000)
    return WireJson.decodeFromJsonElement(response)
}
/** A forced usage read that returns when it's done, for pull to refresh. */
suspend fun AppStore.reloadUsage() { refreshUsage(true); actions["usage"]?.join() }
fun AppStore.refreshUsage(force: Boolean = true) = action("usage") {
    val active = ready()
    // The existing host can spend up to a minute reading an installed provider.
    val result = active.call("usage", buildJsonObject { put("refresh", force) }, timeoutMillis = 75_000)
    val response = result.jsonObject
    UsageReport.decode(response)
    withContext(Dispatchers.IO) { requireNotNull(database).apply(MirrorEvent.Usage(response)) }
    refresh()
    widgetOwner?.let { owner ->
        val current = state.value
        withContext(Dispatchers.IO) { WidgetStore.publish(getApplication(), owner, current.computers.firstOrNull(),
            current.bots, UsageReport.decode(current.usage), false, true) }
        CodyncWidgets.update(getApplication())
    }
}
suspend fun AppStore.listDirs(path: String?): DirListing {
    val response = hostQuery("listDirs", body("path" to path))
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.routineSchedule(triggers: List<JsonObject>, zone: String): RoutineSchedulePreview {
    val response = hostQuery("routineSchedule", buildJsonObject { put("triggers", JsonArray(triggers)); put("timeZone", zone) })
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.routineSchedule(draft: RoutineScheduleDraft): RoutineSchedulePreview {
    val response = hostQuery("routineSchedule", buildJsonObject { put("draft", WireJson.encodeToJsonElement(draft)) })
    return WireJson.decodeFromJsonElement(response)
}
suspend fun AppStore.routineWebhook(botId: String, id: String, rotate: Boolean = false): RoutineWebhook {
    val response = hostQuery("routineWebhook", buildJsonObject { put("botId", botId); put("id", id); put("rotate", rotate) })
    return WireJson.decodeFromJsonElement(response)
}
