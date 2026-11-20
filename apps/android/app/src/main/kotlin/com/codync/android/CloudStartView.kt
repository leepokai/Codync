package com.codync.android

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudMachine
import com.codync.android.design.*

/**
 * First run, while the cloud computer started there gets ready: preparing it, then letting this phone in (iOS
 * `CloudStartView`). The tabs follow once it's reached; [back], whatever its stage, returns to the pages it was started
 * from and Skip goes on without waiting, and either way it turns up in Computers when it's ready.
 */
@Composable internal fun CloudStartView(machine: CloudMachine, back: () -> Unit, skip: () -> Unit, leading: @Composable RowScope.() -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.OnboardingStepViewed(OnboardingStep.Cloud))
    val start = machine.start
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader(leading)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp)) {
            // A bot on a cloud: that's where it lives now.
            Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
                Cloud(Modifier.size(174.dp, 120.dp), Palette.tertiary)
                Box(Modifier.offset(y = 10.dp)) {
                    CharacterAvatar("blob", "blue", 52.dp, if (start == CloudMachine.Start.Failed) "needsInput" else "working")
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(tr("Starting your cloud computer"), style = PAGE_TITLE, color = Palette.text)
                Text("${machine.name} · ${runsOn(machine.providerName, machine.region)}", style = Type.body, color = Palette.secondary)
            }
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Crossfade(start == CloudMachine.Start.Failed, animationSpec = Motion.fade(), label = "Cloud start") { failed ->
                    if (failed) Text(tr("Cloud computer failed"), style = Type.body, color = Palette.danger)
                    else Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Spinner(18.dp)
                            Crossfade(start == CloudMachine.Start.Preparing, animationSpec = Motion.fade(), label = "Cloud start stage") { preparing ->
                                Text(tr(if (preparing) "Preparing your cloud computer…" else "Connecting…"), style = Type.body, color = Palette.text)
                            }
                        }
                        Text(tr("About a minute. You can skip: it shows up on its own when it's ready."), style = Type.footnote, color = Palette.secondary)
                    }
                }
                SecondaryButton(back, Modifier.fillMaxWidth()) { Text(tr("Back"), style = Type.headline) }
            }
        }
        SkipSetupButton(skip)
    }
}

/** SF Symbols' `cloud` at its ultralight weight: a hairline outline of three lobes over a flat base (the Material stand-in
 * is far heavier this large). */
@Composable private fun Cloud(modifier: Modifier, color: Color) {
    Canvas(modifier) {
        val unit = size.height
        fun lobe(x: Float, y: Float, radius: Float) = Path().apply { addOval(Rect(Offset(x * unit, y * unit), radius * unit)) }
        val base = Path().apply { addRect(Rect(0.38f * unit, 0.68f * unit, 1.10f * unit, 0.95f * unit)) }
        val outline = listOf(lobe(0.38f, 0.68f, 0.27f), lobe(0.78f, 0.47f, 0.40f), lobe(1.10f, 0.70f, 0.25f), base)
            .reduce { shape, next -> Path.combine(PathOperation.Union, shape, next) }
        drawPath(outline, color, style = Stroke(2.5.dp.toPx()))
    }
}
