package com.codync.android

import android.content.Context
import android.os.Build
import com.codync.android.core.CloudError
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
    data object ContinuedWithoutAccount : AnalyticsEvent("continued_without_account")
    data object ComputerFoundOnNetwork : AnalyticsEvent("computer_found_on_network")
    data object AddToAccountAsked : AnalyticsEvent("add_to_account_asked")
    data object AddToAccountCompleted : AnalyticsEvent("add_to_account_completed")
    data class AddToAccountFailed(val reason: AddToAccountFailure) : AnalyticsEvent("add_to_account_failed") {
        override val properties get() = mapOf("reason" to reason.json)
    }
    /** The user gave Codync Cloud the key to their own account with a cloud provider ([provider]: the cloud's id for it,
     * such as `boat`; iOS `cloudKeySaved`); never the key or its hint. */
    data class CloudKeySaved(val outcome: CloudKeyOutcome, val provider: String) : AnalyticsEvent("cloud_key_saved") {
        override val properties get() = mapOf("outcome" to outcome.json, "provider" to JsonPrimitive(provider))
    }
    /** The user opened a provider's free plan from its key sheet ([provider]: the cloud's id for it; iOS `cloudOfferOpened`). */
    data class CloudOfferOpened(val provider: String) : AnalyticsEvent("cloud_offer_opened") {
        override val properties get() = mapOf("provider" to JsonPrimitive(provider))
    }
    /** The user made a Codync Cloud computer on a provider ([provider]: the cloud's id for it; iOS `cloudComputerCreated`). */
    data class CloudComputerCreated(val outcome: CloudCreateOutcome, val provider: String) : AnalyticsEvent("cloud_computer_created") {
        override val properties get() = mapOf("outcome" to outcome.json, "provider" to JsonPrimitive(provider))
    }
    /** The user renamed one of their Codync Cloud computers; never the name (iOS `cloudComputerRenamed`). */
    data class CloudComputerRenamed(val outcome: CloudRenameOutcome) : AnalyticsEvent("cloud_computer_renamed") {
        override val properties get() = mapOf("outcome" to outcome.json)
    }
    /** The user deleted one of their Codync Cloud computers ([provider]: the cloud's id for the one it ran on; iOS `cloudComputerDeleted`). */
    data class CloudComputerDeleted(val outcome: CloudDeleteOutcome, val provider: String) : AnalyticsEvent("cloud_computer_deleted") {
        override val properties get() = mapOf("outcome" to outcome.json, "provider" to JsonPrimitive(provider))
    }
    /** The user deleted a Codync Cloud computer's snapshots ([provider]: the cloud's id for the one it runs on; iOS `cloudSnapshotsDeleted`). */
    data class CloudSnapshotsDeleted(val outcome: CloudDeleteOutcome, val provider: String) : AnalyticsEvent("cloud_snapshots_deleted") {
        override val properties get() = mapOf("outcome" to outcome.json, "provider" to JsonPrimitive(provider))
    }
    /** The user removed the key to their own account with [provider] (the cloud's id for it; iOS `cloudKeyRemoved`). */
    data class CloudKeyRemoved(val outcome: CloudRemoveOutcome, val provider: String) : AnalyticsEvent("cloud_key_removed") {
        override val properties get() = mapOf("outcome" to outcome.json, "provider" to JsonPrimitive(provider))
    }
    /** Signing in took the signed-out pairings ([MovedKeys]). */
    data class SignedOutPairingsMoved(val computers: Int, val keys: MovedKeys) : AnalyticsEvent("signed_out_pairings_moved") {
        override val properties get() = mapOf("computers" to JsonPrimitive(computers), "keys" to keys.json)
    }
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
    /** Rate on Google Play, in Settings (iOS `reviewLinkOpened`). */
    data object ReviewLinkOpened : AnalyticsEvent("review_link_opened") {
        override val properties get() = mapOf("source" to JsonPrimitive("settings"))
    }
    /** The What's new window is on screen after an update (iOS `whatsNewShown`). */
    data object WhatsNewShown : AnalyticsEvent("whats_new_shown")
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
    /** A computer's screen stopped streaming after a while with nobody touching the app: the bot details' picture
     * ([AnalyticsScreen.BotComputer]) or the full viewer ([AnalyticsScreen.RemoteScreen]). */
    data class RemoteScreenPaused(val screen: AnalyticsScreen) : AnalyticsEvent("remote_screen_paused") {
        override val properties get() = mapOf("screen" to screen.json)
    }
    /** A picture in a chat or the Library opened full size. Never the picture or its name. */
    data object ImagePreviewOpened : AnalyticsEvent("image_preview_opened")
    /**
     * Something done to a picture (at most one `zoom` per opening), where from, and how it went: `error` is set only when it
     * failed. Never the picture or its name.
     */
    data class ImagePreviewAction(val action: ImageAction, val from: ImageFrom, val error: ImageError? = null) : AnalyticsEvent("image_preview_action") {
        override val properties get() = mapOf("action" to action.json, "from" to from.json,
            "outcome" to JsonPrimitive(if (error == null) "done" else "failed")) + listOfNotNull(error?.let { "error" to it.json })
    }
    /** A dev server opened, or its address copied; the host reports what it does itself. */
    data class DevServerAction(val use: DevServerUse) : AnalyticsEvent("dev_server_action") {
        override val properties get() = mapOf("action" to use.json)
    }
    /** A voice note started from its beginning, or which step broke ([failure]); never which note. */
    data class VoiceNotePlayed(val failure: VoiceNoteFailure?) : AnalyticsEvent("voice_note_played") {
        override val properties get() =
            mapOf("outcome" to JsonPrimitive(if (failure == null) "played" else "failed")) + listOfNotNull(failure?.let { "error" to it.json })
    }
    /**
     * Something another app shared went to a bot (`ShareToBot`), was held for its offline computer, or failed ([error]
     * set only then): how many files, whether it had text, whether the bot came from Recent. Never names or content.
     */
    data class ShareSent(val outcome: ShareOutcome, val files: Int, val text: Boolean, val recent: Boolean, val error: ShareError? = null) :
        AnalyticsEvent("share_sent") {
        override val properties get() = mapOf("outcome" to outcome.json) + listOfNotNull(error?.let { "error" to it.json }) +
            mapOf("files" to JsonPrimitive(files), "text" to JsonPrimitive(text), "recent" to JsonPrimitive(recent))
    }
    /** Find in a conversation opened (iOS `conversationFindOpened`). */
    data object ConversationFindOpened : AnalyticsEvent("conversation_find_opened")
    /** A search in the find bar settled: whether it found anything, never the text (iOS `conversationFindSearched`). */
    data class ConversationFindSearched(val found: Boolean) : AnalyticsEvent("conversation_find_searched") {
        override val properties get() = mapOf("found" to JsonPrimitive(found))
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

internal enum class OnboardingStep(override val wire: String) : CatalogValue { Welcome("welcome"), Choose("choose"), Install("install"), Scan("scan") }

internal enum class OnboardingVia(override val wire: String) : CatalogValue { Paired("paired"), Account("account"), Skipped("skipped") }

internal enum class SignInProvider(override val wire: String) : CatalogValue { Apple("apple"), Google("google"), GitHub("github"), Hosted("hosted") }

/** The signed-out pairings' keys at sign-in: [Moved] with them, or listed to be scanned again ([Rescan]). */
internal enum class MovedKeys(override val wire: String) : CatalogValue { Moved("moved"), Rescan("rescan") }

internal enum class SignInFailure(override val wire: String) : CatalogValue {
    Cancelled("cancelled"), Timeout("timeout"), Verification("verification"), Error("error")
}

/** [Saved]; [Refused]: the cloud or the provider turned the key down (what to fix was shown); [Failed]: anything else. */
internal enum class CloudKeyOutcome(override val wire: String) : CatalogValue {
    Saved("saved"), Refused("refused"), Failed("failed");

    companion object {
        fun of(error: Throwable): CloudKeyOutcome = if (error is CloudError && error.code == "keyRefused") Refused else Failed
    }
}

/** [Created]; [Refused]: the cloud said what to fix (`409`: the limit, no key, no provider chosen; or `keyRefused`); [Failed]: anything else. */
internal enum class CloudCreateOutcome(override val wire: String) : CatalogValue {
    Created("created"), Refused("refused"), Failed("failed");

    companion object {
        fun of(error: Throwable): CloudCreateOutcome =
            if (error is CloudError && (error.status == 409 || error.code == "keyRefused")) Refused else Failed
    }
}

internal enum class CloudRenameOutcome(override val wire: String) : CatalogValue {
    Renamed("renamed"), Failed("failed")
}

/** [Deleted]; [Refused]: the cloud said no (`409`: its snapshots while it sleeps); [Failed]: anything else. */
internal enum class CloudDeleteOutcome(override val wire: String) : CatalogValue {
    Deleted("deleted"), Refused("refused"), Failed("failed");

    companion object {
        fun of(error: Throwable): CloudDeleteOutcome = if (error is CloudError && error.status == 409) Refused else Failed
    }
}

/** [Removed]; [Refused]: the cloud said no (`409`: a computer still runs on that provider); [Failed]: anything else. */
internal enum class CloudRemoveOutcome(override val wire: String) : CatalogValue {
    Removed("removed"), Refused("refused"), Failed("failed");

    companion object {
        fun of(error: Throwable): CloudRemoveOutcome = if (error is CloudError && error.status == 409) Refused else Failed
    }
}

internal enum class AddToAccountFailure(override val wire: String) : CatalogValue {
    Declined("declined"), Certificate("certificate"), Unreachable("unreachable"), Other("other");

    companion object {
        fun of(error: Throwable): AddToAccountFailure = when {
            error is HostError && error.message == "The computer stayed out of this account." -> Declined
            error is HostError && error.message.startsWith("Couldn't confirm your Codync account") -> Certificate
            error is HostError -> Other
            error is CloudError -> if (error.isTransient) Unreachable else Other
            error is IOException && error.message == "Connect to the internet to pair, so Codync can confirm your account." -> Certificate
            error is IOException -> Unreachable
            else -> Other
        }
    }
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
            // 400: a host from before Android phones turns the platform down. 403: the code expired or was used, or the
            // computer turned this phone away (another account, declined, an account it couldn't confirm), as on iOS.
            error is HostError -> when (error.status) {
                400 -> OutdatedHost
                403 -> if (error.message.startsWith("This pairing code expired")) Expired else Unauthorized
                426 -> UpgradeRequired
                else -> Other
            }
            // The cloud answered (a short code that's spent, an account it couldn't confirm): as on iOS.
            error is CloudError -> Other
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

/** What an outbound link leads to (`install_command`: an install command copied; `install_link`: the website link copied). */
internal enum class OutboundLink(override val wire: String) : CatalogValue {
    Github("github"), Issues("issues"), AppStore("app_store"), Demo("demo"), Tailscale("tailscale"), Privacy("privacy"), InstallCommand("install_command"),
    InstallLink("install_link"),
}

internal enum class DevServerUse(override val wire: String) : CatalogValue { Open("open"), Copy("copy") }
internal enum class VoiceNoteFailure(override val wire: String) : CatalogValue { Load("load"), Playback("playback") }

internal enum class ShareOutcome(override val wire: String) : CatalogValue { Sent("sent"), Queued("queued"), Failed("failed") }

/**
 * What stopped a share on Android: a file it couldn't copy in ([Read]), or the message it couldn't hand to the chat
 * ([Send]). The chat's own send reports the rest; the iPhone, sending from its share sheet, also has the catalog's others.
 */
internal enum class ShareError(override val wire: String) : CatalogValue { Read("read"), Send("send") }

internal enum class VoiceEngineKind(override val wire: String) : CatalogValue {
    OnDevice("on_device"), OpenAI("openai"), Gemini("gemini"),
    /** A provider's speech-to-text and text-to-speech (`CloudSpeechEngine`), not its realtime model. */
    Cloud("cloud"),
}

internal enum class ImageAction(override val wire: String) : CatalogValue {
    Zoom("zoom"), CopyImage("copy_image"), CopyName("copy_name"), Download("download"),
}

/** Where a picture action was done: the full-size viewer, or a picture's long-press menu in a chat or the Library. */
internal enum class ImageFrom(override val wire: String) : CatalogValue { Viewer("viewer"), Menu("menu") }

/** Why a picture action failed: its file isn't there, couldn't be read, the clipboard refused it, or a download failed. */
internal enum class ImageError(override val wire: String) : CatalogValue {
    Unavailable("unavailable"), Convert("convert"), Clipboard("clipboard"), Transfer("transfer")
}

/** `screen_viewed` values (the catalog's *Screens* table, iOS/Android column). */
internal enum class AnalyticsScreen(override val wire: String) : CatalogValue {
    Bots("bots"), State("state"), Chat("chat"), Trace("trace"), BotDetails("bot_details"), Memory("memory"),
    Routines("routines"), Marketplace("marketplace"), Settings("settings"), Computers("computers"), Usage("usage"),
    Account("account"), Pairing("pairing"), VoiceSettings("voice_settings"), RemoteScreen("remote_screen"),
    Library("library"), Artifact("artifact"), Network("network"), BotComputer("bot_computer"),
}
