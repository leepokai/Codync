package com.codync.android

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.lifecycle.lifecycleScope
import com.codync.android.core.WidgetFeed
import com.codync.android.core.WidgetKind
import com.codync.android.core.teamSlots
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch

/**
 * The launcher's Edit widget page (iOS Edit Widget): the provider a usage widget follows, or who
 * sits in each spot of the Team widget. It follows the account selected in Codync.
 */
class WidgetConfigurationActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val result = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)
        setResult(RESULT_CANCELED, result)
        val kind = when (AppWidgetManager.getInstance(this).getAppWidgetInfo(id)?.provider) {
            ComponentName(this, ProviderWidgetReceiver::class.java) -> WidgetKind.Provider
            ComponentName(this, TeamWidgetReceiver::class.java) -> WidgetKind.Team
            else -> { finish(); return }
        }
        enableEdgeToEdge()
        setContent {
            CodyncTheme {
                OverlayRoot {
                    val revision by WidgetStore.changes.collectAsState()
                    val feed = remember(revision) { WidgetStore.feed(this) }
                    var busy by remember { mutableStateOf(false) }
                    var error by remember { mutableStateOf<String?>(null) }
                    var provider by rememberSaveable { mutableStateOf(WidgetStore.provider(this, id)) }
                    var team by rememberSaveable { mutableStateOf(WidgetStore.team(this, id)) }
                    Column(Modifier.fillMaxSize().background(Palette.background).safeDrawingPadding()) {
                        ScreenHeader({ IconButton("Cancel", "xmark") { finish() } },
                            { Text(if (kind == WidgetKind.Team) "Team" else "Provider usage", style = Type.body.semibold, color = Palette.text) })
                        CardForm(Modifier.weight(1f)) {
                            Preview(feed, kind, provider, team)
                            CardSection(footer = if (kind == WidgetKind.Team) "Empty spots show the bots that need you first." else "Follows the account selected in Codync.") {
                                row { ValueRow("Computer", feed.computerName.ifBlank { "No computer yet" }) }
                                if (kind == WidgetKind.Provider) row {
                                    val choices = (feed.usage.providers.map { it.id to it.name } + listOf("claude" to "Claude", "codex" to "Codex")).distinctBy { it.first }
                                    ValueRow("Provider") { ChoicePicker(provider, choices, { provider = it }, label = "Provider") }
                                } else for ((spot, label) in listOf("Top left", "Top right", "Bottom left", "Bottom right").withIndex()) row {
                                    val choices = listOf("" to "Automatic") + feed.bots.map { it.key to it.name }
                                    ValueRow(label) {
                                        ChoicePicker(team[spot].orEmpty(), choices, { pick -> team = team.toMutableList().also { it[spot] = pick.ifEmpty { null } } },
                                            label = label)
                                    }
                                }
                            }
                            error?.let { Text(it, style = Type.footnote, color = Palette.danger) }
                            PrimaryButton(if (busy) "Saving…" else "Save widget", Modifier.fillMaxWidth(), enabled = !busy) {
                                busy = true
                                lifecycleScope.launch {
                                    try {
                                        if (kind == WidgetKind.Team) WidgetStore.selectTeam(this@WidgetConfigurationActivity, id, team)
                                        else WidgetStore.selectProvider(this@WidgetConfigurationActivity, id, provider)
                                        val glance: GlanceAppWidget = if (kind == WidgetKind.Team) TeamWidget() else ProviderWidget()
                                        glance.update(this@WidgetConfigurationActivity, GlanceAppWidgetManager(this@WidgetConfigurationActivity).getGlanceIdBy(id))
                                        setResult(RESULT_OK, result); finish()
                                    } catch (_: Exception) { error = "Couldn't update the widget. Try again."; busy = false }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The widget as it will look, on the latest report. */
@Composable private fun Preview(feed: WidgetFeed, kind: WidgetKind, provider: String, team: List<String?>) {
    Box(Modifier.fillMaxWidth().clip(RoundedRectangle(26.dp)).background(Palette.bubbleAgent).padding(14.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.width(if (kind == WidgetKind.Team) 158.dp else 320.dp).height(158.dp).clip(RoundedRectangle(22.dp)).background(Palette.surface).padding(14.dp)) {
            if (kind == WidgetKind.Team) BotsTeamCard(teamSlots(feed.bots, team))
            else feed.usage.providers.firstOrNull { it.id == provider }?.let { ProviderWidgetCard(it, ProviderLayout.Medium) }
                ?: Text("No usage yet. Open Codync to refresh.", style = Type.footnote, color = Palette.secondary)
        }
    }
}
