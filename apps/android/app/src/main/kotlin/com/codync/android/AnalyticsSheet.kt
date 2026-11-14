package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

private val DETAILS = listOf(
    Triple("chart.bar", "Features you use", "Opening the app and its screens, setting up and pairing (and whether you allow notifications), creating or deleting a bot, sending a message (how many attachments, never the files), running a routine or a voice call, installing a connector. Each comes with the app version, the platform, and the bot's agent and model."),
    Triple("exclamationmark.triangle", "Crashes and errors", "When Codync crashes or an agent fails to answer: the kind of error and where in Codync's own code it happened. Never the error's message; file paths are cut down to Codync's own files."),
    Triple("globe", "Approximate location", "Country and city, looked up from your IP address. The IP address itself is never stored."),
    Triple("person.crop.circle", "Who it belongs to", "Your Codync account when you're signed in, otherwise a random ID made on this device."),
    Triple("eye.slash", "Never shared", "Your messages, prompts, code, files, file names or paths, bot names, credentials, or anything your agents produce."),
    Triple("lock.shield", "How it's used", "Only to spot bugs sooner and see which features work and where people get stuck. Never sold, never used for ads or to track you across other apps and websites."),
    Triple("power", "Your choice", "Each phone and each computer has its own switch. Turning it off stops sending right away and drops anything not sent yet."),
)

/** Settings' info button beside Share usage analytics (iOS `AnalyticsSheet`): everything it covers, true to the privacy policy. */
@Composable internal fun AnalyticsSheet() {
    val uri = LocalUriHandler.current
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Usage analytics")
        CardForm {
            CardSection {
                for ((icon, title, detail) in DETAILS) row {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Box(Modifier.width(22.dp).padding(top = 1.dp), contentAlignment = Alignment.Center) { Symbol(icon, size = 18.dp, tint = Palette.secondary) }
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(tr(title), style = Type.body, color = Palette.text)
                            Text(tr(detail), style = Type.footnote, color = Palette.secondary)
                        }
                    }
                }
            }
            Row(Modifier.padding(horizontal = 2.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(tr("See the"), style = Type.caption, color = Palette.secondary)
                LinkButton("Privacy Policy", style = Type.caption.copy(textDecoration = androidx.compose.ui.text.style.TextDecoration.Underline),
                    color = Palette.text) { uri.open("https://codync.dev/privacy", OutboundLink.Privacy) }
                Text(tr("for details."), style = Type.caption, color = Palette.secondary)
            }
        }
    }
}
