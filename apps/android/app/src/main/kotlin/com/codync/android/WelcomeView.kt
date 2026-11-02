package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay
import kotlin.math.sin

/** First launch, before any setup: who the bots are, a glimpse of talking to one, one way forward (iOS `WelcomeView`). */
@Composable internal fun WelcomeView(state: AccountContext, store: AccountStore, start: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.OnboardingStepViewed(OnboardingStep.Welcome))
    val still = reduceMotion()
    // 0 = nothing yet … 4 = everything shown. Each beat enters in turn.
    var beat by remember { mutableIntStateOf(if (still) 4 else 0) }
    LaunchedEffect(Unit) {
        for (next in beat + 1..4) { delay(if (next == 1) 150 else 280); beat = next }
    }
    BoxWithConstraints(Modifier.fillMaxSize().background(Palette.background).safeDrawingPadding()) {
        val height = maxHeight
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            // Three flexible gaps as the iOS spacers: above the crew, under it, and over the buttons.
            Column(Modifier.fillMaxWidth().heightIn(min = height).padding(horizontal = 24.dp).padding(bottom = 12.dp),
                verticalArrangement = Arrangement.SpaceBetween) {
                Spacer(Modifier.height(0.dp))
                Crew(beat >= 1, Modifier.padding(top = 24.dp))
                Column(Modifier.padding(top = 24.dp)) {
                    ChatGlimpse(beat >= 2, Modifier.padding(bottom = 24.dp))
                    Column(Modifier.rise(beat >= 3), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(tr("Your coding agents,\nas teammates."), style = Type.system(34f, FontWeight.SemiBold).copy(letterSpacing = (-0.6).sp),
                            color = Palette.text)
                        Text(tr("Give each one a name and a project. They work on your computer while you're away."), style = Type.body, color = Palette.secondary)
                        DemoButton(Modifier.padding(top = 4.dp))
                    }
                }
                Column(Modifier.padding(top = 32.dp).rise(beat >= 4), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    WideButton("Get started", action = start)
                    // Signed in, the computers on the account show up without scanning a code.
                    if (state.account.configured) SignInButtons(state, store)
                    state.account.error?.let { Text(it, style = Type.footnote, color = Palette.danger) }
                }
            }
        }
    }
}

/** Five bots bobbing out of step: the "persistent, named" part of the pitch, without words. */
@Composable private fun Crew(shown: Boolean, modifier: Modifier = Modifier) {
    val still = reduceMotion()
    var time by remember { mutableFloatStateOf(0f) }
    if (shown && !still) LaunchedEffect(Unit) { while (true) withFrameNanos { time = it / 1e9f } }
    val density = LocalDensity.current
    Box(modifier.fillMaxWidth().height(170.dp), contentAlignment = Alignment.Center) {
        for ((index, member) in CREW.withIndex()) {
            val entry = remember { Animatable(if (still) 1f else 0f) }
            LaunchedEffect(shown) {
                if (shown) { delay(index * 70L); entry.animateTo(1f, Motion.spring(0.7f, 0.4f)) } else entry.snapTo(0f)
            }
            Box(Modifier.graphicsLayer {
                val bob = if (still) 0f else sin(time * 1.3f + index * 1.7f) * 5f
                translationX = with(density) { member.x.dp.toPx() }
                translationY = with(density) { (member.y + bob).dp.toPx() }
                val scale = 0.3f + 0.7f * entry.value
                scaleX = scale; scaleY = scale; alpha = entry.value.coerceIn(0f, 1f)
            }) { CharacterAvatar(member.shape, member.color, member.size.dp, member.mood) }
        }
    }
}

private class CrewMember(val shape: String, val color: String, val size: Int, val x: Float, val y: Float, val mood: String)

private val CREW = listOf(
    CrewMember("blob", "blue", 92, 0f, 0f, "working"),
    CrewMember("squircle", "orange", 64, -104f, -34f, "idle"),
    CrewMember("teardrop", "violet", 58, 100f, -46f, "working"),
    CrewMember("hex", "green", 48, -76f, 64f, "idle"),
    CrewMember("cloud", "magenta", 52, 84f, 58f, "needsInput"),
)

