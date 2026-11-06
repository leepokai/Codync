package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.runtime.collectAsState
import com.codync.android.core.AppVersion
import com.codync.android.core.VersionMismatch
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay

/** Which side must update before this app and the computer can work together (iOS `BotStore.mismatch`), from the host's `hello`. */
internal val AppState.mismatch: VersionMismatch? get() {
    val host = hello?.text("version")
    return VersionMismatch.check(BuildConfig.VERSION_NAME, host, hello?.text("minApp"))
        // Data this app can't read from a newer host: this app is behind, whatever `minApp` says.
        ?: if (unreadable && host != null && AppVersion.isBelow(BuildConfig.VERSION_NAME, host)) VersionMismatch.UpdateApp(host) else null
}

/** The time to judge `update` by, read again when a requested update's wait is over, so one that never finished is offered again. */
@Composable internal fun rememberHostUpdateNow(update: HostUpdate?): Long {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(update) {
        val requested = update as? HostUpdate.Requested ?: return@LaunchedEffect
        delay(requested.at + HOST_UPDATE_WAIT_MS - System.currentTimeMillis())
        now = System.currentTimeMillis()
    }
    return maxOf(now, System.currentTimeMillis())
}

/** Says which side has to update, shown instead of the composer and above the computer's bots. */
@Composable internal fun UpdateNeededCard(state: AppState, mismatch: VersionMismatch, requestUpdate: () -> Unit, modifier: Modifier = Modifier) {
    val host = state.hostName
    val now = rememberHostUpdateNow(state.hostUpdate)
    Row(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("arrow.down.circle", size = 22.dp, tint = Palette.warning)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(when (mismatch) {
                is VersionMismatch.UpdateApp -> "Update this app"
                is VersionMismatch.UpdateHost -> "Update Codync on $host"
            }, style = Type.subheadline.semibold, color = Palette.text)
            when (mismatch) {
                is VersionMismatch.UpdateApp -> {
                    val context = LocalContext.current
                    val store by AppUpdates.store.collectAsState()
                    // Google Play doesn't have the version this computer needs yet (an unknown version keeps the button: the page says).
                    val inReview = store?.let { AppVersion.isBelow(it, mismatch.minimum) } == true
                    Text("${state.hello?.text("version")?.let { "$host runs Codync $it and" } ?: host} needs this app to be " +
                        "${mismatch.minimum} or newer. Your chats are safe on the computer." +
                        if (inReview) " That version is still in Google Play review; it'll be there soon." else "", style = Type.footnote, color = Palette.secondary)
                    PrimaryButton("Update Codync", Modifier.padding(top = 4.dp), enabled = !inReview) { AppUpdates.open(context) }
                }
                is VersionMismatch.UpdateHost -> {
                    val notice = hostUpdateNotice(host, mismatch, state.hello?.text("os"), state.canUpdateHost, state.hostUpdate, now)
                    Text(notice.detail, style = Type.footnote, color = Palette.secondary)
                    if (notice.offersUpdate) PrimaryButton("Update now", Modifier.padding(top = 4.dp), action = requestUpdate)
                }
            }
        }
    }
}
