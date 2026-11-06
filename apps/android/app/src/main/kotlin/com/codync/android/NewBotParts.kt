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
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotCatalog
import com.codync.android.core.BotDraft
import com.codync.android.core.BotProfile
import com.codync.android.core.LinkState
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/** What the bot is for: its standing instructions, in plain words (desktop `PurposeField`). */
@Composable internal fun PurposeField(value: String, onChange: (String) -> Unit) {
    Field("What should this bot help with?") {
        FieldBox(value, onChange, "Plan trips, compare options or help with daily work.", 2, 6, fill = Palette.surface)
    }
}

/** Why the picked agent can't answer yet, if it can't. */
internal fun agentNotice(backend: JsonObject?): String? = when {
    backend == null -> null
    !backend.available -> "Not installed on this computer. Set it up in Extensions."
    backend["signedIn"]?.jsonPrimitive?.booleanOrNull == false -> "Not signed in. The bot asks you to sign in before its first message."
    else -> null
}

/** The agent and its model, with the agent's state when it isn't ready (desktop `AgentAndModel`). */
@Composable internal fun AgentAndModel(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    val all = backends(state)
    val agents = all.filter { it.available || it.curated }.map { info ->
        val id = info.text("id").orEmpty()
        id to ((info.text("name") ?: id) + if (!info.available) " (not installed)" else "")
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        ValueRow("Agent") {
            ChoicePicker(draft.backend, agents, { if (it != draft.backend) change(draft.withAgent(it).copy(account = null)) }, label = "Agent")
        }
        val info = all.firstOrNull { it.text("id") == draft.backend }
        agentNotice(info)?.let { Text(tr(it), style = Type.caption, color = Palette.warning) }
        if (info?.get("accounts")?.jsonPrimitive?.booleanOrNull != false) {
            AgentAccountPicker(draft.backend, info?.text("name") ?: draft.backend, draft.account, state.actualConnection is LinkState.Ready,
                "${state.generation}/${state.computer?.id}/${draft.id}",
                true,
                info?.get("signOut")?.jsonPrimitive?.booleanOrNull == true,
                { store.agentAccounts(draft.backend) }, { store.authenticateAgentAccount(draft.backend) },
                { store.deleteAgentAccount(it) }, { store.signOutAgent(draft.backend) }) {
                change(draft.copy(account = it, model = null, modelOptions = emptyMap()))
            }
        }
        key(state.generation, state.computer?.id, draft.backend) {
            AgentModelPicker(state, { backend -> store.agentModels(backend, draft.account) }, draft.backend, draft.model, draft.modelOptions, false, null,
                { model, options -> change(draft.copy(model = model, modelOptions = options)) }, null)
        }
    }
}

/** The selected computer's catalog; an older host without one leaves it empty. */
@Composable internal fun rememberCatalog(state: AppState, store: AppStore): BotCatalog? {
    var catalog by remember(state.computer?.id) { mutableStateOf<BotCatalog?>(null) }
    val online = state.actualConnection is LinkState.Ready
    LaunchedEffect(state.computer?.id, online) {
        if (!online) return@LaunchedEffect
        catalog = try { store.botCatalog() } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { null }
    }
    return catalog
}

/** Ready-made starting points: picking one fills the name, the look and the instructions (desktop `Suggestions`). */
@Composable internal fun Suggestions(profiles: List<BotProfile>, pick: (BotProfile) -> Unit) {
    if (profiles.isEmpty()) return
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr("Suggestions"), style = Type.footnote.semibold, color = Palette.secondary)
        for (profile in profiles) {
            Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).plainButton { pick(profile) }
                .padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                CharacterAvatar(profile.avatarShape, profile.avatarColor, 32.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(profile.name, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(profile.summary, style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}
