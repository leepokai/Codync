package com.codync.android

import android.content.ClipData
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.net.Inet4Address
import java.net.NetworkInterface

private enum class Step { Install, Scan }
private enum class ComputerOS { Mac, Linux }

/**
 * Pair with a computer running codync-host, one thing per page: install it, then scan its code
 * (iOS `PairingView`). In a sheet it has a close button and closes once paired.
 */
@Composable internal fun PairingView(state: AccountContext, store: AccountStore, inModal: Boolean = false, onSkip: () -> Unit = {},
    leading: @Composable RowScope.() -> Unit = {}) {
    var step by rememberSaveable { mutableStateOf(Step.Install) }
    var os by rememberSaveable { mutableStateOf(ComputerOS.Mac) }
    val dismiss = LocalDismissModal.current
    // In a sheet it's the Pairing screen (reported where it opens); otherwise first-run steps.
    if (!inModal) AnalyticsOnAppear(AnalyticsEvent.OnboardingStepViewed(if (step == Step.Install) OnboardingStep.Install else OnboardingStep.Scan))
    BackHandler(step == Step.Scan) { step = Step.Install }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader({ if (step == Step.Scan) BackButton { step = Step.Install } else leading() }, {
            Row(Modifier.semantics(mergeDescendants = true) { contentDescription = if (step == Step.Install) "Step 1 of 2" else "Step 2 of 2" },
                horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.size(18.dp, 4.dp).clip(Capsule()).background(Palette.text))
                Box(Modifier.size(18.dp, 4.dp).clip(Capsule()).background(if (step == Step.Scan) Palette.text else Palette.accentDim))
            }
        }) { if (inModal) IconButton("Close", "xmark", action = dismiss) }
        AnimatedContent(step, Modifier.weight(1f), transitionSpec = {
            val forward = targetState == Step.Scan
            (slideInHorizontally(Motion.spring(0.45f)) { if (forward) it else -it } + fadeIn(Motion.spring(0.45f))) togetherWith
                (slideOutHorizontally(Motion.spring(0.45f)) { if (forward) -it else it } + fadeOut(Motion.spring(0.45f)))
        }, label = "Pairing step") { shown ->
            when (shown) {
                Step.Install -> InstallPage(os, { os = it }) { step = Step.Scan }
                Step.Scan -> ScanPage(state, store, os) { if (inModal) dismiss() }
            }
        }
        AnimatedVisibility(step == Step.Scan, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            Box(Modifier.padding(horizontal = 24.dp).padding(bottom = 12.dp)) {
                WideButton("Skip", primary = false) { if (inModal) dismiss() else onSkip() }
            }
        }
    }
}

// MARK: - Install

@Composable private fun InstallPage(os: ComputerOS, choose: (ComputerOS) -> Unit, next: () -> Unit) {
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp)) {
            // A bot on the computer's screen: that's where it lives.
            Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
                Laptop(Modifier.size(176.dp, 116.dp), Palette.tertiary)
                Box(Modifier.offset(y = (-8).dp)) { CharacterAvatar("blob", "blue", 52.dp, "working") }
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(tr("Install Codync\non your computer"), style = PAGE_TITLE, color = Palette.text)
                Text(tr("Your bots run there. This phone is how you talk to them."), style = Type.body, color = Palette.secondary)
            }
            SegmentedChoice(os, listOf(ComputerOS.Mac to "Mac", ComputerOS.Linux to "Linux"), choose)
            // Copied here, pasted on the computer (a message to yourself, a shared clipboard).
            Crossfade(os, animationSpec = Motion.fade(), label = "Install steps") { shown ->
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (shown == ComputerOS.Mac) {
                        Hint("Install with Homebrew")
                        CommandBlock("brew install --cask leepokai/codync/codync", records = OutboundLink.InstallCommand)
                        Hint("or download the Mac app")
                        CommandBlock("https://github.com/leepokai/Codync/releases/latest/download/codync-macos.dmg", records = OutboundLink.InstallCommand)
                    } else {
                        Hint("Run the installer")
                        CommandBlock("curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh\ncodync-host install", records = OutboundLink.InstallCommand)
                        Hint("or install with Homebrew")
                        CommandBlock("brew install leepokai/codync/codync-host\ncodync-host install", records = OutboundLink.InstallCommand)
                    }
                    Text(tr(if (shown == ComputerOS.Mac) "Open Codync and it sets up the host on its own."
                        else "codync-host install keeps it running in the background."), style = Type.subheadline, color = Palette.secondary)
                    Hint("Every install option")
                    CommandBlock("https://www.codync.dev/#install", records = OutboundLink.InstallCommand)
                }
            }
            DemoButton()
        }
        Box(Modifier.padding(horizontal = 24.dp).padding(bottom = 12.dp)) { WideButton("Continue", action = next) }
    }
}

/** SF Symbols' `laptopcomputer` at its ultralight weight: a hairline screen over a thin base. */
@Composable private fun Laptop(modifier: Modifier, color: Color) {
    Canvas(modifier) {
        val line = 2.5.dp.toPx()
        val base = 7.dp.toPx()
        val inset = size.width * 0.11f
        val screen = Size(size.width - 2 * inset, size.height - base - 2.dp.toPx())
        drawRoundRect(color, Offset(inset + line / 2, line / 2), Size(screen.width - line, screen.height - line),
            CornerRadius(12.dp.toPx()), style = Stroke(line))
        val top = size.height - base
        drawRoundRect(color, Offset(0f, top), Size(size.width, base), CornerRadius(base / 2))
    }
}

