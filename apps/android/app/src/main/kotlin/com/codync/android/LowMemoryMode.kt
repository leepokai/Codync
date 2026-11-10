package com.codync.android

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Device preference: keep less chat history in memory. Off (the default) loads each chat's newest rows as usual.
 * On shrinks the previews and the open chat's window, so a phone with a small heap or a very large roster stays up.
 * It can only load less, never more, and older messages stay on the phone and load as the reader scrolls back.
 */
internal object LowMemoryMode {
    private var stored: SharedPreferences? = null
    private val current = MutableStateFlow(false)
    val enabled = current.asStateFlow()

    fun initialize(context: Context) {
        val preferences = context.getSharedPreferences("codync-memory", Context.MODE_PRIVATE)
        stored = preferences
        current.value = preferences.getBoolean("lowMemory", false)
    }

    fun set(on: Boolean) {
        stored?.edit()?.putBoolean("lowMemory", on)?.apply()
        current.value = on
        Analytics.capture(AnalyticsEvent.SettingChanged(PhoneSetting.LowMemory, on))
    }

    /** How much of each chat a snapshot loads; [open] chats get a deeper window than the rest. */
    fun residency(open: Set<String>, windows: Map<String, Int>): Residency =
        if (enabled.value) Residency(open, preview = LOW_PREVIEW, window = LOW_WINDOW, windows = windows, ceiling = LOW_CEILING)
        else Residency(open, windows = windows)

    private const val LOW_PREVIEW = 10
    private const val LOW_WINDOW = 100
    private const val LOW_CEILING = 1000
}
