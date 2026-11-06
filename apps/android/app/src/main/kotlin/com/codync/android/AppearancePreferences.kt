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

    fun initialize(context: Context) {
        val preferences = context.getSharedPreferences("codync-appearance", Context.MODE_PRIVATE)
        stored = preferences
        current.value = Appearance.entries.firstOrNull { it.value == preferences.getString("mode", null) } ?: Appearance.System
    }

    fun select(value: Appearance) {
        stored?.edit()?.putString("mode", value.value)?.apply()
        current.value = value
    }
}
