package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.LinkState
import com.codync.android.core.ArtifactCard
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/**
 * An artifact a bot published, in its chat (docs/features/artifacts.md): the top of the artifact's newest version, small and not
 * interactive, fading out over the artifact symbol, its title and "Artifact". `modifier` carries the card's tap and menu.
 */
@Composable internal fun ArtifactCardView(card: ArtifactCard, botId: String, state: AppState, store: AppStore, modifier: Modifier) {
    val artifacts = state.artifacts
    val newest = artifacts.newest(card.id, card.updatedAt)
    val online = state.actualConnection is LinkState.Ready
    LaunchedEffect(card.id, newest, online) { store.loadArtifact(botId, card.id, newest) }
    val html = artifacts.html[card.id]?.text
    // The artifact crashed its renderer: the card keeps its footer.
    var gone by remember(card.id) { mutableStateOf(false) }
    Column(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent)) {
        BoxWithConstraints(Modifier.fillMaxWidth().height(PREVIEW_HEIGHT).clipToBounds(), contentAlignment = Alignment.Center) {
            when {
                gone -> Unit
                html != null -> ArtifactWebView(card.id, html, interactive = false, gone = { gone = true },
                    Modifier.wrapContentSize(Alignment.TopStart, unbounded = true)
                        .requiredSize(maxWidth / PREVIEW_SCALE, PREVIEW_HEIGHT / PREVIEW_SCALE)
                        .graphicsLayer { scaleX = PREVIEW_SCALE; scaleY = PREVIEW_SCALE; transformOrigin = TransformOrigin(0f, 0f) })
                card.id !in artifacts.failures -> Spinner(16.dp)
            }
            // The fade, and a cover that keeps touches off the artifact: they open the card or its menu.
            Box(Modifier.matchParentSize().pointerInput(Unit) {}
                .background(Brush.verticalGradient(0.5f to Color.Transparent, 1f to Palette.bubbleAgent)))
        }
        Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 14.dp, bottom = 12.dp), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Symbol("chart.bar.doc.horizontal", size = 22.dp, tint = Palette.secondary)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(artifacts.title(card), style = Type.subheadline.semibold, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(tr("Artifact"), style = Type.caption2, color = Palette.secondary)
            }
        }
    }
}

private val PREVIEW_HEIGHT = 168.dp
/** The artifact lays out twice the card's width, then shows at half size: the top of a wide artifact, not a zoomed-in corner. */
private const val PREVIEW_SCALE = 0.5f
