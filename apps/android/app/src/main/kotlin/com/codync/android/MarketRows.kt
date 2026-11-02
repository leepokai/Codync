package com.codync.android

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import coil3.compose.AsyncImage
import com.codync.android.core.AgentBackend
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

// The Extensions page's building blocks (iOS `MarketRows`): rows, agent cards and logos.

/** One catalog item: its icon, name and line, then Add (or a check once added). */
@Composable internal fun MarketRow(title: String, subtitle: String, added: Boolean, busy: Boolean = false, addLabel: String = "Add",
    open: (() -> Unit)? = null, icon: @Composable () -> Unit, add: () -> Unit) {
    // The whole row opens the item's details; without it the row does what its button does.
    val rowAction = open ?: if (added || busy) null else add
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).then(if (rowAction != null) Modifier.plainButton(action = rowAction) else Modifier)
        .padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Box(Modifier.size(46.dp), contentAlignment = Alignment.Center) { icon() }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(title), style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(subtitle), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        when {
            busy -> Box(Modifier.width(60.dp), contentAlignment = Alignment.Center) { ThinkingOrb(size = 18.dp, color = Palette.secondary) }
            added -> Box(Modifier.width(60.dp), contentAlignment = Alignment.Center) {
                Symbol("checkmark", size = 16.dp, tint = Palette.secondary, contentDescription = tr("Added"))
            }
            else -> Text(tr(addLabel), Modifier.clip(Capsule()).background(Palette.bubbleAgent).plainButton(action = add)
                .padding(horizontal = 16.dp, vertical = 7.dp), style = Type.subheadline.semibold, color = Palette.text)
        }
    }
}

/** An agent in the Agents grid: its logo on a tile, its name and where it stands. */
@Composable internal fun AgentCard(backend: AgentBackend, modifier: Modifier, action: () -> Unit) {
    val status = when {
        !backend.installed && backend.curated -> "Not installed"
        backend.needsNode -> "Needs Node.js"
        !backend.installed -> "Sets up on first use"
        backend.signedIn == false -> "Not signed in"
        else -> "Installed"
    }
    Column(modifier.clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent.copy(alpha = 0.45f)).plainButton(action = action)
        .semantics(mergeDescendants = true) {}.padding(horizontal = 10.dp, vertical = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(64.dp).clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
            AgentIcon(backend.registry ?: backend.id, 34.dp)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(backend.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(status), style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** A coding agent's logo by ACP registry id, in its brand color when it has one; a terminal glyph otherwise. */
@Composable internal fun AgentIcon(registry: String?, size: Dp) {
    val tint = AGENT_TINTS[registry]?.let { Color(it) } ?: Palette.text
    val res = agentIconRes(registry)
    if (res != null) Image(painterResource(res), null, Modifier.size(size).clearAndSetSemantics {}, colorFilter = ColorFilter.tint(tint))
    else Symbol("terminal", Modifier.padding(size * 0.1f), size = size * 0.8f, tint = tint)
}

// Registry logos are one color; these carry their brand's (a gradient's middle for Gemini and omp).
private val AGENT_TINTS = mapOf("claude-acp" to 0xFFD97857, "gemini" to 0xFF9178C7, "antigravity-acp" to 0xFF4285F5,
    "mistral-vibe" to 0xFFFA520F, "qwen-code" to 0xFF615CED, "amp-acp" to 0xFFF24F40, "kiro" to 0xFF8F45FF,
    "cortex-code" to 0xFF29B5E8, "omp" to 0xFF9C4DFF, "dsh" to 0xFF4D6BFE)

/** Placeholder rows while a list loads, shaped like the real ones. */
@Composable internal fun SkeletonRows() {
    val dim by rememberInfiniteTransition(label = "Skeleton").animateFloat(1f, 0.5f, infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "Dim")
    val loading = tr("Loading…")
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(loading, style = Type.footnote, color = Palette.secondary)
        Column(Modifier.graphicsLayer { alpha = dim }.clearAndSetSemantics { contentDescription = loading }) {
            repeat(4) {
                Row(Modifier.padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.size(46.dp).clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent))
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(Modifier.size(120.dp, 12.dp).clip(Capsule()).background(Palette.bubbleAgent))
                        Box(Modifier.size(190.dp, 10.dp).clip(Capsule()).background(Palette.bubbleAgent))
                    }
                }
            }
        }
    }
}