/** A small label above a `CommandBlock`. */
@Composable private fun Hint(text: String) = Text(tr(text), style = Type.footnote, color = Palette.secondary)

/** A terminal command with a copy button; copying it counts as opening `records`. */
@Composable private fun CommandBlock(command: String, records: OutboundLink? = null) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var copied by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.codeBackground).padding(start = 14.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        SelectionContainer(Modifier.weight(1f).padding(vertical = 6.dp)) {
            Text(command, style = Type.footnote.monospaced, color = Palette.text)
        }
        Crossfade(copied, animationSpec = Motion.morph(), label = "Copy") { done ->
            IconButton(if (done) "Copied" else "Copy", if (done) "checkmark" else "doc.on.doc") {
                scope.launch {
                    clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Command", command)))
                    records?.let { Analytics.capture(AnalyticsEvent.LinkOpened(it)) }
                    copied = true; delay(1500); copied = false
                }
            }
        }
    }
}

// MARK: - Scan

@Composable private fun ScanPage(state: AccountContext, store: AccountStore, os: ComputerOS, paired: () -> Unit) {
    val uri = LocalUriHandler.current
    var pasted by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    // Bumped after a failed pair so the scanner starts looking again.
    var attempt by remember { mutableIntStateOf(0) }
    var started by remember { mutableStateOf(false) }
    val tailscale = rememberOnResume { tailscaleConnected() }
    fun pair(text: String) {
        if (state.pairing || text.isBlank()) return
        error = null; started = true
        store.pair(text.trim(), PairedVia.Qr)
    }
    // The store pairs in the background: its finish (and its error) belong to this page.
    LaunchedEffect(state.pairing, state.error) {
        if (!started || state.pairing) return@LaunchedEffect
        started = false
        val failure = state.error
        if (failure == null) { pasted = ""; paired() }
        else { error = failure; attempt++; store.clearError() }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(tr("Scan the pairing code"), style = PAGE_TITLE, color = Palette.text)
            Text(tr(if (os == ComputerOS.Mac) "On your Mac, click Codync in the menu bar, then Pair iPhone. The same code pairs this phone."
                else "On your computer, run this in a terminal:"), style = Type.body, color = Palette.secondary)
            if (os == ComputerOS.Linux) CommandBlock("codync-host pair")
        }
        // The camera, right on the page: no extra sheet between the instruction and the scan.
        Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedRectangle(24.dp))) {
            key(attempt) { QrViewfinder({ pair(it) }, Modifier.fillMaxSize()) { message -> if (message != null) error = message } }
            val dim by animateFloatAsState(if (state.pairing) 1f else 0f, Motion.fade(), label = "Pairing")
            if (dim > 0f) Box(Modifier.fillMaxSize().graphicsLayer { alpha = dim }.background(Palette.background.copy(alpha = 0.7f)),
                contentAlignment = Alignment.Center) { Spinner(28.dp) }
        }
        Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.surface).padding(start = 14.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically) {
            PlainTextField(pasted, { pasted = it }, Modifier.weight(1f), tr("Or paste a {scheme}://pair link").replace("{scheme}", BuildConfig.URL_SCHEME), singleLine = true,
                style = Type.callout.monospaced, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false,
                    imeAction = ImeAction.Go), actions = KeyboardActions(onGo = { pair(pasted) }))
            IconButton("Pair", "arrow.right", enabled = pasted.isNotEmpty() && !state.pairing) { pair(pasted) }
        }
        AnimatedVisibility(error != null, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(tr(error.orEmpty()), style = Type.footnote, color = Palette.danger)
                // The usual cause: the phone has no road to the computer yet.
                IconLabel("Check that this phone and the computer are on the same Wi-Fi or both on Tailscale, or that “Reach from anywhere” is on in Codync on the computer.",
                    "wifi.exclamationmark", Palette.secondary)
            }
        }
        // Signed in, account computers show up in Computers, where a tap asks one for access.
        if (state.account.configured && state.account.userId == null) SignInButtons(state, store)
        // How it connects is never a choice here: every paired computer gets every route.
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            IconLabel(pairingLine(pasted, BuildConfig.URL_SCHEME), "lock.fill")
            if (tailscale) IconLabel("Tailscale is on: Codync connects over it directly.", "checkmark.circle.fill")
            else IconLabel("Use Tailscale? Direct and faster away from home.", "arrow.up.right",
                modifier = Modifier.plainButton { uri.open("https://tailscale.com/download", OutboundLink.Tailscale) })
        }
    }
}

/** `Label` in footnote type: the icon beside the first line. */
@Composable internal fun IconLabel(text: String, systemImage: String, color: Color = Palette.tertiary, modifier: Modifier = Modifier,
    style: TextStyle = Type.footnote) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol(systemImage, Modifier.padding(top = 1.dp), size = 16.dp, tint = color)
        Text(tr(text), style = style, color = color)
    }
}

private val PAGE_TITLE = Type.system(30f, FontWeight.SemiBold).copy(letterSpacing = (-0.5).sp)

/** Tailscale gives every device an address in 100.64.0.0/10 on its VPN (`tun`) interface. */
private fun tailscaleConnected(): Boolean = try {
    NetworkInterface.getNetworkInterfaces()?.toList().orEmpty().any { network ->
        network.isUp && network.name.startsWith("tun") && network.inetAddresses.toList().any { address ->
            val bytes = (address as? Inet4Address)?.address
            bytes != null && bytes[0].toInt() and 0xFF == 100 && (bytes[1].toInt() and 0xFF) in 64..127
        }
    }
} catch (_: Exception) { false }
