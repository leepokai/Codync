package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.codync.android.core.InstalledConnector
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private data class Removal(val id: String, val name: String, val skill: Boolean)

/** What's installed on the computer: connectors (sign in, remove) and skills (details, remove) (iOS `InstalledView`). */
@Composable internal fun InstalledView(
    state: AppState,
    store: AppStore,
    inModal: Boolean = true,
    bottomInset: Dp = 0.dp,
    back: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var removing by remember { mutableStateOf<Removal?>(null) }
    var signingIn by remember { mutableStateOf<String?>(null) }
    var viewingSkill by remember { mutableStateOf<SkillRef?>(null) }
    var tools by remember(state.generation) { mutableStateOf<InstalledConnector?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val connectors = state.plugins?.connectors.orEmpty()
    val skills = state.plugins?.skills.orEmpty()
    BackHandler(onBack = back)
    fun run(work: suspend () -> Unit) = scope.launch {
        error = null
        try { work() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "That didn't go through." }
    }
    Column(Modifier.fillMaxSize().background(Palette.background).then(if (inModal) Modifier else Modifier.statusBarsPadding())) {
        ScreenHeader({ BackButton(back) }, { Text("Connectors & skills", style = Type.body.semibold, color = Palette.text) })
        CardForm(
            Modifier.weight(1f),
            padding = PaddingValues(start = 20.dp, top = 20.dp, end = 20.dp, bottom = 20.dp + bottomInset),
        ) {
            if (connectors.isEmpty() && skills.isEmpty()) MarketEmpty("shippingbox", "Nothing installed", "Connectors and skills you add show up here.")
            if (connectors.isNotEmpty()) CardSection("Connectors") {
                for (c in connectors) row {
                    InstalledRow(c.name, if (c.needsSignIn) "Sign in so bots can use it" else c.command ?: c.url ?: c.description, icon = {
                        if (c.kind == "composio") AppLogo(c.logo, c.name, 32.dp) else ServiceLogo(null, c.name, c.registryName, 32.dp)
                    }, accessory = {
                        SecondaryButton("Tools & requests") { tools = c }
                        if (signingIn == c.id) Spinner()
                        else if (c.needsSignIn) PrimaryButton("Sign in") {
                            signingIn = c.id
                            run { try { if (!store.signInConnector(c.id)) throw IOException("Sign-in to ${c.name} wasn't confirmed.") } finally { signingIn = null } }
                        }
                    }) { removing = Removal(c.id, c.name, skill = false) }
                }
            }
            if (skills.isNotEmpty()) CardSection("Skills") {
                for (s in skills) row {
                    InstalledRow(s.name, s.description, { viewingSkill = SkillRef(s.id, s.name, s.description) },
                        icon = { Box(Modifier.size(32.dp)) { SkillGlyph() } }) { removing = Removal(s.id, s.name, skill = true) }
                }
            }
            error?.let { ErrorRow(it) }
        }
    }
    val target = removing
    CodyncDialog(tr("Remove {name}?").replace("{name}", target?.name.orEmpty()), target != null, { removing = null },
        if (target?.skill == true) "Bots stop using this skill." else "Bots lose this connector, and the keys saved for it are deleted.") {
        listOf(DialogAction("Remove", destructive = true) { target?.let { r -> run { store.removePlugin(if (r.skill) "removeSkill" else "removeConnector", r.id) } } })
    }
    CodyncSheet(viewingSkill, { viewingSkill = null }) { SkillSheet(it, state, store) }
    CodyncSheet(tools, { tools = null }) { McpToolsSheet(it, state, store) }
}

@Composable private fun InstalledRow(title: String, subtitle: String, open: (() -> Unit)? = null, icon: @Composable () -> Unit,
    accessory: @Composable () -> Unit = {}, remove: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(Modifier.weight(1f).then(if (open != null) Modifier.plainButton(action = open) else Modifier), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            icon()
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(title, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(tr(subtitle), style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        accessory()
        IconButton("Remove $title", "trash", action = remove)
    }
}
