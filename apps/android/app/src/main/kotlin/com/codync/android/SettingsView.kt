package com.codync.android

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/**
 * The Settings tab: what belongs to this phone alone (appearance, language, notification permission,
 * analytics, Start over; iOS `SettingsView`). Each computer's own settings (voice chat, push, connection,
 * hidden bots) are in Computer management (`ComputersView`).
 */
@Composable internal fun SettingsView(state: AppState, store: AppStore, bottomInset: Dp) {
    val uri = LocalUriHandler.current
    var confirmStartOver by remember { mutableStateOf(false) }
    var aboutAnalytics by remember { mutableStateOf(false) }
    CardForm(Modifier.background(Palette.background).statusBarsPadding(), PaddingValues(start = 20.dp, top = 12.dp, end = 20.dp, bottom = bottomInset)) {
        Text(tr("Settings"), Modifier.semantics { heading() }, style = Type.largeTitle.bold, color = Palette.text)
        AppearanceSettings()
        val appLanguage = LocalAppLanguage.current
        val setAppLanguage = LocalSetAppLanguage.current
        val systemChinese = LocalSystemChinese.current
        val phrases = LocalPhrases.current
        CardSection("Language", footer = "Choose the language for this device. Unsupported system languages use English.") {
            row {
                ValueRow("App language") {
                    ChoicePicker(appLanguage, AppLanguage.entries.map {
                        it to AppLanguage.menuTitle(it, appLanguage, systemChinese, phrases)
                    }, setAppLanguage)
                }
            }
        }
        ThinkingDetailsSettings()
        NotificationsSection()
        CardSection(footer = "Which features you use, never your messages, code or files. Helps us spot bugs sooner and make Codync better for you.") {
            row {
                val analyticsLabel = tr("Share usage analytics")
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(analyticsLabel, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    IconButton("About usage analytics", "info.circle") { aboutAnalytics = true }
                    Box(Modifier.weight(1f))
                    CodyncSwitch(Analytics.enabled, Analytics::setEnabled, Modifier.width(34.dp)
                        .semantics { contentDescription = analyticsLabel }) {}
                }
            }
        }
        CardSection {
            row { ValueRow("App version", BuildConfig.VERSION_NAME) }
            row { LinkButton("Source code") { uri.open("https://github.com/leepokai/Codync", OutboundLink.Github) } }
        }
        CardSection {
            row {
                Row(Modifier.fillMaxWidth().plainButton(enabled = "session" !in state.busy) { confirmStartOver = true },
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("arrow.counterclockwise", size = 20.dp, tint = Palette.danger)
                    Text(tr("Start over"), style = Type.body, color = Palette.danger)
                }
            }
        }
    }
    CodyncSheet(aboutAnalytics, { aboutAnalytics = false }) { AnalyticsSheet() }
    CodyncDialog("Start over?", confirmStartOver, { confirmStartOver = false },
        "Signs out of every account and forgets every computer on this phone, then shows the welcome again. Your computers keep their bots and chats; pair again to use them.") {
        listOf(DialogAction("Start over", destructive = true, action = store::startOver))
    }
}

/** Whether Android lets Codync show notifications at all; each computer's push switch is on its card. */
@Composable internal fun NotificationsSection() {
    val context = LocalContext.current
    var answered by remember { mutableStateOf(false) }
    val asked = rememberOnResume { PushPreferences.asked(context) } || answered
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        notificationsAnswered(context, granted, PromptSource.Settings); answered = true
    }
    val allowed = rememberOnResume { notificationsAllowed(context) } || asked && notificationsAllowed(context)
    CardSection("Notifications", "Get a result summary, a request for input, or a failure notice. Notification previews follow your Android settings.") {
        if (!BuildConfig.PUSH_CONFIGURED) row { Text("Push isn't set up in this build. Bots and chats still update while Codync is open.", style = Type.subheadline, color = Palette.secondary) }
        row {
            ValueRow("Notifications", if (allowed) "On" else if (asked) "Off in Settings" else "Turn on", Modifier.plainButton {
                if (!allowed && !asked && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) request.launch(Manifest.permission.POST_NOTIFICATIONS)
                else openNotificationSettings(context)
            })
        }
    }
}
