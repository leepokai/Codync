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
    val agentLogos by AppearancePreferences.agentLogos.collectAsStateWithLifecycle()
    CardSection("Appearance", "Only changes appearance on this device.") {
        row {
            ValueRow("Theme") {
                ChoicePicker(mode, Appearance.entries.map { it to it.label }, AppearancePreferences::select, label = "Theme")
            }
        }
        row {
            SwitchRow("Show agent logos", "Marks each bot with its agent's logo when your bots use more than one agent.",
                isOn = agentLogos, onChange = AppearancePreferences::showAgentLogos)
        }
    }
}
