package com.codync.android.core

import kotlinx.serialization.Serializable

/** The private alert the host seals to this phone's push key (spec §6.7). */
@Serializable data class PushAlert(
    val title: String,
    /** What happened: "Task complete", "Response needed" or "Task failed". */
    val subtitle: String? = null,
    val body: String,
    /** The member bot speaking, when the alert is for a group chat. */
    val from: String? = null,
    /** The chat, its first members and the speaker: names and avatars only, sent with the alert
     *  so a bot the phone hasn't seen yet still shows its face. */
    val faces: List<Bot> = emptyList(),
    /** A pending approval this alert can answer without opening the app; null for other alerts. */
    val approval: PushApproval? = null,
) {
    /** Who the alert is from, for a conversation notification; null when no face came for the chat. */
    fun conversation(botId: String): AlertConversation? {
        val chat = faces.firstOrNull { it.id == botId } ?: return null
        val members = chat.members.mapNotNull { id -> faces.firstOrNull { it.id == id } }
        val speaker = faces.firstOrNull { it.id == from }.takeIf { chat.kind == "group" }
        return AlertConversation(chat, members, speaker)
    }

    companion object {
        fun decode(plain: String): PushAlert = WireJson.decodeFromString<PushAlert>(plain).run {
            copy(title = title.take(120), subtitle = subtitle?.take(120), body = body.take(400), faces = faces.take(8),
                approval = approval?.sanitized())
        }
    }
}

/** One option of a pending approval, as the agent offered it. */
@Serializable data class PushOption(val optionId: String, val name: String, val kind: String = "")

/** The pending approval an alert carries (host 2.x): the card to answer and the agent's real options. */
@Serializable data class PushApproval(val entryId: String, val options: List<PushOption> = emptyList()) {
    /** What a notification may show: a bounded entry id and a few named options; null when nothing is answerable. */
    fun sanitized(): PushApproval? {
        // The same checks that accept a tap: a button a notification shows must be one the receiver keeps.
        if (!NotificationActionPlan.validEntry(entryId)) return null
        val kept = options.filter { NotificationActionPlan.validOption(it.optionId) && it.name.isNotBlank() }
            .take(MAX_OPTIONS)
            .map { it.copy(name = it.name.take(60), kind = it.kind.take(20)) }
        return if (kept.isEmpty()) null else copy(options = kept)
    }

    companion object {
        /** The notification shows at most three: the system drops the extra ones anyway. */
        const val MAX_OPTIONS = 3
    }
}

/** A chat's alert: the chat, its members the alert carried a face for, and in a group the member speaking. */
data class AlertConversation(val chat: Bot, val members: List<Bot>, val speaker: Bot?)
