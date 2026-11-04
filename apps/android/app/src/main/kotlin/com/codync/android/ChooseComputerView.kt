package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.CloudComputer
import com.codync.android.design.*

/**
 * First run, signed in to an account that already has computers: ask one of them for access, or pair a new
 * one, like a VPS (iOS `ChooseComputerView`). Nothing is asked until a tap. An account with none goes straight to `PairingView`.
 */
@Composable internal fun ChooseComputerView(state: AccountContext, computers: List<CloudComputer>, ask: (CloudComputer) -> Unit,
    pairNew: () -> Unit, skip: () -> Unit, leading: @Composable RowScope.() -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.OnboardingStepViewed(OnboardingStep.Choose))
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader(leading)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp)) {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(tr("Connect a computer"), style = PAGE_TITLE, color = Palette.text)
                Text(tr("Ask a computer in your account for access, or pair a new one, like a VPS."), style = Type.body, color = Palette.secondary)
            }
            CardSection("In your account") {
                for (computer in computers) row {
                    AccountComputerRow(computer, state.access[computer.computerId]?.code, asking = computer.computerId in state.asking, ask = { ask(computer) })
                }
            }
            CardSection {
                row {
                    Row(Modifier.fillMaxWidth().plainButton(action = pairNew), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) { Symbol("plus", size = 22.dp, tint = Palette.text) }
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(tr("Pair a new computer"), style = Type.body.semibold, color = Palette.text)
                            Text(tr("Install Codync on a VPS or another machine."), style = Type.subheadline, color = Palette.secondary)
                        }
                        Symbol("chevron.right", size = 14.dp, tint = Palette.tertiary)
                    }
                }
            }
        }
        Box(Modifier.padding(horizontal = 24.dp).padding(bottom = 12.dp)) { WideButton("Skip", primary = false, action = skip) }
    }
}
