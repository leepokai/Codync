package com.codync.android.design

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import com.kyant.backdrop.Backdrop
import com.kyant.backdrop.backdrops.emptyBackdrop
import com.kyant.backdrop.backdrops.layerBackdrop
import com.kyant.backdrop.backdrops.rememberLayerBackdrop
import com.kyant.backdrop.drawBackdrop
import com.kyant.backdrop.drawPlainBackdrop
import com.kyant.backdrop.effects.blur
import com.kyant.backdrop.effects.lens
import com.kyant.backdrop.effects.vibrancy
import com.kyant.backdrop.highlight.Highlight
import com.kyant.backdrop.shadow.Shadow
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import com.kyant.shapes.RoundedRectangularShape
import kotlin.math.min

/** What Liquid Glass above refracts: the nearest `GlassStage`'s content. */
val LocalBackdrop = staticCompositionLocalOf<Backdrop> { emptyBackdrop() }

/**
 * Content with glass chrome floating over it (iOS 26 bars over scroll content). `content` is
 * recorded so `chrome`'s glass can refract and blur it; chrome itself is never recorded.
 */
@Composable fun GlassStage(modifier: Modifier = Modifier, chrome: @Composable BoxScope.() -> Unit = {},
    content: @Composable BoxScope.() -> Unit) {
    val ground = Palette.background
    val onDraw: ContentDrawScope.() -> Unit = remember(ground) { { drawRect(ground); drawContent() } }
    val backdrop = rememberLayerBackdrop(onDraw = onDraw)
    Box(modifier) {
        Box(Modifier.fillMaxSize().layerBackdrop(backdrop), content = content)
        CompositionLocalProvider(LocalBackdrop provides backdrop) { Box(Modifier.fillMaxSize(), content = chrome) }
    }
}

/** Which bar a scroll edge sits under. */
enum class Edge { Top, Bottom }

/**
 * iOS 26's soft scroll edge under a bar: content passing beneath blurs and washes into the
 * background, strongest at the screen's edge, so the bar stays legible over it. Chrome of a
 * `GlassStage`, like the bar it sits under.
 */
@Composable fun ScrollEdge(edge: Edge, height: Dp, modifier: Modifier = Modifier) {
    val backdrop = LocalBackdrop.current
    val ground = Palette.background
    val top = edge == Edge.Top
    Box(modifier.fillMaxWidth().height(height)
        .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
        .drawWithContent {
            drawContent()
            // The blur and the wash both fade out toward the content.
            drawRect(Brush.verticalGradient(*if (top) arrayOf(0f to Color.Black, 0.8f to Color.Black, 1f to Color.Transparent)
                else arrayOf(0f to Color.Transparent, 0.2f to Color.Black, 1f to Color.Black)), blendMode = BlendMode.DstIn)
        }
        .drawPlainBackdrop(backdrop, shape = { RoundedRectangle(0.dp) }, effects = { blur(10.dp.toPx()) }, onDrawSurface = {
            val wash = listOf(ground.copy(alpha = 0.85f), ground.copy(alpha = 0.45f))
            drawRect(Brush.verticalGradient(if (top) wash else wash.reversed()))
        }))
}

/** How strongly glass frosts what's behind it. */
enum class GlassStyle { Regular, Clear, Menu }

/**
 * iOS 26 Liquid Glass: blurred, refracted at the rim, lit along the edge, lifted by a soft shadow.
 * `pressed` swells it the way `.glassEffect(.regular.interactive())` does.
 */
@Composable fun Modifier.glass(shape: RoundedRectangularShape = Capsule(), style: GlassStyle = GlassStyle.Regular,
    tint: Color = Color.Unspecified, pressed: Boolean = false, shadow: Boolean = true): Modifier {
    val backdrop = LocalBackdrop.current
    val dark = LocalDark.current
    val surface = when {
        tint.isSpecified -> tint
        style == GlassStyle.Clear -> if (dark) Color(0x33000000) else Color(0x33FFFFFF)
        style == GlassStyle.Menu -> if (dark) Color(0xB8262626) else Color(0xC7FAFAFA)
        dark -> Color(0xA8222222)
        else -> Color(0xC2FFFFFF)
    }
    val blurRadius = if (style == GlassStyle.Menu) 22.dp else 4.dp
    val swell by animateFloatAsState(if (pressed) 1.06f else 1f, Motion.layout(), label = "Glass press")
    return graphicsLayer { scaleX = swell; scaleY = swell }.drawBackdrop(
        backdrop = backdrop,
        shape = { shape },
        effects = {
            vibrancy()
            blur(blurRadius.toPx())
            val rim = min(size.minDimension * 0.3f, 18.dp.toPx())
            lens(rim, rim * 2f)
        },
        highlight = { Highlight(width = 0.75.dp, alpha = if (dark) 0.45f else 0.8f) },
        shadow = if (!shadow) null else {
            { Shadow(radius = 18.dp, offset = DpOffset(0.dp, 4.dp), color = Color.Black.copy(alpha = if (dark) 0.32f else 0.09f)) }
        },
        onDrawSurface = { drawRect(surface) },
    )
}

/** A plain frosted layer over the stage (a context menu's backdrop): blur and a tint, no rim. */
@Composable fun Modifier.frost(radius: Dp, tint: Color): Modifier {
    val backdrop = LocalBackdrop.current
    return drawPlainBackdrop(backdrop, shape = { RoundedRectangle(0.dp) }, effects = { blur(radius.toPx()) },
        onDrawSurface = { drawRect(tint) })
}

/** Tracks a press for glass that swells under the finger. */
@Composable fun rememberPress(): Pair<MutableInteractionSource, Boolean> {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    return source to pressed
}
