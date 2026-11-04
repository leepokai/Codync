package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.DemoScript
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * "Try demo mode": while it is on, the app shows [DemoScript]'s sample bots instead of the computer's and refuses every call
 * to the computer and every analytics event. The real state keeps living underneath, so leaving brings back exactly what was
 * there. Nothing is read from or written to the device by the demo itself.
 */
internal object DemoMode {
    private val on = MutableStateFlow(false)
    val active: StateFlow<Boolean> = on.asStateFlow()

    fun enter() { on.value = true }
    fun exit() { on.value = false }

    /** What the screens render while the demo is on: the script over the real state's shell, with the computer reachable. */
    fun shown(real: AppState, now: Long): AppState = real.copy(
        computers = listOf(DemoScript.computer),
        bots = DemoScript.bots(now),
        entries = DemoScript.entries(now),
        routines = DemoScript.routines(now),
        pending = emptyList(),
        selectedBot = null,
        thread = null,
        loading = false,
        setupComplete = true,
        rosterLoaded = true,
        connection = LinkState.Ready(HostRoute.Direct),
        actualConnection = LinkState.Ready(HostRoute.Direct),
        busy = emptySet(),
        error = null,
    )
}

/** Always visible while the demo is on. */
@Composable internal fun DemoBadge() {
    Row(Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 12.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(tr("Demo mode · sample bots, nothing here is real"), Modifier.weight(1f), style = Type.caption, color = Palette.warning, maxLines = 1)
        SecondaryButton("Exit demo") { DemoMode.exit() }
    }
}
