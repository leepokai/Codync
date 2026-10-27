package com.codync.android.core

import kotlinx.serialization.Serializable

/** A snapshot reported by the agent, with percentage and severity computed by the host. */
@Serializable
data class ContextUsage(
    val usedTokens: Long,
    val maxTokens: Long,
    val percent: Int,
    /** normal | warning | danger; future levels use the neutral presentation. */
    val level: String,
)
