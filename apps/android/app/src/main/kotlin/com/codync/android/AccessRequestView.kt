package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.AccessTicket
import com.codync.android.core.CloudComputer
import androidx.compose.ui.draw.clip
import com.kyant.shapes.RoundedRectangle
import com.codync.android.design.*
import kotlinx.coroutines.delay

/** `Failed`: the request didn't reach the computer (try again); `Ended`: the computer showed a code but didn't approve it (ask again). */
private enum class AccessPhase { Renew, Asking, Code, Approved, Failed, Ended }

/** Asks an account computer for this phone: the code to check on it, until it answers (iOS `AccessRequestView`). */
@Composable internal fun AccessRequestView(state: AccountContext, store: AccountStore, computer: CloudComputer) {
    val dismiss = LocalDismissModal.current
    val ticket = state.access[computer.computerId]
    var requestId by remember { mutableStateOf(ticket?.requestId) }
    var confirmRenewal by remember { mutableStateOf(computer.isGranted && ticket == null) }
    var renewRequest by remember { mutableStateOf(false) }
    val approved = requestId != null && state.approvedAccess[computer.computerId] == requestId
    // Whether this request got as far as a code on the computer (iOS `sawTicket`).
    var sawTicket by remember { mutableStateOf(ticket != null) }
    LaunchedEffect(ticket?.requestId) { ticket?.requestId?.let { requestId = it; renewRequest = false; sawTicket = true } }
    // The request ran and is over without an approval: denied or expired after its code, or it failed before one.
    val asking = "access:${computer.computerId}" in state.busy
    var asked by remember { mutableStateOf(asking) }
    var ended by remember { mutableStateOf<String?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { if (!asking && !approved && ticket == null && !confirmRenewal) store.requestAccess(computer) }
    LaunchedEffect(asking, approved) {
        if (asking) asked = true
        else if (asked && !approved) {
            if (sawTicket) ended = state.error ?: "${computer.name} didn't approve this request."
            else failed = state.error ?: "Something went wrong"
            store.clearError(); asked = false
        }
    }
    LaunchedEffect(approved) { if (approved) { delay(1000); dismiss() } }
    val phase = when {
        confirmRenewal -> AccessPhase.Renew
        approved -> AccessPhase.Approved
        ticket != null -> AccessPhase.Code
        failed != null -> AccessPhase.Failed
        ended != null -> AccessPhase.Ended
        else -> AccessPhase.Asking
    }
    // Closing keeps waiting in the background; the computer shows up once approved.
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Access")
        Column(Modifier.fillMaxWidth().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp)) {
            ComputerBadge(computer.device, 64.dp)
            AnimatedContent(phase, transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Access") { shown ->
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp)) {
                    when (shown) {
                        AccessPhase.Renew -> {
                            Text("This phone already has approval. If access no longer works, request a new approval on the computer.",
                                style = Type.body, color = Palette.secondary, textAlign = TextAlign.Center)
                            Text("This replaces this phone's current approval. The computer must be online to show the new code.",
                                style = Type.footnote, color = Palette.secondary, textAlign = TextAlign.Center)
                            SecondaryButton("Request new approval") { confirmRenewal = false; renewRequest = true; store.requestAccess(computer, renew = true) }
                        }
                        AccessPhase.Approved -> IconLabel("${computer.name} approved this phone", "checkmark.circle.fill", Palette.added, style = Type.headline)
                        AccessPhase.Code -> {
                            val code = ticket?.code.orEmpty()
                            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("Check the code on ${computer.name}", style = Type.headline, color = Palette.text)
                                SelectionContainer {
                                    Text(if (code.length == 6) "${code.take(3)} ${code.takeLast(3)}" else code,
                                        Modifier.semantics { contentDescription = "Code ${code.toList().joinToString(" ")}" },
                                        style = Type.system(52f, FontWeight.SemiBold).copy(fontFamily = FontFamily.Monospace), color = Palette.text)
                                }
                                Text("Approve only if the computer shows exactly this code. If it doesn't, deny it there: someone may be trying to get in.",
                                    style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
                            }
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Spinner()
                                Text("Waiting for approval…", style = Type.subheadline, color = Palette.secondary)
                                IconButton("Cancel request", "xmark.circle.fill") { store.cancelAccess(computer.computerId); dismiss() }
                            }
                        }
                        AccessPhase.Failed -> {
                            Text(failed.orEmpty(), style = Type.body, color = Palette.danger, textAlign = TextAlign.Center)
                            SecondaryButton("Try again") { failed = null; store.requestAccess(computer, renew = renewRequest) }
                        }
                        AccessPhase.Ended -> {
                            Text(ended.orEmpty(), style = Type.body, color = Palette.secondary, textAlign = TextAlign.Center)
                            SecondaryButton("Ask again") { ended = null; sawTicket = false; store.requestAccess(computer, renew = renewRequest) }
                        }
                        AccessPhase.Asking -> Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Spinner()
                            Text(if (computer.online != false) "Asking ${computer.name}…" else "${computer.name} is offline. Turn it on to continue.",
                                style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
                        }
                    }
                }
            }
        }
    }
}

/** The pending access request over the tabs: approve on that computer, or cancel. */
@Composable internal fun AccessBanner(ticket: AccessTicket, name: String, cancel: () -> Unit) {
    Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent)
        .padding(start = 14.dp, end = 6.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Spinner(16.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text("Approve on $name", style = Type.subheadline.medium, color = Palette.text)
            Text("Code ${ticket.code.take(3)} ${ticket.code.takeLast(3)}", style = Type.footnote.monospacedDigit, color = Palette.secondary)
        }
        IconButton("Cancel request", "xmark", action = cancel)
    }
}
