package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.*

// The host's ready-made bots and coordinator teams (`botCatalog`, `installTeam`).

suspend fun AppStore.botCatalog(): BotCatalog = WireJson.decodeFromJsonElement(hostQuery("botCatalog", JsonObject(emptyMap())))

/** The busy key while a catalog bot or team is being added. */
internal fun catalogKey(id: String) = "catalog:$id"

/** Adds a ready-made bot on the computer's defaults, staying on the current screen. */
fun AppStore.addProfile(draft: BotDraft, profileId: String, onAdded: () -> Unit) = action(catalogKey(profileId)) {
    val result = ready().call("createBot", draft.body())
    receiveBot(result)
    onAdded()
}

/** Creates a catalog team on the host: its coordinator, its teammates and their group. */
fun AppStore.installTeam(id: String, onAdded: () -> Unit) = action(catalogKey(id)) {
    val result = ready().call("installTeam", buildJsonObject { put("id", id) }, timeoutMillis = 60_000)
    receiveBot(result)
    onAdded()
}

private suspend fun AppStore.receiveBot(result: JsonElement) {
    val bot = HostClient.decode(result.jsonObject.getValue("bot").jsonObject)
    withContext(Dispatchers.IO) { requireNotNull(database).receive(bot) }
    refresh()
}

/** Adds a marketplace agent with its look, instructions, skills and routines. */
fun AppStore.installAgent(id: String, onAdded: () -> Unit) = action(catalogKey(id)) {
    val result = ready().call("installAgent", buildJsonObject { put("id", id) }, timeoutMillis = 120_000)
    receiveBot(result)
    refreshPlugins()
    onAdded()
}
