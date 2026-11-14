package com.codync.android.design

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize

/** Grok Bot's motion, as CodyncKit `Motion` has it, so both phones feel the same. */
object Motion {
    private val ease = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1f)
    private val morphEase = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)

    /** Hover and selection fills, tabs, chips, switch knob: `.12s ease`. */
    fun <T> hover(): FiniteAnimationSpec<T> = tween(120, easing = ease)
    /** A view switched in: `opacity .12s`. */
    fun <T> fade(): FiniteAnimationSpec<T> = tween(120, easing = ease)
    /** Press feedback lands almost at once: 50 ms. */
    fun <T> press(): FiniteAnimationSpec<T> = tween(50, easing = ease)
    /** Icon and glyph swaps: `.2s cubic-bezier(.22,1,.36,1)`. */
    fun <T> morph(): FiniteAnimationSpec<T> = tween(200, easing = morphEase)
    /** Size and layout changes: SwiftUI's critically damped 0.3 s spring. */
    fun <T> layout(): FiniteAnimationSpec<T> = spring(Spring.DampingRatioNoBouncy, stiffness = 440f)
    /** A slightly slower spring for a new chat message and its scroll (0.42 s). */
    fun <T> conversation(): FiniteAnimationSpec<T> = spring(Spring.DampingRatioNoBouncy, stiffness = 224f)
    /** UIKit's push and sheet presentation: a 0.5 s critically damped spring. */
    fun <T> present(): FiniteAnimationSpec<T> = spring(Spring.DampingRatioNoBouncy, stiffness = 158f)

    /** SwiftUI's `.spring(duration:bounce:)`. */
    fun <T> spring(duration: Float, bounce: Float = 0f): FiniteAnimationSpec<T> =
        androidx.compose.animation.core.spring(1f - bounce, (2 * Math.PI / duration).let { (it * it).toFloat() })

    val layoutOffset = spring(Spring.DampingRatioNoBouncy, stiffness = 440f, visibilityThreshold = IntOffset(1, 1))
    val layoutSize = spring(Spring.DampingRatioNoBouncy, stiffness = 440f, visibilityThreshold = IntSize(1, 1))

    /** `--ui-press-scale`. */
    const val PRESS_SCALE = 0.98f
}
