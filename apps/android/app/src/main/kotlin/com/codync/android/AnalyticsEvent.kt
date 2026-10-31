package com.codync.android

import android.content.Context
import android.os.Build
import com.codync.android.core.HostError
import com.codync.android.core.PairingProblem
import java.io.IOException
import java.io.InterruptedIOException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive

/**
 * What the phone reports (iOS `AnalyticsEvent`; every event and value: docs/reference/analytics-events.md).
 * Properties are typed: counts, booleans and the catalog's values, never content. Bot and message events
 * come from the computer's host instead.
 */
internal sealed class AnalyticsEvent(val name: String) {
    open val properties: Map<String, JsonElement> get() = emptyMap()

    /** `widgets` is null when the launcher can't say how many are installed. */
    data class AppOpened(val notifications: NotificationAccess, val widgets: Int?) : AnalyticsEvent("app_opened") {
        override val properties get() = mapOf("notifications" to notifications.json) + listOfNotNull(widgets?.let { "widgets" to JsonPrimitive(it) })

        companion object {
            fun of(context: Context) = AppOpened(NotificationAccess.of(context), runCatching { CodyncWidgets.installed(context) }.getOrNull())
        }
    }
    data class OnboardingStepViewed(val step: OnboardingStep) : AnalyticsEvent("onboarding_step_viewed") {
        override val properties get() = mapOf("step" to step.json)
    }
    data class OnboardingCompleted(val via: OnboardingVia) : AnalyticsEvent("onboarding_completed") {
        override val properties get() = mapOf("via" to via.json)
    }
    data class SignInStarted(val provider: SignInProvider) : AnalyticsEvent("sign_in_started") {
        override val properties get() = mapOf("provider" to provider.json)
    }
    data class SignInFailed(val provider: SignInProvider, val reason: SignInFailure) : AnalyticsEvent("sign_in_failed") {
        override val properties get() = mapOf("provider" to provider.json, "reason" to reason.json)
    }
    data class SignedIn(val provider: SignInProvider) : AnalyticsEvent("signed_in") {
        override val properties get() = mapOf("provider" to provider.json)
    }
    data object SignedOut : AnalyticsEvent("signed_out")
    data class PairingFailed(val reason: PairingFailure) : AnalyticsEvent("pairing_failed") {
        override val properties get() = mapOf("reason" to reason.json)
    }
    /** Sends the computer's analytics key (`Analytics.computerTag`), never its id. */
    data class ComputerPaired(val via: PairedVia, val computerId: String) : AnalyticsEvent("computer_paired") {
        override val properties get() = mapOf("via" to via.json, "computer" to JsonPrimitive(Analytics.computerTag(computerId)))
    }
    data object AccessRequested : AnalyticsEvent("access_requested")
    data class AccessRequestFailed(val reason: AccessFailure) : AnalyticsEvent("access_request_failed") {
        override val properties get() = mapOf("reason" to reason.json)
    }
    data class NotificationsAnswered(val granted: Boolean, val source: PromptSource) : AnalyticsEvent("notifications_answered") {
        override val properties get() = mapOf("granted" to JsonPrimitive(granted), "source" to source.json)
    }
    data class NotificationOpened(val kind: NotificationKind) : AnalyticsEvent("notification_opened") {
        override val properties get() = mapOf("kind" to kind.json)
    }
    data class SettingChanged(val setting: PhoneSetting, val on: Boolean) : AnalyticsEvent("setting_changed") {
        override val properties get() = mapOf("setting" to setting.json, "on" to JsonPrimitive(on))
    }
    /** An eligible, viewed result was followed by returning to Bots (iOS `reviewOpportunity`). */
    data class ReviewOpportunity(val completed: Int) : AnalyticsEvent("review_opportunity") {
        override val properties get() = mapOf("completed_interactions" to JsonPrimitive(completed), "policy" to JsonPrimitive("v1"),
            "trigger" to JsonPrimitive("return_to_bots"))
    }
    data class ReviewCancelled(val reason: com.codync.android.core.ReviewCancellation) : AnalyticsEvent("review_cancelled") {
        override val properties get() = mapOf("reason" to JsonPrimitive(reason.wire), "policy" to JsonPrimitive("v1"))
    }
    /** The cooldown is used up and Google Play is about to be asked; not that a prompt showed. */
    data class ReviewRequested(val completed: Int) : AnalyticsEvent("review_requested") {
        override val properties get() = mapOf("completed_interactions" to JsonPrimitive(completed), "policy" to JsonPrimitive("v1"),
            "trigger" to JsonPrimitive("return_to_bots"))
    }
    /** Rate on Google Play, in Settings (iOS `reviewLinkOpened`). */
    data object ReviewLinkOpened : AnalyticsEvent("review_link_opened") {
        override val properties get() = mapOf("source" to JsonPrimitive("settings"))
    }
    data class LinkOpened(val link: OutboundLink) : AnalyticsEvent("link_opened") {
        override val properties get() = mapOf("link" to link.json)
    }
    data class VoiceCallStarted(val engine: VoiceEngineKind) : AnalyticsEvent("voice_call_started") {
        override val properties get() = mapOf("engine" to engine.json)
    }
    /** How one engine of a call fared ([com.codync.android.core.CallQuality]). */
    data class VoiceCallQuality(val engine: VoiceEngineKind, val report: com.codync.android.core.CallQuality.Report) : AnalyticsEvent("voice_call_quality") {
        override val properties get() = mapOf("engine" to engine.json, "reconnects" to JsonPrimitive(report.reconnects),
            "stuck" to JsonPrimitive(report.stuck.wire), "failure" to JsonPrimitive(report.failure?.wire ?: "none")) +
            listOfNotNull(report.readyMs?.let { "ready_ms" to JsonPrimitive(it) })
    }
    data class ScreenViewed(val screen: AnalyticsScreen) : AnalyticsEvent("screen_viewed") {
        override val properties get() = mapOf("screen" to screen.json)
    }
}

