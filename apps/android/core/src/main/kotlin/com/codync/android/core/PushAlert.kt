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
            copy(title = title.take(120), subtitle = subtitle?.take(120), body = body.take(400), faces = faces.take(8))
        }
    }
}

/** A chat's alert: the chat, its members the alert carried a face for, and in a group the member speaking. */
data class AlertConversation(val chat: Bot, val members: List<Bot>, val speaker: Bot?)
