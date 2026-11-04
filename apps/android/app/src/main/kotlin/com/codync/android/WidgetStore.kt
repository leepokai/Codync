package com.codync.android

import android.content.Context
import android.util.AtomicFile
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.serialization.Serializable

@Serializable internal data class WidgetOwner(val context: String, val scope: String, val epoch: String)

/** Every write checks the original account activation, including background responses. */
internal object WidgetStore {
    /** How long a card keeps saying "Sending…" for an answer whose work hasn't finished. */
    private const val SENDING_MS = 90_000L
    val changes = MutableStateFlow(0L)
    private fun activeFile(context: Context) = AtomicFile(File(context.noBackupFilesDir, "widget-owner.json"))
    private fun feedFile(context: Context, owner: WidgetOwner) = AtomicFile(File(LocalStore(context, owner.context).directory, "widget-feed.json"))

    @Synchronized fun owner(context: Context): WidgetOwner? = read<WidgetOwner>(activeFile(context))

    @Synchronized fun activate(context: Context, account: String): WidgetOwner {
        val owner = WidgetOwner(account, LocalStore.referenceId(account), UUID.randomUUID().toString())
        write(activeFile(context), owner)
        changed()
        return owner
    }

    @Synchronized fun retire(context: Context, expected: WidgetOwner?) {
        if (expected != null && owner(context) == expected) { activeFile(context).delete(); changed() }
    }

    @Synchronized fun feed(context: Context): WidgetFeed {
        val active = owner(context) ?: return WidgetFeed()
        val saved = read<WidgetFeed>(feedFile(context, active))
        val paired = runCatching { LocalStore(context, active.context).computers().firstOrNull()?.id }.getOrNull()
        return saved?.takeIf { it.scope == active.scope && it.computerId == paired } ?: WidgetFeed(scope = active.scope)
    }

    @Synchronized fun publish(context: Context, expected: WidgetOwner, computer: Computer?, bots: List<Bot>,
        usage: UsageReport, freshBots: Boolean, freshUsage: Boolean, now: Long = System.currentTimeMillis(),
        expectedRevision: Long? = null, approvals: List<WidgetApproval> = emptyList()): Boolean {
        if (owner(context) != expected || expectedRevision != null && changes.value != expectedRevision) return false
        val file = feedFile(context, expected)
        val previous = read<WidgetFeed>(file)?.takeIf { it.computerId == computer?.id } ?: WidgetFeed(scope = expected.scope)
        val next = WidgetFeed(expected.scope, computer?.id, computer?.name.orEmpty().take(256), widgetBots(bots),
            if (freshBots) now else previous.botsAt, usage, if (freshUsage) now else
                if (usage != previous.usage) usage.providers.mapNotNull { it.updatedAt?.takeIf { time -> time > 0 } }.minOrNull() ?: 0
                else previous.usageAt,
            // A read that only refreshed usage keeps what it knew; a report of the bots replaces it, and a read that saw none leaves none.
            approvals = if (freshBots) approvals.map { it.copy(at = now) } else previous.approvals)
        if (previous == next) return true
        write(file, next)
        changed()
        return true
    }

    /** A background read failed: the widgets say so instead of showing the last numbers as current. The next report clears it. */
    @Synchronized fun markOffline(context: Context, expected: WidgetOwner, now: Long = System.currentTimeMillis()) {
        if (owner(context) != expected) return
        val file = feedFile(context, expected)
        val previous = read<WidgetFeed>(file) ?: return
        if (previous.offlineSince > 0) return
        write(file, previous.copy(offlineSince = now))
        changed()
    }

    @Synchronized fun selectProvider(context: Context, widget: Int, provider: String) {
        require(provider.isNotBlank() && provider.length <= 128 && provider.none(Char::isISOControl))
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().putString("provider:$widget", provider).commit())
        changed()
    }
    fun provider(context: Context, widget: Int): String = context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE)
        .getString("provider:$widget", "claude") ?: "claude"
    /** Usage shows each provider as a character, or as its own logo ("Choose the look"), in the app and its widgets. */
    fun originalIcons(context: Context): Boolean = context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).getBoolean("originalIcons", false)
    @Synchronized fun setOriginalIcons(context: Context, on: Boolean) {
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().putBoolean("originalIcons", on).commit())
        changed()
    }

    /** The Team widget's chosen bot per spot; empty spots fill themselves. */
    @Synchronized fun selectTeam(context: Context, widget: Int, picks: List<String?>) {
        require(picks.size == 4 && picks.all { it == null || it.length <= 128 && it.none(Char::isISOControl) })
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit()
            .putString("team:$widget", picks.joinToString("\n") { it.orEmpty() }).commit())
        changed()
    }
    fun team(context: Context, widget: Int): List<String?> = context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE)
        .getString("team:$widget", null)?.split("\n")?.map { it.ifEmpty { null } }?.takeIf { it.size == 4 } ?: List(4) { null }
    /** What the person chose for one overview widget; a damaged or missing value reads as the defaults. */
    fun overview(context: Context, widget: Int): OverviewConfig =
        OverviewConfig.decode(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).getString("overview:$widget", null))
    @Synchronized fun selectOverview(context: Context, widget: Int, config: OverviewConfig) {
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().putString("overview:$widget", config.encode()).commit())
        changed()
    }
    /** Requests a widget has answered and not yet seen leave the report: the card says "Sending…" instead of offering the buttons again. */
    @Synchronized fun sending(context: Context, entry: String, now: Long = System.currentTimeMillis()) {
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().putLong("sending:$entry", now).commit())
        changed()
    }
    @Synchronized fun sent(context: Context, entry: String) {
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().remove("sending:$entry").commit())
        changed()
    }
    fun isSending(context: Context, entry: String, now: Long = System.currentTimeMillis()): Boolean =
        now - context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).getLong("sending:$entry", 0) in 0..SENDING_MS

    @Synchronized fun remove(context: Context, ids: IntArray) {
        val editor = context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit()
        ids.forEach { editor.remove("provider:$it"); editor.remove("team:$it"); editor.remove("overview:$it") }; check(editor.commit()); changed()
    }
    @Synchronized fun reset(context: Context) {
        activeFile(context).delete()
        check(context.getSharedPreferences("codync-widgets", Context.MODE_PRIVATE).edit().clear().commit())
        changed()
    }
    @Synchronized fun refreshLanguage() { changed() }
    private fun changed() { changes.value++ }
    private inline fun <reified T> read(file: AtomicFile): T? = try {
        if (!file.baseFile.exists() || file.baseFile.length() > 512 * 1024) null
        else WireJson.decodeFromString<T>(file.readFully().toString(Charsets.UTF_8))
    } catch (_: Exception) { null }
    private inline fun <reified T> write(file: AtomicFile, value: T) {
        val bytes = WireJson.encodeToString(value).toByteArray()
        require(bytes.size <= 512 * 1024) { "The widget report is too large." }
        val output = file.startWrite()
        try { output.write(bytes); file.finishWrite(output) }
        catch (error: Throwable) { file.failWrite(output); throw error }
    }
}
