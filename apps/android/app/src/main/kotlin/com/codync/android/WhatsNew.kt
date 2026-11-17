package com.codync.android

import android.content.Context
import android.util.Log
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
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

/**
 * Each highlight's clip over its name and a sentence about it, then the smaller changes (iOS `WhatsNewSheet`).
 */
@Composable internal fun WhatsNewSheet(announcements: List<Announcement>) {
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("What's new") {
            announcements.firstOrNull()?.let { Text(it.version, style = Type.footnote, color = Palette.secondary) }
        }
        CardForm {
            for (announcement in announcements) {
                for (feature in announcement.features) FeatureStory(feature)
                if (announcement.notes.isNotEmpty()) Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(tr("Also in this release"), Modifier.semantics { heading() }, style = Type.subheadline.semibold, color = Palette.text)
                    for (note in announcement.notes) Text(tr(note), style = Type.subheadline, color = Palette.secondary)
                }
            }
        }
    }
}

/** One highlight: its looping clip, then its name and what it does (iOS `FeatureStory`). */
@Composable private fun FeatureStory(feature: Announcement.Feature) {
    val uri = LocalUriHandler.current
    val watch = tr("Watch the full video")
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        feature.media?.let { media ->
            Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(RoundedCornerShape(14.dp)).background(Color.Black)) {
                LoopingClip("$media.mp4", Modifier.fillMaxSize())
                // Opens the full video, with sound.
                feature.film?.let { film ->
                    Box(Modifier.align(Alignment.BottomEnd).padding(8.dp).size(32.dp).clip(CircleShape).background(Color.Black.copy(alpha = 0.55f))
                        .plainButton { uri.open(film, OutboundLink.WhatsNewFilm) }.semantics { contentDescription = watch },
                        contentAlignment = Alignment.Center) { Symbol("play.fill", size = 13.dp, tint = Color.White) }
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            Text(tr(feature.title), Modifier.semantics { heading() }, style = Type.title3.semibold, color = Palette.text)
            if (feature.beta) BetaTag()
        }
        Text(tr(feature.body), style = Type.body, color = Palette.secondary)
    }
}
