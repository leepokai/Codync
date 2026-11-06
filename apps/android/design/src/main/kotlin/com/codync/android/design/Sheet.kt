package com.codync.android.design

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.kyant.shapes.UnevenRoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/** Closes the modal the content is in (`\.dismissModal`). */
val LocalDismissModal = staticCompositionLocalOf<() -> Unit> { {} }

/** Allowed height detents for a `CodyncSheet`. */
enum class SheetDetent { Medium, Expanded }

internal enum class SheetReleaseOutcome { Dismiss, SettleMedium, SettleExpanded }

/** Pure height and transition calculations for `CodyncSheet` detents and IME adaptation. */
internal object SheetMetrics {
    const val MEDIUM_OFFSET_FRACTION = 0.36f
    const val MIN_WINDOW_DP_FOR_MEDIUM = 520f

    fun canUseMedium(detents: Set<SheetDetent>, windowHeightDp: Float, imeBottomPx: Int): Boolean =
        SheetDetent.Medium in detents && windowHeightDp >= MIN_WINDOW_DP_FOR_MEDIUM && imeBottomPx <= 0

    fun anchorOffsetPx(detent: SheetDetent, windowHeightPx: Float, allowMedium: Boolean): Float =
        if (detent == SheetDetent.Medium && allowMedium) windowHeightPx * MEDIUM_OFFSET_FRACTION else 0f

    fun resolveRelease(
        currentOffsetPx: Float,
        velocityY: Float,
        windowHeightPx: Float,
        allowMedium: Boolean,
        currentDetent: SheetDetent,
    ): SheetReleaseOutcome {
        val mediumPx = anchorOffsetPx(SheetDetent.Medium, windowHeightPx, allowMedium)
        val closeThreshold = if (currentDetent == SheetDetent.Medium && allowMedium) {
            mediumPx + windowHeightPx * 0.18f
        } else {
            windowHeightPx * 0.25f
        }
        if (currentOffsetPx > closeThreshold || velocityY > 1600f) return SheetReleaseOutcome.Dismiss
        if (!allowMedium) return SheetReleaseOutcome.SettleExpanded
        if (velocityY < -900f) return SheetReleaseOutcome.SettleExpanded
        if (velocityY > 900f && currentDetent == SheetDetent.Expanded) return SheetReleaseOutcome.SettleMedium
        val midpoint = mediumPx * 0.5f
        return if (currentOffsetPx >= midpoint) SheetReleaseOutcome.SettleMedium else SheetReleaseOutcome.SettleExpanded
    }
}

/**
 * Modal sheet with configurable detents (`Medium`, `Expanded`), IME-aware auto-expansion,
 * unified drag + nested scroll handling, and Android 14+ Predictive Back progression.
 */
@Composable fun CodyncSheet(
    isPresented: Boolean,
    onDismiss: () -> Unit,
    detents: Set<SheetDetent> = setOf(SheetDetent.Expanded),
    initialDetent: SheetDetent = if (SheetDetent.Medium in detents) SheetDetent.Medium else SheetDetent.Expanded,
    content: @Composable () -> Unit,
) {
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(isPresented) }
    LaunchedEffect(isPresented) {
        if (isPresented) {
            mounted = true
            shown.animateTo(1f, Motion.present())
        } else {
            shown.animateTo(0f, Motion.present())
            mounted = false
        }
    }
    if (!mounted) return
    val dismiss by rememberUpdatedState(onDismiss)
    Overlay {
        SheetFrame(
            shown = shown.value,
            isPresented = isPresented,
            detents = detents.ifEmpty { setOf(SheetDetent.Expanded) },
            initialDetent = initialDetent,
            dismiss = { dismiss() },
            content = content,
        )
    }
}

/** `.codyncSheet(item:)`: keeps showing the last item while the sheet slides away. */
@Composable fun <T : Any> CodyncSheet(
    item: T?,
    onDismiss: () -> Unit,
    detents: Set<SheetDetent> = setOf(SheetDetent.Expanded),
    initialDetent: SheetDetent = if (SheetDetent.Medium in detents) SheetDetent.Medium else SheetDetent.Expanded,
    content: @Composable (T) -> Unit,
) {
    var last by remember { mutableStateOf(item) }
    if (item != null) last = item
    CodyncSheet(item != null, onDismiss, detents, initialDetent) { last?.let { content(it) } }
}

