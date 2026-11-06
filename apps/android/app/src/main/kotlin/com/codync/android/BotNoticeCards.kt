package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

/** Sandboxed mini-app card published by the bot in chat (Ideas 1–7). */
@Composable internal fun BotAppNoticeCard(entry: Entry, store: AppStore) {
    val appJson = entry.data["botApp"] as? JsonObject ?: return
    val app = remember(appJson, entry.botId) { runCatching { botAppFromNotice(entry.botId, appJson) }.getOrNull() }
    if (app == null) {
        Text("Mini-app details are unavailable. Open the bot's settings to try again.", style = Type.chatFootnote, color = Palette.danger)
        return
    }
    var open by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("macwindow", size = 16.dp, tint = Palette.accent)
            Text(app.title, style = Type.chatSubheadline.semibold, color = Palette.text)
            Text("v${app.version}", Modifier.clip(Capsule()).background(Palette.surface).padding(horizontal = 6.dp, vertical = 2.dp),
                style = Type.caption2.semibold, color = Palette.secondary)
        }
        app.description?.takeIf { it.isNotBlank() }?.let { Text(it, style = Type.chatFootnote, color = Palette.secondary) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton("Open App") { open = true }
        }
    }
    if (open) CodyncSheet(app, { open = false }) { BotAppSheet(botId = entry.botId, app = it, store = store) }
}

/** Ideas 13, 19 & 20: Human takeover request card with action bar in chat. */
@Composable internal fun TakeoverRequestCard(entry: Entry, state: AppState, store: AppStore) {
    val req = entry.data["takeoverRequest"] as? JsonObject ?: return
    val reason = req.text("reason").orEmpty()
    val url = req.text("url").orEmpty()
    val status = req.text("status")
    val resolved = status == "resolved" || status == "cancelled"
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("hand.raised.fill", size = 16.dp, tint = Palette.warning)
            Text("Human Takeover Requested", style = Type.chatSubheadline.semibold, color = Palette.warning)
        }
        Text(if (url.isNotBlank()) "$reason · $url" else reason, style = Type.chatFootnote, color = Palette.secondary)
        if (!resolved) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PrimaryButton("Done — hand back") {
                    store.sendDraft(entry.botId, entry.threadId, "I have completed the requested step in the browser. Please continue.") {}
                }
                SecondaryButton("Cancel") { state.bots.firstOrNull { it.id == entry.botId }?.let { store.stopBot(it) } }
            }
        } else {
            Text(if (status == "resolved") "Completed" else "Cancelled", style = Type.caption, color = Palette.tertiary)
        }
    }
}

/** Interactive Passkey / 2FA / Captcha assistance card from CDP browser detector (INIC-02). */
@Composable internal fun PasskeyWallCard(entry: Entry, state: AppState, store: AppStore) {
    val wall = entry.data["passkeyWall"] as? JsonObject ?: return
    val kind = wall.text("kind")?.uppercase() ?: "AUTH"
    val url = wall.text("url").orEmpty()
    val reason = wall.text("reason").orEmpty()
    val status = wall.text("status")
    val resolved = status == "resolved" || status == "cancelled"
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("person.badge.key.fill", size = 16.dp, tint = Palette.warning)
            Text("Authentication Required ($kind)", style = Type.chatSubheadline.semibold, color = Palette.warning)
        }
        Text("$reason · $url", style = Type.chatFootnote, color = Palette.secondary)
        if (!resolved) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                PrimaryButton("Continue") {
                    store.sendDraft(entry.botId, entry.threadId, "I have completed the authentication in the browser. Please continue.") {}
                }
                SecondaryButton("Cancel") { state.bots.firstOrNull { it.id == entry.botId }?.let { store.stopBot(it) } }
            }
        } else {
            Text(if (status == "resolved") "Authentication completed" else "Cancelled", style = Type.caption, color = Palette.tertiary)
        }
    }
}

/** Chief of Staff automatic intervention / escalation card (INIC-04). */
@Composable internal fun ChiefIncidentCard(entry: Entry, state: AppState, store: AppStore) {
    val inc = entry.data["chiefIncident"] as? JsonObject ?: return
    val workerId = inc.text("workerBotId").orEmpty()
    val workerName = state.bots.firstOrNull { it.id == workerId }?.name ?: workerId
    val reason = inc.text("reason").orEmpty()
    val summary = inc.text("summary").orEmpty()
    val attempt = inc["retryAttempt"]?.jsonPrimitive?.intOrNull ?: 1
    val escalated = inc.text("status") == "escalated" || attempt >= 2
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("shield.lefthalf.filled", size = 16.dp, tint = if (escalated) Palette.danger else Palette.warning)
            Text(if (escalated) "Chief of Staff · Escalated Incident" else "Chief of Staff · Intervention (Attempt $attempt/2)",
                style = Type.chatSubheadline.semibold, color = if (escalated) Palette.danger else Palette.warning)
        }
        Text("Worker: $workerName · Reason: $reason", style = Type.chatFootnote, color = Palette.secondary)
        if (summary.isNotEmpty()) Text(summary, style = Type.chatFootnote, color = Palette.text)
        if (escalated) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton("Retry manually") {
                    store.sendDraft(entry.botId, entry.threadId, "Retry assignment for $workerName") {}
                }
                SecondaryButton("Stop worker") { state.bots.firstOrNull { it.id == workerId }?.let { store.stopBot(it) } }
            }
        }
    }
}
