package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.design.CardSection
import com.codync.android.design.ChoicePicker
import com.codync.android.design.ValueRow

@Composable
internal fun AppearanceSettings() {
    val mode by AppearancePreferences.mode.collectAsStateWithLifecycle()
    CardSection("Appearance", "Only changes appearance on this device.") {
        row {
            ValueRow("Theme") {
                ChoicePicker(mode, Appearance.entries.map { it to it.label }, AppearancePreferences::select, label = "Theme")
            }
        }
    }
}
