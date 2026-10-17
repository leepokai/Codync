package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.codync.android.core.AppVersion
import com.codync.android.core.VersionMismatch
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** Which side must update before this app and the computer can work together (iOS `BotStore.mismatch`), from the host's `hello`. */
internal val AppState.mismatch: VersionMismatch? get() {
    val host = hello?.text("version")
    return VersionMismatch.check(BuildConfig.VERSION_NAME, host, hello?.text("minApp"))
        // Data this app can't read from a newer host: this app is behind, whatever `minApp` says.
        ?: if (unreadable && host != null && AppVersion.isBelow(BuildConfig.VERSION_NAME, host)) VersionMismatch.UpdateApp(host) else null
}

/** Says which side has to update, shown instead of the composer and above the computer's bots. */
@Composable internal fun UpdateNeededCard(state: AppState, mismatch: VersionMismatch, modifier: Modifier = Modifier) {
    val host = state.hello?.text("name") ?: state.computers.firstOrNull()?.name ?: "your computer"
    Row(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("arrow.down.circle", size = 22.dp, tint = Palette.warning)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(when (mismatch) {
                is VersionMismatch.UpdateApp -> "Update this app"
                is VersionMismatch.UpdateHost -> "Update Codync on $host"
            }, style = Type.subheadline.semibold, color = Palette.text)
            Text(when (mismatch) {
                is VersionMismatch.UpdateApp -> "${state.hello?.text("version")?.let { "$host runs Codync $it and" } ?: host} needs this app to be " +
                    "${mismatch.minimum} or newer. Your chats are safe on the computer."
                is VersionMismatch.UpdateHost -> "$host runs Codync ${mismatch.version}; this app needs ${mismatch.minimum} or newer. " + when (state.hello?.text("os")) {
                    "macos" -> "Open Codync on that Mac and choose Check for Updates."
                    "linux" -> "Run codync-host update there."
                    else -> "Update Codync on that computer."
                }
            }, style = Type.footnote, color = Palette.secondary)
        }
    }
}
