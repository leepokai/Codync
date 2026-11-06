package com.codync.android.core

import kotlinx.serialization.Serializable

/** The last days at a glance, counted by the host from the computer's transcripts (`weekSummary`). No message text. */
@Serializable data class WeekSummary(
    val messages: Int = 0,
    val replies: Int = 0,
    val tools: Int = 0,
    val fileEdits: Int = 0,
    val approvalsAsked: Int = 0,
    val approvalsAnswered: Int = 0,
    val approvalsWaiting: Int = 0,
    val activeBots: Int = 0,
    val topBots: List<TopBot> = emptyList(),
) {
    @Serializable data class TopBot(val id: String, val name: String = "", val entries: Int = 0)
}
