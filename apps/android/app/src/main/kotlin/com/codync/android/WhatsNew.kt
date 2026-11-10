package com.codync.android

import android.content.Context
import android.util.Log
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.codync.android.core.Announcement
import com.codync.android.core.WhatsNewRun
import com.codync.android.design.*

/**
 * This run's What's new window (iOS `WhatsNew`, docs/features/whats-new.md), decided once per process when the app
 * first opens: before onboarding can finish, so a new install never counts as an update. Main thread only.
 */
internal object WhatsNew {
    private const val SEEN = "whatsNewSeen"
    private var run: WhatsNewRun? = null
    /** Due on this run, newest first. */
    val due get() = run?.due.orEmpty()
    /** Until it's closed. */
    var isPresented by mutableStateOf(false)

    /** After `Analytics.start`, whose onboarding record says whether this phone was set up before. */
    fun start(context: Context) {
        if (run != null) return
        val prefs = context.getSharedPreferences("codync-whats-new", Context.MODE_PRIVATE)
        val started = WhatsNewRun(bundled(context), prefs.getString(SEEN, null), BuildConfig.VERSION_NAME, Analytics.onboarded,
            save = { prefs.edit().putString(SEEN, it).apply() }, capture = { Analytics.capture(AnalyticsEvent.WhatsNewShown) })
        run = started
        isPresented = started.due.isNotEmpty()
    }

    /** On screen: not again on this version. */
    fun shown() {
        run?.shown()
    }

    /** A missing or malformed file is logged and shows nothing. */
    private fun bundled(context: Context): List<Announcement> = try {
        Announcement.parse(context.assets.open("announcements.json").bufferedReader().use { it.readText() })
    } catch (e: Exception) {
        Log.e("WhatsNew", "announcements.json failed to load", e)
        emptyList()
    }
}

/** Each due release's notes (iOS `WhatsNewSheet`). */
@Composable internal fun WhatsNewSheet(announcements: List<Announcement>) {
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("What's new")
        CardForm {
            for (announcement in announcements) CardSection(announcement.version) {
                for (note in announcement.notes) row { Text(tr(note), style = Type.body, color = Palette.text) }
            }
        }
    }
}
