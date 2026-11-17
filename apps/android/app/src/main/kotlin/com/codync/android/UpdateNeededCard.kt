package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import com.codync.android.core.LinkState
import com.codync.android.core.VersionMismatch
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay

/**
 * Which side must update before this app and the computer can work together (iOS `BotStore.mismatch`), from the
 * host's `hello`; first, the Codync cloud turning this app version away (`426`, an emergency floor).
 */
internal val AppState.mismatch: VersionMismatch? get() {
    appFloor?.let { return VersionMismatch.AppFloor(it) }
    (connection as? LinkState.Failed)?.takeIf { it.status == 426 }?.let { return VersionMismatch.AppFloor(it.message) }
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

/**
 * Says which side has to update, shown instead of the composer and above the computer's bots. A cloud computer
 * updates itself: it says so, with no instructions, while the link keeps reconnecting.
 */
@Composable internal fun UpdateNeededCard(state: AppState, mismatch: VersionMismatch, requestUpdate: () -> Unit, modifier: Modifier = Modifier) {
    val translate = translator()
    val host = state.hostName
    val now = rememberHostUpdateNow(state.hostUpdate)
    val updating = state.cloudUpdating
    Row(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp),
        verticalAlignment = if (updating) Alignment.CenterVertically else Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (updating) Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) { Spinner(18.dp) }
        else Symbol("arrow.down.circle", size = 22.dp, tint = Palette.warning)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(tr(when (mismatch) {
                is VersionMismatch.UpdateApp, is VersionMismatch.AppFloor -> "Update this app"
                is VersionMismatch.UpdateHost -> if (updating) "Your cloud computer is updating…" else "Update Codync on $host"
            }), style = Type.subheadline.semibold, color = Palette.text)
            when (mismatch) {
                is VersionMismatch.UpdateApp -> {
                    val context = LocalContext.current
                    val store by AppUpdates.store.collectAsState()
                    // Google Play doesn't have the version this computer needs yet (an unknown version keeps the button: the page says).
                    val inReview = store?.let { AppVersion.isBelow(it, mismatch.minimum) } == true
                    Text(appUpdateDetail(host, mismatch.minimum, translate) +
                        if (inReview) " That version is still in Google Play review; it'll be there soon." else "", style = Type.footnote, color = Palette.secondary)
                    PrimaryButton("Update Codync", Modifier.padding(top = 4.dp), enabled = !inReview) { AppUpdates.open(context) }
                }
                is VersionMismatch.AppFloor -> {
                    val context = LocalContext.current
                    Text(mismatch.reason, style = Type.footnote, color = Palette.secondary)
                    PrimaryButton("Update Codync", Modifier.padding(top = 4.dp)) { AppUpdates.open(context) }
                }
                is VersionMismatch.UpdateHost -> if (updating) {
                    Text(cloudUpdateDetail(mismatch.minimum, translate), style = Type.footnote, color = Palette.secondary)
                } else {
                    val notice = hostUpdateNotice(host, mismatch, state.hello?.text("os"), state.canUpdateHost, state.hostUpdate, now, translate)
                    Text(notice.detail, style = Type.footnote, color = Palette.secondary)
                    if (notice.offersUpdate) PrimaryButton("Update now", Modifier.padding(top = 4.dp), action = requestUpdate)
                }
            }
        }
    }
}
