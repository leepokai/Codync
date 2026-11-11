package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.MarketAgent
import com.codync.android.core.MarketAgentDetail
import com.codync.android.core.MarketAgentRoutine
import com.codync.android.core.MarketApp
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

// A marketplace agent's and app's pages (iOS `MarketAgentSheet`, `AppSheet`).

private val WEEKDAYS = listOf("Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays")

/** `Weekdays at 09:00`, `Mondays at 08:49`, `Day 1 of each month at 09:00`. */
@Composable private fun routineWhen(routine: MarketAgentRoutine): String {
    val time = routine.schedule.time.orEmpty()
    return when (routine.schedule.kind) {
        "weekdays" -> tr("Weekdays at {time}").replace("{time}", time)
        "weekly" -> "${WEEKDAYS[(routine.schedule.weekday ?: 1).coerceIn(0, 6)]} $time"
        "monthly" -> tr("Day {day} of each month at {time}").replace("{day}", "${routine.schedule.day ?: 1}").replace("{time}", time)
        else -> tr("Daily at {time}").replace("{time}", time)
    }
}

/** An agent's page: its title, the skills it brings, its routines and its instructions. */
@Composable internal fun MarketAgentSheet(agent: MarketAgent, state: AppState, store: AppStore, added: Boolean,
    onAdded: () -> Unit, onSkill: (String, String, String) -> Unit) {
    val scope = rememberCoroutineScope()
    var detail by remember(agent.id) { mutableStateOf<MarketAgentDetail?>(null) }
    var error by remember(agent.id) { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val installed = added || agent.installed
    LaunchedEffect(agent.id) {
        try { detail = store.marketAgent(agent.id) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load this agent." }
    }
    fun add() {
        busy = true
        scope.launch {
            try { store.installAgent(agent.id) { onAdded() } }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't add this agent." }
            finally { busy = false }
        }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(agent.name) {
            if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else if (!installed) IconButton("Add agent", "plus") { add() }
            else Symbol("checkmark", size = 18.dp, tint = Palette.secondary, contentDescription = tr("Added"))
        }
        CardForm(Modifier.weight(1f)) {
            CardSection {
                row {
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(agent.title, style = Type.body, color = Palette.text)
                            Text(tr(if (installed) "Added" else "Not added"), style = Type.caption,
                                color = Palette.secondary, modifier = Modifier.pill())
                        }
                        Text(tr("Created by another OpenBot user. OpenBot reviewed it before publishing. Once added, it can act on your behalf."),
                            style = Type.caption, color = Palette.secondary)
                    }
                }
            }
            val page = detail
            if (page != null) {
                if (page.skills.isNotEmpty()) CardSection("Skills") {
                    for (skill in page.skills) row {
                        Row(Modifier.fillMaxWidth().plainButton { onSkill(skill.name, skill.description, skill.id) }
                            .padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            SkillGlyph()
                            Column(Modifier.weight(1f)) {
                                Text(skill.name, style = Type.body, color = Palette.text)
                                Text(skill.description, style = Type.caption, color = Palette.secondary, maxLines = 2)
                            }
                            Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                        }
                    }
                }
                if (page.routines.isNotEmpty()) CardSection("Routines") {
                    for (routine in page.routines) row {
                        Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            TileIcon("calendar.badge.clock")
                            Column(Modifier.weight(1f)) {
                                Text(routine.name, style = Type.body, color = Palette.text)
                                Text("${routineWhen(routine)} · ${if (routine.active) tr("Active") else tr("Inactive")}",
                                    style = Type.caption, color = Palette.secondary)
                            }
                        }
                    }
                }
                CardSection("Information") {
                    row { Text(page.instructions, style = Type.body, color = Palette.text) }
                    row {
                        Text(tr("Origin") + ": " + tr("Reviewed by OpenBot"), style = Type.caption, color = Palette.secondary)
                    }
                    row {
                        Text(tr("By {name} · {category} · version {version} · updated {updated}")
                            .replace("{name}", agent.creator).replace("{category}", tr(MarketCategories.label(agent.category)))
                            .replace("{version}", "${agent.version}").replace("{updated}", agent.updated),
                            style = Type.caption, color = Palette.secondary)
                    }
                }
            }
            if (error != null) CardSection { row { ErrorRow(error!!) } }
        }
    }
}

/** An app's page: what it does, its setup (steps, a key or a sign-in) and example prompts. */
@Composable internal fun MarketAppSheet(app: MarketApp, state: AppState, store: AppStore, added: Boolean, onAdded: () -> Unit) {
    val scope = rememberCoroutineScope()
    var values by remember(app.id) { mutableStateOf(emptyMap<String, String>()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember(app.id) { mutableStateOf<String?>(null) }
    val installed = added || app.installed
    val missing = app.fields.any { (it.required || it.url) && values[it.id].isNullOrBlank() }
    fun add() {
        busy = true; error = null
        scope.launch {
            try { store.installApp(app, values); onAdded() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't add this app." }
            finally { busy = false }
        }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(app.name) {
            if (busy) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else if (!installed) IconButton(if (app.setup == "link") "Connect" else "Add app", "plus", enabled = !missing) { add() }
            else Symbol("checkmark", size = 18.dp, tint = Palette.secondary, contentDescription = tr("Added"))
        }
        CardForm(Modifier.weight(1f)) {
            CardSection {
                row {
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(app.tagline, style = Type.body, color = Palette.text)
                        Text(app.description, style = Type.body, color = Palette.secondary)
                        app.origin?.let { WebLink("${tr("Origin")}: ${app.creator}", it, Type.footnote) }
                        app.website?.let { WebLink(it, it, Type.footnote) }
                    }
                }
            }
            if (app.setup == "local" && app.steps.isNotEmpty()) CardSection("Setup") {
                for ((index, step) in app.steps.withIndex()) row { Text("${index + 1}. $step", style = Type.body, color = Palette.text) }
                app.docsUrl?.let { url -> row { WebLink(app.docsLabel ?: tr("Open the guide"), url, Type.footnote) } }
            }
            if (app.setup == "key") CardSection("Setup") {
                for (field in app.fields) row {
                    NotedInput(values[field.id].orEmpty(), { values = values + (field.id to it) },
                        field.placeholder ?: field.label, field.hint, secret = !field.url)
                }
                app.docsUrl?.let { url -> row { WebLink(app.docsLabel ?: tr("Get a key"), url, Type.footnote) } }
            }
            if (app.setup == "link" || app.setup == "open") CardSection("Setup") {
                row {
                    Text(if (app.setup == "link") tr("{name} asks you to sign in to your own account in the browser.").replace("{name}", app.name)
                        else tr("Nothing to fill in here."), style = Type.body, color = Palette.secondary)
                }
            }
            if (app.prompts.isNotEmpty()) CardSection("Try it") {
                for (prompt in app.prompts) row {
                    Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        TileIcon("text.bubble")
                        Text(prompt.text, Modifier.weight(1f), style = Type.body, color = Palette.text)
                    }
                }
            }
            if (error != null) CardSection { row { ErrorRow(error!!) } }
        }
    }
}
