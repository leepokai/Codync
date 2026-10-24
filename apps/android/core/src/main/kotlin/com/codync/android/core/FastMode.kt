package com.codync.android.core

// Fast mode in the chat (the composer's `/fast`) and bot settings, as iOS `Bot+FastMode.swift`; the host reports
// what the agent offers (`fastModeSupport`, `fastModeIssue`): docs/features/marketplace.md, "Fast mode".

enum class FastModeState { On, Off, Refused }

val Bot.fastModeState: FastModeState get() = when {
    !fastMode -> FastModeState.Off
    fastModeSupport == "refused" -> FastModeState.Refused
    else -> FastModeState.On
}

/** What picking `/fast` in the composer's command list does, or why it isn't in effect. */
val Bot.fastModeDetail: String get() = when (fastModeState) {
    FastModeState.Off -> "Turn on for faster replies and more usage"
    FastModeState.On -> "Turn off and return to standard speed"
    FastModeState.Refused -> fastModeRefusal.orEmpty()
}

/** A group's `/fast`: the members that show Fast mode, switched together. Each member's own setting, so it holds in their own chats too. */
class GroupFastMode(members: List<Bot>) {
    val members: List<Bot> = members.filter { it.fastModeShown }

    /** On once every member that shows it has it on; refused when one of their agents turned it off. */
    val state: FastModeState get() = when {
        members.isEmpty() || !members.all { it.fastMode } -> FastModeState.Off
        members.any { it.fastModeState == FastModeState.Refused } -> FastModeState.Refused
        else -> FastModeState.On
    }

    val detail: String get() = when (state) {
        FastModeState.Off -> "Turn on for every bot here: faster replies and more usage"
        FastModeState.On -> "Turn off for every bot here and return to standard speed"
        FastModeState.Refused -> members.firstOrNull { it.fastModeState == FastModeState.Refused }?.let { "${it.name}: ${it.fastModeRefusal}" }.orEmpty()
    }
}
