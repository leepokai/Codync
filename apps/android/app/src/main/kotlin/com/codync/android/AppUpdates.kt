package com.codync.android

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import com.codync.android.core.UpdateReminder
import com.codync.android.design.*
import com.google.android.play.core.appupdate.AppUpdateManager
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.install.model.UpdateAvailability
import com.google.android.play.core.ktx.requestAppUpdateInfo
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * What Google Play has for this app, asked now and then (iOS `AppUpdates`): the update notice greys out its button while
 * the needed version isn't on Play yet, and a newer release gets a dismissible reminder. A copy not installed from Google
 * Play gets neither.
 */
/** This app's release, without a build's suffix ("2.34.0" of "2.34.0-dev"). */
internal val APP_RELEASE: String = BuildConfig.VERSION_NAME.substringBefore('-')

internal object AppUpdates {
    private val mutableStore = MutableStateFlow<String?>(null)
    /** The version Play has for this phone; null until asked, or when Play can't say. */
    val store = mutableStore.asStateFlow()
    private val mutableDismissed = MutableStateFlow<String?>(null)
    val dismissed = mutableDismissed.asStateFlow()
    private var askedAt = 0L

    private fun prefs(context: Context) = context.getSharedPreferences("codync-updates", Context.MODE_PRIVATE)

    /** Asks Play at most every six hours (launch, foreground). */
    suspend fun refresh(context: Context) {
        mutableDismissed.value = prefs(context).getString("dismissedAppUpdate", null)
        val now = System.currentTimeMillis()
        if (now - askedAt < 6 * 3_600_000L) return
        askedAt = now
        ask(AppUpdateManagerFactory.create(context))
    }

    /** What [manager] (Google Play's, or a test's stand-in) has for this phone. */
    internal suspend fun ask(manager: AppUpdateManager) {
        try {
            val info = manager.requestAppUpdateInfo()
            mutableStore.value = if (info.updateAvailability() == UpdateAvailability.UPDATE_AVAILABLE)
                UpdateReminder.versionOf(info.availableVersionCode().toLong()) else APP_RELEASE
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* Not from Google Play, or Play can't be reached: no reminder. */ }
    }

    fun dismiss(context: Context) {
        val version = store.value ?: return
        mutableDismissed.value = version
        prefs(context).edit { putString("dismissedAppUpdate", version) }
    }

    /** Opens this app's Google Play page. */
    fun open(context: Context) {
        Analytics.capture(AnalyticsEvent.LinkOpened(OutboundLink.AppStore))
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(PLAY_LISTING)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/** A quiet, dismissible note at the top of the roster that a newer release is out; everything still works (iOS `AppUpdateReminder`). */
@Composable internal fun AppUpdateReminder(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val store by AppUpdates.store.collectAsState()
    val dismissed by AppUpdates.dismissed.collectAsState()
    val version = UpdateReminder.app(BuildConfig.VERSION_NAME, store, dismissed) ?: return
    Row(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("arrow.down.circle.fill", size = 22.dp, tint = Palette.text)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(tr("Codync {version} is available").replace("{version}", version), style = Type.subheadline.semibold, color = Palette.text)
            Text(tr("You have {version}.").replace("{version}", APP_RELEASE), style = Type.footnote, color = Palette.secondary)
            SecondaryButton("Update", Modifier.padding(top = 4.dp)) { AppUpdates.open(context) }
        }
        IconButton("Dismiss", "xmark") { AppUpdates.dismiss(context) }
    }
}
