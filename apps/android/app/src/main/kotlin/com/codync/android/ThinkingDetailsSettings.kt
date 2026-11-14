package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.design.CardSection

/** Settings › Chat: whether the working pill unfolds the turn's thinking and steps on this device. */
@Composable
internal fun ThinkingDetailsSettings() {
    val details by ThinkingDetails.enabled.collectAsStateWithLifecycle()
    CardSection("Chat", "Lets you unfold a working bot's thinking and steps from its status in the chat. Only changes this device.") {
        row { SwitchRow("Show thinking details", isOn = details, onChange = ThinkingDetails::set) }
    }
}
