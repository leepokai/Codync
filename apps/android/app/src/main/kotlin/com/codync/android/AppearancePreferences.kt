package com.codync.android

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

internal enum class Appearance(val value: String, val label: String) {
    System("system", "System"), Light("light", "Light"), Dark("dark", "Dark");

    fun isDark(system: Boolean): Boolean = when (this) {
        System -> system
        Light -> false
        Dark -> true
    }
}

/** Device-wide UI preference, independent of the active account and host. */
internal object AppearancePreferences {
    private var stored: SharedPreferences? = null
    private val current = MutableStateFlow(Appearance.System)
    val mode = current.asStateFlow()
    private val logos = MutableStateFlow(true)
    /** The roster marks each bot with its agent's logo while the bots run more than one agent (iOS `AgentLogos`). */
    val agentLogos = logos.asStateFlow()

    fun initialize(context: Context) {
        val preferences = context.getSharedPreferences("codync-appearance", Context.MODE_PRIVATE)
        stored = preferences
        current.value = Appearance.entries.firstOrNull { it.value == preferences.getString("mode", null) } ?: Appearance.System
        logos.value = preferences.getBoolean("showAgentLogos", true)
    }

    fun select(value: Appearance) {
        stored?.edit()?.putString("mode", value.value)?.apply()
        current.value = value
    }

    fun showAgentLogos(on: Boolean) {
        stored?.edit()?.putBoolean("showAgentLogos", on)?.apply()
        logos.value = on
    }
}