/** One of a property's catalog values, as sent. */
internal interface CatalogValue { val wire: String }

private val CatalogValue.json get() = JsonPrimitive(wire)

internal enum class NotificationAccess(override val wire: String) : CatalogValue {
    Allowed("allowed"), Denied("denied"), NotAsked("not_asked");

    companion object {
        /** Android 13+ asks; before that, notifications are on until turned off. */
        fun of(context: Context) = when {
            notificationsAllowed(context) -> Allowed
            Build.VERSION.SDK_INT >= 33 && !PushPreferences.asked(context) -> NotAsked
            else -> Denied
        }
    }
}

internal enum class OnboardingStep(override val wire: String) : CatalogValue { Welcome("welcome"), Install("install"), Scan("scan") }

internal enum class OnboardingVia(override val wire: String) : CatalogValue { Paired("paired"), Account("account"), Skipped("skipped") }

internal enum class SignInProvider(override val wire: String) : CatalogValue { Apple("apple"), Google("google"), Hosted("hosted") }

internal enum class SignInFailure(override val wire: String) : CatalogValue {
    Cancelled("cancelled"), Timeout("timeout"), Verification("verification"), Error("error")
}

internal enum class PairingFailure(override val wire: String) : CatalogValue {
    NotPairingLink("not_pairing_link"), OutdatedHost("outdated_host"), Invalid("invalid"), Expired("expired"), Unauthorized("unauthorized"),
    Unreachable("unreachable"), Timeout("timeout"), UpgradeRequired("upgrade_required"), Other("other");

    companion object {
        fun of(error: Throwable): PairingFailure = when {
            error is PairingProblem -> when (error.kind) {
                PairingProblem.Kind.NotPairingLink -> NotPairingLink
                PairingProblem.Kind.OutdatedHost -> OutdatedHost
                PairingProblem.Kind.Invalid -> Invalid
            }
            // 400: a host from before Android phones turns the platform down; 403: the code expired or was used.
            error is HostError -> when (error.status) { 400 -> OutdatedHost; 403 -> Expired; 426 -> UpgradeRequired; else -> Other }
            // The computer's keys don't match the code's.
            error is SecurityException -> Unauthorized
            error.isTimeout -> Timeout
            error is IOException -> Unreachable
            else -> Other
        }
    }
}

/** A deadline or a socket timeout anywhere in the chain. */
internal val Throwable.isTimeout: Boolean
    get() = generateSequence(this) { it.cause }.any { it is InterruptedIOException || it is TimeoutCancellationException }

internal enum class PairedVia(override val wire: String) : CatalogValue { Qr("qr"), Link("link"), Account("account") }

internal enum class AccessFailure(override val wire: String) : CatalogValue { Denied("denied"), Expired("expired"), Cancelled("cancelled") }

internal enum class PromptSource(override val wire: String) : CatalogValue { Settings("settings"), Widgets("widgets"), Automatic("automatic") }

/** An alert's kind; `category` is the host's `AlertKind` the push carries. */
internal enum class NotificationKind(override val wire: String, val category: String) : CatalogValue {
    Done("done", "done"), NeedsInput("needs_input", "needsInput"), Failed("failed", "failed");

    companion object {
        fun of(category: String): NotificationKind? = entries.firstOrNull { it.category == category }
    }
}

internal enum class PhoneSetting(override val wire: String) : CatalogValue { Push("push"), LiveActivities("live_activities") }

/** What an outbound link leads to (`install_command`: an install command copied). */
internal enum class OutboundLink(override val wire: String) : CatalogValue {
    Github("github"), Issues("issues"), AppStore("app_store"), Demo("demo"), Tailscale("tailscale"), Privacy("privacy"), InstallCommand("install_command"),
}

internal enum class VoiceEngineKind(override val wire: String) : CatalogValue {
    OnDevice("on_device"), OpenAI("openai"), Gemini("gemini"),
    /** A provider's speech-to-text and text-to-speech (`CloudSpeechEngine`), not its realtime model. */
    Cloud("cloud"),
}

/** `screen_viewed` values (the catalog's *Screens* table, iOS/Android column). */
internal enum class AnalyticsScreen(override val wire: String) : CatalogValue {
    Bots("bots"), State("state"), Chat("chat"), Trace("trace"), BotDetails("bot_details"), Memory("memory"),
    Routines("routines"), Marketplace("marketplace"), Settings("settings"), Computers("computers"), Usage("usage"),
    Account("account"), Pairing("pairing"), VoiceSettings("voice_settings"), RemoteScreen("remote_screen"),
    Library("library"), Artifact("artifact"), Network("network"),
}
