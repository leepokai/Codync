package com.codync.android

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import com.codync.android.core.leadBot
import com.codync.android.design.symbolRes

/**
 * The Bots tile in Quick Settings (the iOS Control Center control): the bot that needs you most
 * and its state, from the widgets' report; a tap opens it, or Codync when no bot is at work.
 */
class LeadBotTile : TileService() {
    override fun onStartListening() {
        val tile = qsTile ?: return
        val feed = WidgetStore.feed(this)
        val lead = leadBot(feed.bots)
        val (label, state, symbol) = when {
            feed.computerId == null -> Triple("Bots", "Connect a computer", "desktopcomputer")
            lead == null -> Triple("Bots", "All quiet", "moon.zzz.fill")
            lead.status == "needsInput" -> Triple(lead.name, "Needs you", "hand.raised.fill")
            else -> Triple(lead.name, "Working", "ellipsis.bubble.fill")
        }
        tile.label = label
        tile.subtitle = state
        tile.contentDescription = "$label, $state"
        tile.icon = Icon.createWithResource(this, requireNotNull(symbolRes(symbol)))
        tile.state = if (lead != null) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.updateTile()
    }

    // Before Android 14 the Intent form is the only one; it is guarded by version.
    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        val feed = WidgetStore.feed(this)
        val intent = widgetIntent(this, feed, leadBot(feed.bots)?.id)
        // A bot's chat is private: the phone unlocks first.
        unlockAndRun {
            if (Build.VERSION.SDK_INT >= 34) startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            else @Suppress("DEPRECATION") startActivityAndCollapse(intent)
        }
    }
}