/** One exchange, played once: you ask, a bot works, a bot answers. */
@Composable private fun ChatGlimpse(shown: Boolean, modifier: Modifier = Modifier) {
    val still = reduceMotion()
    var replied by remember { mutableStateOf(false) }
    LaunchedEffect(shown) { if (shown) { delay(if (still) 0 else 1600); replied = true } }
    Column(modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.fillMaxWidth().rise(shown), contentAlignment = Alignment.CenterEnd) { Bubble("Fix the flaky login test", Palette.bubbleUser) }
        Row(Modifier.rise(shown), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CharacterAvatar("blob", "blue", 28.dp, if (replied) "idle" else "working")
            AnimatedContent(replied, transitionSpec = {
                (fadeIn(Motion.spring(0.45f, 0.2f)) + scaleIn(Motion.spring(0.45f, 0.2f), 0.92f, TransformOrigin(0f, 1f))) togetherWith
                    fadeOut(Motion.fade())
            }, label = "Reply") { done ->
                if (done) Bubble("Done. It raced the session refresh; tests pass on fix/login.", Palette.bubbleAgent)
                else Text("Working…", style = Type.subheadline, color = Palette.tertiary)
            }
        }
    }
}

@Composable private fun Bubble(text: String, fill: Color) =
    Text(text, Modifier.clip(RoundedRectangle(18.dp)).background(fill).padding(horizontal = 14.dp, vertical = 9.dp),
        style = Type.subheadline, color = Palette.text)

/** Opens a short recording of the app in use on YouTube, for anyone who can't pair a computer yet. */
@Composable internal fun DemoButton(modifier: Modifier = Modifier) {
    val uri = LocalUriHandler.current
    Row(modifier.plainButton { uri.open("https://youtube.com/shorts/xtZ5WeyOewU", OutboundLink.Demo) }, verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol("play.circle.fill", size = 17.dp, tint = Palette.text)
        Text("Watch the demo", style = Type.subheadline.medium, color = Palette.text)
    }
}

/** Equal access to both account providers at every sign-in entry point (iOS `SignInButtons`). */
@Composable internal fun SignInButtons(state: AccountContext, store: AccountStore) {
    val busy = "session" in state.busy
    val enabled = state.account.ready && !busy
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        WideButton(enabled = enabled, busy = busy, action = { store.signIn(SignInProvider.Apple) }) {
            Symbol("apple.logo", size = 20.dp, tint = Palette.onAccent)
            Text(tr("Continue with Apple"))
        }
        WideButton(primary = false, enabled = enabled, busy = busy, action = { store.signIn(SignInProvider.Google) }) {
            Image(painterResource(R.drawable.google_logo), null, Modifier.size(20.dp))
            Text(tr("Continue with Google"))
        }
        // Clerk's own page covers the sign-ins that need another step (email codes, verification).
        LinkButton("Other sign-in options", enabled = enabled, color = Palette.secondary, style = Type.footnote) { store.signIn(SignInProvider.Hosted) }
    }
}

/** The full-width 50-point button of the onboarding pages: headline label, a spinner while busy. */
@Composable internal fun WideButton(title: String, primary: Boolean = true, enabled: Boolean = true, action: () -> Unit) =
    WideButton(primary, enabled, action = action) { Text(tr(title)) }

@Composable internal fun WideButton(primary: Boolean = true, enabled: Boolean = true, busy: Boolean = false, action: () -> Unit,
    label: @Composable () -> Unit) {
    val content: @Composable () -> Unit = {
        Box(Modifier.fillMaxWidth().heightIn(min = WIDE_HEIGHT), contentAlignment = Alignment.Center) {
            Row(Modifier.graphicsLayer { alpha = if (busy) 0f else 1f }, verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                androidx.compose.runtime.CompositionLocalProvider(androidx.compose.material3.LocalTextStyle provides Type.headline) { label() }
            }
            if (busy) Spinner(18.dp)
        }
    }
    if (primary) PrimaryButton(action, Modifier.fillMaxWidth(), enabled, content) else SecondaryButton(action, Modifier.fillMaxWidth(), enabled, content)
}

// The label's own 50-point frame, inside the style's 11-point vertical padding (72 in all, as on iOS).
private val WIDE_HEIGHT: Dp = 50.dp

/** Fades up into place once `shown`. */
@Composable private fun Modifier.rise(shown: Boolean): Modifier {
    val progress by animateFloatAsState(if (shown) 1f else 0f, Motion.spring(0.6f, 0.25f), label = "Rise")
    val lift = with(LocalDensity.current) { 14.dp.toPx() }
    return graphicsLayer { alpha = progress.coerceIn(0f, 1f); translationY = (1f - progress) * lift }
}
