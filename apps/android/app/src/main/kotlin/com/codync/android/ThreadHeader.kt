package com.codync.android

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/**
 * Android-style conversation header:
 * - Left: Back button delegating to the existing roster navigation stack (`store.openBot(null)`).
 * - Middle (`weight(1f)`): Bot/group avatar + left-aligned name + computer/route subtitle; tapping
 *   opens the bot/group inspector (`edit(bot)`). Long names truncate without displacing actions.
 * - Right: Remote screen button, adaptive secondary action (Routines when width and font scale allow),
 *   and overflow menu (`${bot.name} actions`).
 */
@Composable internal fun ThreadHeader(bot: Bot, state: AppState, store: AppStore, edit: (Bot) -> Unit, trace: () -> Unit,
    routines: () -> Unit, newSession: () -> Unit, delete: () -> Unit, search: () -> Unit) {
    val density = LocalDensity.current
    val host = state.computer?.name.orEmpty()
    BoxWithConstraints(Modifier.fillMaxWidth().statusBarsPadding().padding(top = 6.dp).padding(horizontal = 12.dp).height(48.dp)) {
        val showInlineRoutines = bot.kind != "group" && maxWidth >= 380.dp && density.fontScale <= 1.15f
        Row(
            Modifier.fillMaxSize(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val (backSource, backPressed) = rememberPress()
            Box(
                Modifier.size(44.dp).glass(Capsule(), pressed = backPressed)
                    .clickable(backSource, null, role = Role.Button) { store.openBot(null) }
                    .semantics { contentDescription = "Back" },
                contentAlignment = Alignment.Center,
            ) {
                Symbol("chevron.left", size = 22.dp, tint = Palette.text)
            }
            val (identitySource, identityPressed) = rememberPress()
            Row(
                Modifier.weight(1f)
                    .height(44.dp)
                    .pressScale(identityPressed)
                    .clip(RoundedRectangle(14.dp))
                    .clickable(identitySource, null, role = Role.Button) { edit(bot) }
                    .semantics { contentDescription = "${bot.name} details" }
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                BotAvatar(bot, state.bots, 32.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                    Text(bot.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    ConnectionSubtitle(state)
                }
            }
            val screen = state.screen
            if (screen != null) {
                val operating = screen.agentBot == bot.id
                ComputerButton(operating, Modifier) { store.openScreen(bot.id.takeIf { operating }) }
            }
            if (showInlineRoutines) {
                val (routineSource, routinePressed) = rememberPress()
                Box(
                    Modifier.size(44.dp).glass(Capsule(), pressed = routinePressed)
                        .clickable(routineSource, null, role = Role.Button, onClick = routines)
                        .semantics { contentDescription = "Open routines" },
                    contentAlignment = Alignment.Center,
                ) {
                    Symbol("clock.arrow.circlepath", size = 20.dp, tint = Palette.text)
                }
            }
            val (searchSource, searchPressed) = rememberPress()
            Box(
                Modifier.size(44.dp).glass(Capsule(), pressed = searchPressed)
                    .clickable(searchSource, null, role = Role.Button, onClick = search)
                    .semantics { contentDescription = "Search conversation" },
                contentAlignment = Alignment.Center,
            ) {
                Symbol("magnifyingglass", size = 20.dp, tint = Palette.text)
            }
            DropdownMenu({ buildList {
                add(MenuItem("$host · ${connectionLabel(state)}", enabled = false))
                add(MenuItem("Reconnect", icon = "arrow.clockwise") { store.reconnect() })
                add(MenuItem("Details", icon = "info.circle", divider = true) { edit(bot) })
                add(MenuItem("Full conversation", icon = "list.bullet.rectangle", action = trace))
                val pin = MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = "pin") {
                    store.command("updateBot", kotlinx.serialization.json.buildJsonObject {
                        put("id", kotlinx.serialization.json.JsonPrimitive(bot.id)); put("pinned", kotlinx.serialization.json.JsonPrimitive(!bot.pinned))
                    }, "roster:" + bot.id)
                }
                if (bot.kind == "group") {
                    add(MenuItem("Edit group", icon = "person.2") { edit(bot) }); add(pin)
                    add(MenuItem("Delete group", icon = "trash", destructive = true, divider = true, action = delete))
                } else {
                    add(MenuItem("Edit profile", icon = "pencil") { edit(bot) }); if (!bot.lead) add(pin)
                    add(MenuItem("Routines", icon = "clock.arrow.circlepath", action = routines))
                    add(MenuItem("New session", icon = "arrow.counterclockwise", action = newSession))
                    if (!bot.lead) add(MenuItem("Delete bot", icon = "trash", destructive = true, divider = true, action = delete))
                }
            } }, Modifier.size(44.dp).glass(Capsule()), label = "${bot.name} actions") {
                Symbol("ellipsis", size = 21.dp, tint = Palette.text)
            }
        }
    }
}

@Composable private fun ComputerButton(operating: Boolean, modifier: Modifier, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val pulse by rememberInfiniteTransition(label = "Operating").animateFloat(1f, 0.35f,
        infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "Pulse")
    Box(modifier.size(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = action).semantics { contentDescription = "Computer" },
        contentAlignment = Alignment.Center) {
        Symbol("desktopcomputer", Modifier.graphicsLayer { alpha = if (operating) pulse else 1f }, size = 22.dp, tint = Palette.text)
    }
}

/** The link to the computer: route icon (or a spinner), computer name when known, and connection state. */
@Composable private fun ConnectionSubtitle(state: AppState) {
    val host = state.computer?.name.orEmpty()
    val statusText = if (host.isNotEmpty()) "$host · ${connectionLabel(state)}" else connectionLabel(state)
    Row(
        Modifier.clearAndSetSemantics { contentDescription = "${state.hostName}, ${connectionDescription(state)}" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Box(Modifier.width(12.dp), contentAlignment = Alignment.Center) {
            when (val link = state.connection) {
                LinkState.Connecting -> Spinner(8.dp)
                is LinkState.Ready -> Symbol(if (link.route == HostRoute.Relay) "cloud" else "wifi", size = 11.dp, tint = Palette.secondary)
                else -> Symbol("wifi.slash", size = 11.dp, tint = Palette.secondary)
            }
        }
        Text(statusText, style = Type.system(11f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}
