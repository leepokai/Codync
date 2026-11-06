package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.takeWhile
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/** The initial hello is a catch-up target, never evidence that the roster arrived. */
internal suspend fun botSnapshot(events: Flow<JsonObject>): List<Bot> {
    val bots = linkedMapOf<String, Bot>()
    var head: Long? = null
    var complete = false
    events.takeWhile { event ->
        when (event.text("type")) {
            "hello" -> {
                check(head == null) { "The computer restarted its roster snapshot." }
                // The last rev the catch-up sends; `rev` counts mutations it never replays (older hosts lack it).
                head = requireNotNull((event["catchUpRev"] ?: event["rev"])?.jsonPrimitive?.longOrNull).also { require(it >= 0) }
                complete = head == 0L
            }
            "bot", "entry" -> {
                val target = requireNotNull(head) { "The computer sent updates before its snapshot header." }
                if (event.text("type") == "bot") {
                    val bot = HostClient.decode(event.getValue("bot").jsonObject)
                    if (bot.deleted) bots.remove(bot.id) else bots[bot.id] = bot
                }
                val revision = requireNotNull(event["rev"]?.jsonPrimitive?.longOrNull)
                complete = revision >= target
            }
            "resync" -> throw IOException("The computer's roster changed too quickly. Try again.")
        }
        !complete
    }.collect()
    check(complete) { "The computer disconnected before its roster finished loading." }
    return bots.values.toList()
}
