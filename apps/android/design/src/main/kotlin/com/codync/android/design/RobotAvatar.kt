package com.codync.android.design

import android.animation.ValueAnimator
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

val ROBOT_AVATAR_LOOKS: List<String> = listOf(
    "round", "visor", "dome", "boxy", "cyclops", "ears",
    "scout", "owl", "pixel", "astro", "cat", "hex", "plush"
)

fun isRobotLook(look: String?): Boolean = look != null && look in ROBOT_AVATAR_LOOKS

/**
 * 12 geometric robot head avatars with working-state spinner crossfade (Ideas 21 & 22).
 */
@Composable
fun RobotAvatar(
    look: String,
    color: String,
    size: Dp = 40.dp,
    working: Boolean = false,
) {
    val animEnabled = ValueAnimator.areAnimatorsEnabled()
    val workingProgress by animateFloatAsState(
        targetValue = if (working && animEnabled) 1f else 0f,
        animationSpec = tween(durationMillis = 220),
        label = "robotWorkingCrossfade",
    )
    val spinDeg = if (working && animEnabled) {
        val transition = rememberInfiniteTransition(label = "robotSpinner")
        val deg by transition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(tween(900, easing = LinearEasing)),
            label = "robotSpinnerAngle",
        )
        deg
    } else 0f

    val bg = AvatarPalette.tint(color)
    val radius = size * 0.28f

    Box(
        modifier = Modifier
            .size(size)
            .clip(RoundedCornerShape(radius))
            .clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(
            modifier = Modifier
                .size(size)
                .graphicsLayer {
                    val scale = 1f - 0.35f * workingProgress
                    scaleX = scale
                    scaleY = scale
                    alpha = 1f - workingProgress
                },
        ) {
            drawRect(color = bg)
            val s = this.size.width / 36f
            val fg = Color.White
            when (look) {
                "visor" -> {
                    drawRoundRect(
                        color = fg,
                        topLeft = Offset(7f * s, 9f * s),
                        size = Size(22f * s, 18f * s),
                        cornerRadius = CornerRadius(5f * s, 5f * s),
                        style = Stroke(width = 2f * s),
                    )
                    drawRoundRect(
                        color = fg,
                        topLeft = Offset(10f * s, 14f * s),
                        size = Size(16f * s, 5f * s),
                        cornerRadius = CornerRadius(2.5f * s, 2.5f * s),
                    )
                }
                "cyclops" -> {
                    drawCircle(
                        color = fg,
                        radius = 10f * s,
                        center = Offset(18f * s, 18f * s),
                        style = Stroke(width = 2f * s),
                    )
                    drawCircle(color = fg, radius = 4f * s, center = Offset(18f * s, 18f * s))
                }
                "cat" -> {
                    val path = Path().apply {
                        moveTo(8f * s, 25f * s)
                        lineTo(8f * s, 10f * s)
                        lineTo(14f * s, 15f * s)
                        lineTo(22f * s, 15f * s)
                        lineTo(28f * s, 10f * s)
                        lineTo(28f * s, 25f * s)
                        close()
                    }
                    drawPath(path, color = fg, style = Stroke(width = 2f * s))
                    drawCircle(color = fg, radius = 1.8f * s, center = Offset(14f * s, 20f * s))
                    drawCircle(color = fg, radius = 1.8f * s, center = Offset(22f * s, 20f * s))
                }
                "plush" -> {
                    // A felt face: short fur strokes around the blob and large open eyes.
                    val center = Offset(18f * s, 18f * s)
                    val fur = 12.2f * s
                    for (i in 0 until 26) {
                        val a = i / 26f * 2f * Math.PI
                        val from = Offset(center.x + (fur * Math.cos(a)).toFloat(), center.y + (fur * Math.sin(a)).toFloat())
                        val to = Offset(center.x + ((fur + 1.5f * s) * Math.cos(a)).toFloat(), center.y + ((fur + 1.5f * s) * Math.sin(a)).toFloat())
                        drawLine(fg.copy(alpha = 0.7f), from, to, strokeWidth = 1.1f * s, cap = StrokeCap.Round)
                    }
                    drawCircle(fg, radius = 8.6f * s, center = center)
                    drawCircle(bg, radius = 2.6f * s, center = Offset(14.5f * s, 17f * s))
                    drawCircle(bg, radius = 2.6f * s, center = Offset(21.5f * s, 17f * s))
                    drawCircle(fg, radius = 1.15f * s, center = Offset(14.5f * s, 17.2f * s))
                    drawCircle(fg, radius = 1.15f * s, center = Offset(21.5f * s, 17.2f * s))
                }
                else -> {
                    drawLine(
                        color = fg,
                        start = Offset(18f * s, 5f * s),
                        end = Offset(18f * s, 9f * s),
                        strokeWidth = 2f * s,
                        cap = StrokeCap.Round,
                    )
                    drawRoundRect(
                        color = fg,
                        topLeft = Offset(8f * s, 9f * s),
                        size = Size(20f * s, 18f * s),
                        cornerRadius = CornerRadius(6f * s, 6f * s),
                        style = Stroke(width = 2f * s),
                    )
                    drawCircle(color = fg, radius = 2f * s, center = Offset(14f * s, 17f * s))
                    drawCircle(color = fg, radius = 2f * s, center = Offset(22f * s, 17f * s))
                }
            }
        }
        if (workingProgress > 0.01f) {
            Canvas(
                modifier = Modifier
                    .size(size)
                    .graphicsLayer {
                        val scale = 0.65f + 0.35f * workingProgress
                        scaleX = scale
                        scaleY = scale
                        alpha = workingProgress
                        rotationZ = spinDeg
                    },
            ) {
                drawArc(
                    color = bg,
                    startAngle = -90f,
                    sweepAngle = 250f,
                    useCenter = false,
                    style = Stroke(width = this.size.width * 0.08f, cap = StrokeCap.Round),
                )
            }
        }
    }
}