/** A service's logo from its website favicon, on a white tile; its initial otherwise. */
@Composable internal fun ServiceLogo(website: String?, name: String, registryName: String? = null, size: Dp = 46.dp, site: String? = null) {
    val domain = site ?: serviceDomain(website, name, registryName)
    var loaded by remember(domain) { mutableStateOf(false) }
    val shape = RoundedRectangle(size * 0.26f)
    Box(Modifier.size(size).clip(shape).background(if (loaded) Color.White else Palette.bubbleAgent).clearAndSetSemantics {},
        contentAlignment = Alignment.Center) {
        if (!loaded) Text(name.firstOrNull()?.uppercase() ?: "?", style = Type.system(size.value * 0.4f, FontWeight.SemiBold), color = Palette.text)
        if (domain != null) AsyncImage("https://www.google.com/s2/favicons?domain=$domain&sz=128", null, imageLoader = LocalNetworkImages.current,
            modifier = Modifier.fillMaxSize().padding(size * 0.2f), contentScale = ContentScale.Fit, onSuccess = { loaded = true })
    }
}

/** MCP Registry names are reverse-DNS ("com.notion/mcp"): the owner's domain; a website's host otherwise. */
internal fun serviceDomain(website: String?, name: String, registryName: String?): String? {
    website?.let { it.toUri().host }?.takeIf { it != "github.com" }?.let { return it }
    val labels = registryName?.substringBefore('/')?.split('.') ?: return null
    // io.github.<user> is a GitHub account, not the service's own site.
    if (labels.size < 2 || labels[0] == "io" && labels[1] == "github") return if (name.contains("github", ignoreCase = true)) "github.com" else null
    return "${labels[1]}.${labels[0]}"
}

/** An app's logo from Composio, a letter tile until it loads. */
@Composable internal fun AppLogo(url: String?, name: String, size: Dp = 46.dp) {
    Box(Modifier.size(size).clip(RoundedRectangle(size * 0.28f)).background(Palette.bubbleAgent).clearAndSetSemantics {},
        contentAlignment = Alignment.Center) {
        Text(name.take(1).uppercase(), style = Type.system(size.value * 0.4f, FontWeight.SemiBold), color = Palette.secondary)
        if (url != null) AsyncImage(url, null, imageLoader = LocalNetworkImages.current,
            modifier = Modifier.fillMaxSize().background(Palette.bubbleAgent).padding(size * 0.18f), contentScale = ContentScale.Fit)
    }
}

@Composable internal fun TileIcon(systemImage: String) {
    Box(Modifier.fillMaxSize().clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
        Symbol(systemImage, size = 20.dp, tint = Palette.text)
    }
}

@Composable internal fun SkillGlyph() = TileIcon("book.pages")

/** Text that opens a web page (iOS `WebLink`). */
@Composable internal fun WebLink(title: String, url: String, style: androidx.compose.ui.text.TextStyle = Type.body) {
    val uri = LocalUriHandler.current
    var failed by remember { mutableStateOf(false) }
    val label = tr(title)
    Text(if (failed) "$label (couldn't open the browser)" else label,
        Modifier.plainButton { failed = runCatching { uri.openUri(url) }.isFailure }, style = style.copy(textDecoration = TextDecoration.Underline),
        color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

/** A quiet "nothing here" message. */
@Composable internal fun MarketEmpty(icon: String, title: String, detail: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(icon, size = 30.dp, tint = Palette.tertiary)
        Text(tr(title), style = Type.body.semibold, color = Palette.text)
        Text(tr(detail), style = Type.subheadline, color = Palette.secondary)
    }
}
