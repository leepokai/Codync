package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

/** One bot-to-bot exchange as seen from the chat of one of the two bots (CodyncKit `BotExchange`). */
data class BotExchange(val id: String, val chatId: String, val seq: Long, val rev: Long, val createdAt: Long,
    val source: String, val target: String, val text: String, val reply: String?, val outcome: Outcome,
    /** The peer's name when it is a bot on another computer (its id is `name@computer`, not in this computer's list). */
    val peerName: String? = null) {
    sealed interface Outcome {
        data object Pending : Outcome
        data object Done : Outcome
        data class Failed(val detail: String) : Outcome
    }

    val isOutgoing: Boolean get() = chatId == source
    val peerId: String get() = if (isOutgoing) target else source
    val verb: String get() = if (isOutgoing) "Messaged" else "Message from"

    companion object {
        /** A notice carrying the host's structured `botMessage` is an exchange; other notices stay plain. */
        fun of(entry: Entry): BotExchange? {
            val message = entry.data["botMessage"] as? JsonObject ?: return null
            if (entry.kind != "notice") return null
            fun field(name: String) = message[name]?.jsonPrimitive?.contentOrNull
            val source = field("sourceBotId") ?: return null
            val target = field("targetBotId") ?: return null
            val outcome = when (entry.status) {
                "completed" -> Outcome.Done
                "failed", "cancelled" -> Outcome.Failed(field("detail").orEmpty())
                else -> Outcome.Pending
            }
            return BotExchange(entry.id, entry.botId, entry.seq, entry.rev, entry.createdAt, source, target, field("text").orEmpty(), field("reply"), outcome,
                field("peerName"))
        }

        /** The exchanges with `peer` in a chat, oldest first: what the host returned plus the live chat (the higher `rev` wins). */
        fun conversation(fetched: List<Entry>, live: List<Entry>, peer: String): List<BotExchange> =
            (fetched + live).filter { it.kind == "notice" }.groupBy { it.id }.values.map { copies -> copies.maxBy { it.rev } }
                .mapNotNull(::of).filter { it.peerId == peer }.sortedBy { it.seq }
    }
}

/** A run of consecutive exchanges with one peer in a chat's main transcript, shown as one row. */
data class BotExchangeGroup(val exchanges: List<BotExchange>) {
    val id: String get() = exchanges.first().id
    val peerId: String get() = exchanges.first().peerId
    /** The peer's name when it is on another computer. */
    val peerName: String? get() = exchanges.firstNotNullOfOrNull { it.peerName }
    val createdAt: Long get() = exchanges.first().createdAt
    val failed: Boolean get() = exchanges.any { it.outcome is BotExchange.Outcome.Failed }
    /** What the row says before the peer's name: "Messaged" for one, "3 messages with" for more. */
    val title: String get() = if (exchanges.size == 1) exchanges.first().verb else "${exchanges.size} messages with"
}

/** A line of the read-only conversation sheet. */
sealed interface BotConversationRow {
    val id: String
    data class Separator(override val id: String, val at: Long) : BotConversationRow
    data class Message(override val id: String, val author: String, val text: String, val showsAuthor: Boolean) : BotConversationRow
    /** `awaiting` is the bot being waited for while the outcome is pending. */
    data class Status(override val id: String, val outcome: BotExchange.Outcome, val awaiting: String) : BotConversationRow

    companion object {
        /** Rows for exchanges oldest first; a gap over an hour starts a new time block, and a separator or status resets the author. */
        fun build(exchanges: List<BotExchange>): List<BotConversationRow> = buildList {
            var author: String? = null
            var at: Long? = null
            for (x in exchanges) {
                if (at == null || x.createdAt - at > 3_600_000) { add(Separator("sep-${x.id}", x.createdAt)); author = null }
                at = x.createdAt
                add(Message("${x.id}-request", x.source, x.text, author != x.source)); author = x.source
                if (!x.reply.isNullOrEmpty()) { add(Message("${x.id}-reply", x.target, x.reply, author != x.target)); author = x.target }
                if (x.outcome != BotExchange.Outcome.Done) { add(Status("${x.id}-status", x.outcome, x.target)); author = null }
            }
        }
    }
}