@Composable private fun SheetFrame(
    shown: Float,
    isPresented: Boolean,
    detents: Set<SheetDetent>,
    initialDetent: SheetDetent,
    dismiss: () -> Unit,
    content: @Composable () -> Unit,
) {
    val dark = LocalDark.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val imeBottomPx = WindowInsets.ime.getBottom(density)
    var currentDetent by remember(detents, initialDetent) {
        mutableStateOf(if (initialDetent in detents) initialDetent else SheetDetent.Expanded)
    }
    var backProgress by remember { mutableFloatStateOf(0f) }

    PredictiveBackHandler(enabled = isPresented) { events ->
        try {
            events.collect { event ->
                backProgress = event.progress.coerceIn(0f, 1f)
            }
            backProgress = 0f
            dismiss()
        } catch (cancelled: CancellationException) {
            val anim = Animatable(backProgress)
            scope.launch {
                anim.animateTo(0f, Motion.layout()) { backProgress = value }
            }
            throw cancelled
        }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val windowHeightDp = maxHeight.value
        val height = with(density) { maxHeight.toPx() }
        val allowMedium = SheetMetrics.canUseMedium(detents, windowHeightDp, imeBottomPx)
        val effectiveDetent = if (allowMedium) currentDetent else SheetDetent.Expanded
        val targetAnchor = SheetMetrics.anchorOffsetPx(effectiveDetent, height, allowMedium)
        val sheetOffset = remember { Animatable(targetAnchor) }

        LaunchedEffect(targetAnchor) {
            sheetOffset.animateTo(targetAnchor, Motion.layout())
        }

        fun release(velocity: Float) {
            scope.launch {
                when (SheetMetrics.resolveRelease(sheetOffset.value, velocity, height, allowMedium, effectiveDetent)) {
                    SheetReleaseOutcome.Dismiss -> dismiss()
                    SheetReleaseOutcome.SettleMedium -> {
                        currentDetent = SheetDetent.Medium
                        sheetOffset.animateTo(SheetMetrics.anchorOffsetPx(SheetDetent.Medium, height, true), Motion.layout())
                    }
                    SheetReleaseOutcome.SettleExpanded -> {
                        currentDetent = SheetDetent.Expanded
                        sheetOffset.animateTo(0f, Motion.layout())
                    }
                }
            }
        }

        val pull = remember(allowMedium, height, effectiveDetent) {
            object : NestedScrollConnection {
                override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                    if (available.y >= 0f || sheetOffset.value <= 0f) return Offset.Zero
                    // Dragging upward while at Medium or partially pulled down expands the sheet first.
                    val used = maxOf(available.y, -sheetOffset.value)
                    scope.launch { sheetOffset.snapTo((sheetOffset.value + used).coerceAtLeast(0f)) }
                    return Offset(0f, used)
                }

                override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                    if (available.y <= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                    scope.launch { sheetOffset.snapTo(sheetOffset.value + available.y) }
                    return Offset(0f, available.y)
                }

                override suspend fun onPreFling(available: Velocity): Velocity {
                    val settledAnchor = SheetMetrics.anchorOffsetPx(effectiveDetent, height, allowMedium)
                    if (abs(sheetOffset.value - settledAnchor) < 1f) return Velocity.Zero
                    release(available.y)
                    return available
                }
            }
        }

        val scrimAlpha = (if (dark) 0.48f else 0.2f) * shown * (1f - 0.35f * backProgress)
        Box(
            Modifier.fillMaxSize()
                .background(Color.Black.copy(alpha = scrimAlpha))
                .clickable(remember { MutableInteractionSource() }, null, onClick = dismiss)
        )
        val slideOffset = (1f - shown) * (height - sheetOffset.value) + sheetOffset.value + (backProgress * height * 0.12f)
        val scale = 1f - 0.04f * backProgress
        Column(Modifier.fillMaxSize()) {
            Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
            Spacer(Modifier.height(10.dp))
            Box(
                Modifier.fillMaxSize()
                    .offset { IntOffset(0, slideOffset.roundToInt()) }
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        transformOrigin = TransformOrigin(0.5f, 1f)
                        shadowElevation = if (dark) 0f else 24f
                        shape = SheetShape
                        clip = true
                    }
                    .background(Palette.background)
                    .clickable(remember { MutableInteractionSource() }, null) {}
                    .draggable(
                        rememberDraggableState { delta ->
                            scope.launch { sheetOffset.snapTo((sheetOffset.value + delta).coerceAtLeast(0f)) }
                        },
                        Orientation.Vertical,
                        onDragStopped = { velocity -> release(velocity) },
                    )
                    .nestedScroll(pull)
            ) {
                val visibleSheetHeightDp = with(density) { (height - slideOffset.coerceAtLeast(0f)).coerceAtLeast(0f).toDp() }
                CompositionLocalProvider(LocalDismissModal provides dismiss) {
                    GlassStage(
                        Modifier.fillMaxWidth()
                            .height(visibleSheetHeightDp)
                            .navigationBarsPadding()
                            .imePadding()
                    ) { content() }
                }
                Box(
                    Modifier.fillMaxWidth()
                        .padding(top = 6.dp)
                        .then(
                            if (allowMedium && SheetDetent.Expanded in detents) {
                                val toggleDetent = {
                                    currentDetent = if (effectiveDetent == SheetDetent.Medium) SheetDetent.Expanded else SheetDetent.Medium
                                }
                                Modifier.semantics {
                                    contentDescription = if (effectiveDetent == SheetDetent.Medium) "Expand sheet" else "Collapse sheet"
                                    onClick { toggleDetent(); true }
                                }.clickable(role = Role.Button, onClick = toggleDetent)
                            } else Modifier
                        ),
                    contentAlignment = Alignment.TopCenter,
                ) {
                    Box(Modifier.size(36.dp, 4.dp).clip(Capsule()).background(if (dark) Color(0xFF5A5A5A) else Color(0xFFC6C6C6)))
                }
            }
        }
    }
}

private val SheetShape = UnevenRoundedRectangle(topStart = 28.dp, topEnd = 28.dp)
