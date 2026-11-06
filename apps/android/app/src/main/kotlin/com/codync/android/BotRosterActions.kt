package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.design.MenuItem
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/** Shared single-source-of-truth mutations for the bot roster. */
internal fun AppStore.toggleBotPin(bot: Bot) {
    if (bot.lead) return
    command("updateBot", buildJsonObject { put("id", bot.id); put("pinned", !bot.pinned) }, "roster:" + bot.id)
}

internal fun AppStore.markBotRead(bot: Bot) {
    command("markRead", buildJsonObject { put("botId", bot.id); put("all", true) }, "roster:" + bot.id)
}

internal fun AppStore.hideBotFromRoster(bot: Bot) {
    if (bot.lead) return
    command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", true) }, "roster:" + bot.id)
}

/**
 * Stops an active bot execution without faking local idle state; keeps `"stop:${bot.id}"` in `busy`
 * until the host confirms the command.
 */
internal fun AppStore.stopBot(bot: Bot) {
    command("stop", body("botId" to bot.id), "stop:" + bot.id)
}

/** Stops every bot on this computer that is working or waiting on you; keeps `"stopAll"` in `busy` until the host confirms. */
internal fun AppStore.stopAllBots() {
    command("stopAll", body(), "stopAll")
}

/**
 * Revalidates that `entryId` is still `"pending"` in the active state before sending `respondPermission`.
 * If another client already answered or expired it, opens the bot's chat instead.
 */
internal fun AppStore.respondToAttention(bot: Bot, entryId: String, optionId: String) {
    val current = state.value.entries.firstOrNull {
        it.id == entryId && it.botId == bot.id && it.kind == "permission" && it.data.text("status") == "pending"
    }
    if (current != null) respond(current, optionId)
    else openBot(bot.id)
}

/**
 * Bounded, cancellable peek query that loads up to `limit` recent messages into the local mirror
 * without marking the conversation as read or setting `historyComplete`.
 */
internal suspend fun AppStore.peekBotHistory(botId: String, limit: Int = 3) {
    val expectedGen = state.value.generation
    val mirror = database ?: return
    val response = hostQuery(
        "history",
        buildJsonObject {
            put("botId", botId)
            put("beforeSeq", Long.MAX_VALUE)
            put("limit", limit)
        },
        timeoutMillis = 8_000,
        expectedGeneration = expectedGen,
    )
    if (state.value.generation != expectedGen) return
    val entries = response.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
    withContext(Dispatchers.IO) { mirror.receive(entries) }
    if (state.value.generation == expectedGen) refresh()
}

/** Both roster presentations and accessibility actions send the same intents to the host. */
internal fun botRosterActions(bot: Bot, store: AppStore, edit: () -> Unit, delete: () -> Unit,
    newSession: () -> Unit = { store.command("newSession", body("botId" to bot.id)) }): List<MenuItem> =
    buildList {
        if (!bot.lead) add(
            MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = if (bot.pinned) "pin.slash" else "pin") {
                store.toggleBotPin(bot)
            },
        )
        add(
            if (bot.kind == "group") MenuItem("Edit group", icon = "person.2", action = edit)
            else MenuItem("Edit profile", icon = "pencil", action = edit),
        )
        if (bot.status == "working") {
            add(
                MenuItem(
                    title = "Stop",
                    icon = "stop.fill",
                    enabled = "stop:${bot.id}" !in store.state.value.busy,
                ) { store.stopBot(bot) },
            )
        }
        add(
            MenuItem("Mark as read", icon = "checkmark.message") {
                store.markBotRead(bot)
            },
        )
        if (bot.kind != "group") add(MenuItem("New session", icon = "arrow.counterclockwise", action = newSession))
        if (!bot.lead) add(
            MenuItem("Hide from list", icon = "eye.slash") {
                store.hideBotFromRoster(bot)
            },
        )
        if (!bot.lead) add(MenuItem("Delete", icon = "trash", destructive = true, divider = true, action = delete))
    }
