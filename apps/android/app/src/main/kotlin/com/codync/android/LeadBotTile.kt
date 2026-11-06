package com.codync.android

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import com.codync.android.core.mostUrgentBot
import com.codync.android.design.symbolRes

/**
 * The Bots tile in Quick Settings (the iOS Control Center control): the bot that needs you most
 * and its state, from the widgets' report; a tap opens it, or Computer management when no bot is at work.
 * An active tile: [refresh] redraws it each time that report changes, so it's current before Quick Settings opens.
 * (Its class keeps its first name, the manifest's component: tiles already placed stay.)
 */
class LeadBotTile : TileService() {
    companion object {
        /** Asks the system to bind the tile so it redraws from the latest report; nothing happens while it isn't added. */
        fun refresh(context: Context) {
            try { requestListeningState(context, ComponentName(context, LeadBotTile::class.java)) }
            catch (_: RuntimeException) { /* No Quick Settings on this device, or the system refused: the tile still redraws when shown. */ }
        }
    }

    // An active tile isn't bound on its own: draw it as soon as it's added.
    override fun onTileAdded() = refresh(this)

    override fun onStartListening() {
        val tile = qsTile ?: return
        val feed = WidgetStore.feed(this)
        val urgent = mostUrgentBot(feed.bots)
        val (label, state, symbol) = when {
            feed.computerId == null -> Triple("Bots", "Connect a computer", "desktopcomputer")
            urgent == null -> Triple("Bots", "All quiet", "moon.zzz.fill")
            urgent.status == "needsInput" -> Triple(urgent.name, "Needs you", "hand.raised.fill")
            else -> Triple(urgent.name, "Working", "ellipsis.bubble.fill")
        }
        tile.label = label
        tile.subtitle = state
        tile.contentDescription = "$label, $state"
        tile.icon = Icon.createWithResource(this, requireNotNull(symbolRes(symbol)))
        tile.state = if (urgent != null) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.updateTile()
    }

    // Before Android 14 the Intent form is the only one; it is guarded by version.
    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        val feed = WidgetStore.feed(this)
        // No bot at work: Computer management, as the iPhone's control does.
        val intent = mostUrgentBot(feed.bots)?.let { widgetIntent(this, feed, it.id) }
            ?: widgetIntent(this, feed).setData(Uri.parse("${BuildConfig.URL_SCHEME}://computers"))
        // A bot's chat is private: the phone unlocks first.
        unlockAndRun {
            if (Build.VERSION.SDK_INT >= 34) startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            else @Suppress("DEPRECATION") startActivityAndCollapse(intent)
        }
    }
}
