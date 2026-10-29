package com.codync.android

import android.content.ClipData
import android.content.ClipDescription
import android.os.Build
import android.os.PersistableBundle
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.Routine
import com.codync.android.core.RoutineDraft
import com.codync.android.core.RoutineScheduleDraft
import com.codync.android.core.RoutineSchedulePreview
import com.codync.android.core.RoutineWebhook
import com.codync.android.core.canEdit
import com.codync.android.core.routineRepeats
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.text.DateFormat
import java.util.Date
import java.util.TimeZone
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * A bot's routines, on its details' Details page (iOS `RoutinesView`); a row opens its editor.
 * `initialId`: open on that routine's editor (a run's notice in the chat was tapped). `leave` goes back to the chat once a
 * request to the bot is in the composer.
 */
@Composable fun RoutinesView(bot: Bot, state: AppState, store: AppStore, initialId: String?, leave: () -> Unit) {
    val online = state.actualConnection is LinkState.Ready
    val listing = state.routines[bot.id]
    val busy = "routines:${bot.id}" in state.busy
    // The open editor: on a routine (its id), or "" setting up a new one.
    var editing by remember(bot.id) { mutableStateOf(initialId) }
    LaunchedEffect(bot.id, online) { if (online) while (true) { store.loadRoutines(bot.id); delay(3_000) } }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SectionHeading("Routines", Modifier.weight(1f).padding(start = 2.dp))
            IconButton("Ask the bot for a routine", "text.bubble", enabled = online) { store.editDraft(bot.id, "I want a routine that "); leave() }
            IconButton("Set up a routine", "plus", enabled = online) { editing = "" }
        }
        val routines = listing?.routines.orEmpty()
        if (routines.isEmpty()) Text(tr(if (listing != null) "No routines yet. Ask the bot for one, or set it up yourself with +." else "Loading routines…"),
            Modifier.outlinedCard(vertical = 14.dp), style = Type.system(12f).copy(lineHeight = Type.system(12f).lineHeight * 1.25f), color = Palette.secondary)
        else Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (routine in routines) {
                Row(Modifier.outlinedCard(vertical = 0.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Column(Modifier.weight(1f).plainButton { editing = routine.id }.padding(vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        Text(routine.name, style = Type.system(13f), color = Palette.text)
                        Text(listing?.summary(routine).orEmpty(), style = Type.caption, color = if (routine.lastError == null) Palette.secondary else Palette.danger)
                    }
                    CodyncSwitch(routine.enabled, { store.routineAction(bot.id, routine.id, "setRoutineEnabled", it) },
                        Modifier.width(34.dp).semantics { contentDescription = routine.name }, enabled = online && !busy) {}
                }
            }
        }
    }
    // Item-based, so the editor always gets the routine it was opened on; it opens once that routine is listed.
    CodyncSheet(editing?.takeIf { listing.canEdit(it) }, { editing = null }) { id ->
        // The list is part of the bot's details; a routine's editor is the routines screen, as on the iPhone.
        AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Routines))
        RoutineEditor(bot.id, listing?.routines?.firstOrNull { it.id == id }, state, store) { saved ->
            // A new webhook routine stays open: its URL and key exist only now.
            editing = if (id.isEmpty() && saved?.hasWebhook == true) saved.id else null
        }
    }
}

/** Set up or edit one routine (iOS `RoutineEditorView`); `saved` gets the routine, or null once it's deleted. */
@Composable private fun RoutineEditor(botId: String, routine: Routine?, state: AppState, store: AppStore, saved: (Routine?) -> Unit) {
    var draft by remember(routine?.id) {
        mutableStateOf(RoutineDraft(routine?.id, routine?.name.orEmpty(), routine?.instruction.orEmpty(), timeoutSeconds = (routine?.timeoutSeconds ?: 3600).toString()))
    }
    var loaded by remember(routine?.id) { mutableStateOf(false) }
    var loadAttempt by remember { mutableIntStateOf(0) }
    var checkAttempt by remember { mutableIntStateOf(0) }
    var preview by remember { mutableStateOf<RoutineSchedulePreview?>(null) }
    var checked by remember { mutableStateOf<RoutineScheduleDraft?>(null) }
    var previewError by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var advanced by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var choosingZone by remember { mutableStateOf(false) }
    var started by remember { mutableStateOf(false) }
    val busy = "routines:$botId" in state.busy
    val online = state.actualConnection is LinkState.Ready
    LaunchedEffect(routine?.id, loadAttempt) {
        try {
            val result = store.routineSchedule(routine?.triggers.orEmpty(), TimeZone.getDefault().id)
            draft = draft.copy(schedule = result.draft.forMobileEditor())
            preview = result; checked = draft.schedule; loaded = true; error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Connect to this computer to load the schedule." }
    }
    // The host checks the schedule and describes when it runs.
    LaunchedEffect(draft.schedule, loaded, checkAttempt) {
        if (!loaded || checked == draft.schedule) return@LaunchedEffect
        val requested = draft.schedule
        preview = null; previewError = null; checked = null
        delay(300)
        try {
            val result = store.routineSchedule(requested)
            if (draft.schedule == requested) { preview = result; checked = requested }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { if (draft.schedule == requested) previewError = failure.message ?: "Couldn't check this schedule." }
    }
    val canSave = draft.name.isNotBlank() && draft.instruction.isNotBlank() && online && !busy && loaded && checked == draft.schedule && previewError == null
    val options = buildList {
        if (routine != null && routine.keepsOriginal && routine.triggers.isNotEmpty()) add("keep" to routine.triggerDescriptions.joinToString(" · "))
        add("cron" to "Schedule"); add("webhook" to "Webhook")
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(if (routine == null) "Set up a routine" else "Edit routine")
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Field("Name") { FieldBox(draft.name, { draft = draft.copy(name = it) }, "e.g. Morning summary", fill = Palette.bubbleUser) }
                Field("Instruction") {
                    FieldBox(draft.instruction, { draft = draft.copy(instruction = it) }, "Describe what this bot should do each time it runs.", 3, 8, fill = Palette.bubbleUser)
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Field("When to run") {
                    ChoicePicker(draft.schedule.kind, options, { draft = draft.copy(schedule = draft.schedule.copy(kind = it)) }, label = "When to run")
                }
                if (!loaded) IconLabel("Loading schedule…", "clock", Palette.secondary, style = Type.body)
                else when (draft.schedule.kind) {
                    "cron" -> Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        val schedule = draft.schedule
                        fun change(edit: RoutineScheduleDraft) { draft = draft.copy(schedule = edit) }
                        val current = preview.takeIf { checked == schedule }
                        Field("Repeat") {
                            ChoicePicker(schedule.calendarStyle, routineRepeats, { change(schedule.repeating(it, current?.cronExpression)) }, label = "Repeat")
                        }
                        when (schedule.calendarStyle) {
                            "hourly" -> Field("At minute") { ChoicePicker(schedule.minute, MINUTES, { change(schedule.copy(minute = it)) }, label = "At minute") }
                            "daily", "weekdays" -> TimeField(schedule) { change(it) }
                            "weekly" -> {
                                Field("On") { ChoicePicker(schedule.weekday, WEEKDAYS, { change(schedule.copy(weekday = it)) }, label = "On") }
                                TimeField(schedule) { change(it) }
                            }
                            else -> Field("Cron") {
                                FieldBox(schedule.expression, { change(schedule.copy(expression = it)) }, "0 9 * * 1-5",
                                    style = Type.body.monospaced, fill = Palette.bubbleUser, capitalize = false)
                            }
                        }
                        Field("Time zone") { ZoneButton(schedule.zone) { choosingZone = true } }
                        SchedulePreview(current, previewError, cronHint = schedule.calendarStyle == "custom")
                    }
                    "webhook" -> if (routine?.hasWebhook == true) RoutineWebhookPanel(botId, routine.id, online, store)
                        else Note("Runs each time something is posted to its URL. Save to get the URL and key.")
                    else -> Note("Set up by the bot. Pick Schedule or Webhook to replace it.")
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val turn by animateFloatAsState(if (advanced) 180f else 0f, Motion.layout(), label = "Chevron")
                Row(Modifier.fillMaxWidth().plainButton { advanced = !advanced }.padding(start = 4.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Run settings", style = Type.subheadline, color = Palette.secondary)
                    Symbol("chevron.down", Modifier.rotate(turn), size = 11.dp, tint = Palette.secondary)
                    Spacer(Modifier.weight(1f))
                    if (!advanced) Text("Timeout ${draft.timeoutText}", style = Type.subheadline, color = Palette.tertiary)
                }
                AnimatedVisibility(advanced, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                    exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Field("Timeout (seconds)") { FieldBox(draft.timeoutSeconds, { draft = draft.copy(timeoutSeconds = it) }, "3600", fill = Palette.bubbleUser) }
                        Note("A run is stopped after this long. 1 second to 24 hours.")
                    }
                }
            }
        }
        // The footer: what keeps it from saving, the routine's own actions, then Save.
        Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 36.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (previewError != null) LinkButton("Check schedule again", enabled = online) { checkAttempt++ }
            if (!loaded && error != null) LinkButton("Retry loading schedule") { loadAttempt++ }
            if (draft.schedule.kind != "cron") previewError?.let { Text(it, style = Type.caption, color = Palette.danger) }
            error?.let { IconLabel(it, "exclamationmark.circle", Palette.danger, style = Type.callout) }
            if (!online) IconLabel("Connect to this computer to save your routine.", "wifi.slash", Palette.danger, style = Type.caption)
            if (loaded && online && (draft.name.isBlank() || draft.instruction.isBlank()))
                Text("Enter a name and instruction above to save.", style = Type.caption, color = Palette.secondary)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                if (routine != null) {
                    IconButton("Delete routine", "trash") { confirmDelete = true }
                    IconButton(if (started) "Started" else "Test run", if (started) "checkmark" else "play.circle", enabled = !started && online) {
                        store.routineAction(botId, routine.id, "runRoutine") { started = true }
                    }
                }
                PrimaryButton({ store.saveRoutine(botId, draft) { saved(it) } }, Modifier.weight(1f), canSave) {
                    Row(Modifier.heightIn(min = 26.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (busy) Spinner(color = Palette.onAccent)
                        Text(if (busy) "Saving…" else if (routine == null) "Create routine" else "Save changes")
                    }
                }
            }
        }
    }
    CodyncDialog("Delete routine?", confirmDelete, { confirmDelete = false }, "This deletes the routine and stops its future runs. This can't be undone.") {
        listOf(DialogAction("Delete routine", destructive = true) { routine?.let { store.routineAction(botId, it.id, "deleteRoutine") { saved(null) } } })
    }
    CodyncSheet(choosingZone, { choosingZone = false }) {
        ZonePicker(draft.schedule.zone) { draft = draft.copy(schedule = draft.schedule.copy(zone = it)); choosingZone = false }
    }
}

private fun Int.twoDigits() = toString().padStart(2, '0')
private val HOURS = (0..23).map { it to it.twoDigits() }
private val MINUTES = (0..59).map { it to it.twoDigits() }
// Monday first; the host counts 0 = Sunday … 6 = Saturday.
private val WEEKDAYS = listOf(1 to "Monday", 2 to "Tuesday", 3 to "Wednesday", 4 to "Thursday", 5 to "Friday", 6 to "Saturday", 0 to "Sunday")

/** "At": the hour and minute a daily, weekday or weekly schedule runs. */
@Composable private fun TimeField(schedule: RoutineScheduleDraft, change: (RoutineScheduleDraft) -> Unit) = Field("At") {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ChoicePicker(schedule.hour, HOURS, { change(schedule.copy(hour = it)) }, label = "Hour")
        Text(":", style = Type.body, color = Palette.secondary)
        ChoicePicker(schedule.minute, MINUTES, { change(schedule.copy(minute = it)) }, label = "Minute")
    }
}

/** One quiet line under the schedule fields: what the host read, or why it can't (with the cron fields' order while it checks a Custom one). */
@Composable private fun SchedulePreview(preview: RoutineSchedulePreview?, error: String?, cronHint: Boolean) {
    Column(Modifier.padding(start = 4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        when {
            error != null -> Text(tr(error), style = Type.caption, color = Palette.danger)
            preview != null -> {
                Text(preview.summary, style = Type.caption.medium, color = Palette.text)
                preview.nextRunAt?.let {
                    val whenText = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it))
                    Text(tr("Next {when}").replace("{when}", whenText), style = Type.caption, color = Palette.secondary)
                }
                preview.warning?.let { Text(tr(it), style = Type.caption, color = Palette.secondary) }
            }
            cronHint -> Text("minute  hour  day  month  weekday", style = Type.caption.monospaced, color = Palette.tertiary)
        }
    }
}

@Composable private fun Note(text: String) = Text(tr(text), Modifier.padding(start = 4.dp), style = Type.caption, color = Palette.secondary)

/** A saved webhook routine's address and key: copy either, reveal or replace the key (iOS `RoutineWebhookPanel`). */
@Composable private fun RoutineWebhookPanel(botId: String, id: String, online: Boolean, store: AppStore) {
    var webhook by remember(id) { mutableStateOf<RoutineWebhook?>(null) }
    var showKey by remember(id) { mutableStateOf(false) }
    var confirm by remember(id) { mutableStateOf(false) }
    var rotating by remember(id) { mutableStateOf(false) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    var copied by remember(id) { mutableStateOf<String?>(null) }
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    suspend fun load(rotate: Boolean) {
        rotating = rotate
        try {
            webhook = store.routineWebhook(botId, id, rotate); error = null
            if (rotate) { copied = null; showKey = true }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Connect to this computer to see the webhook." }
        finally { rotating = false }
    }
    LaunchedEffect(id) { load(false) }
    fun copy(field: String, value: RoutineWebhook) = scope.launch {
        val clip = ClipData.newPlainText("Routine $field", if (field == "key") value.key else value.url ?: value.localUrl)
        // Keys stay out of the clipboard preview.
        if (field == "key" && Build.VERSION.SDK_INT >= 33) clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
        clipboard.setClipEntry(ClipEntry(clip)); copied = field
    }
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        val value = webhook
        when {
            value != null -> {
                Field(if (value.url == null) "Local URL" else "URL") {
                    WebhookValue(value.url ?: value.localUrl) {
                        IconButton(if (copied == "url") "Copied" else "Copy", if (copied == "url") "checkmark" else "doc.on.doc") { copy("url", value) }
                    }
                }
                Field("Key") {
                    WebhookValue(if (showKey) value.key else "•".repeat(16)) {
                        IconButton(if (showKey) "Hide key" else "Show key", if (showKey) "eye.slash" else "eye") { showKey = !showKey }
                        IconButton("Replace key", "arrow.triangle.2.circlepath", enabled = !rotating && online) { confirm = true }
                        IconButton(if (copied == "key") "Copied" else "Copy", if (copied == "key") "checkmark" else "doc.on.doc") { copy("key", value) }
                    }
                }
                Note(if (value.url == null) "The Codync cloud is off, so only this computer can send to it."
                    else "POST with Authorization: Bearer <key>. For GitHub, use content type application/json and the key as the secret. " +
                        "Deliveries wait up to 72 hours while this computer is off. They pass through the Codync cloud, which can read them.")
            }
            error != null -> Text(tr(error.orEmpty()), Modifier.padding(start = 4.dp), style = Type.caption, color = Palette.danger)
            else -> Note("Loading webhook…")
        }
    }
    CodyncDialog("Replace the key?", confirm, { confirm = false }, "Senders using the current key stop working until they get the new one.") {
        listOf(DialogAction("Replace key", destructive = true) { scope.launch { load(true) } })
    }
}

/** A selectable monospaced value with its buttons. */
@Composable private fun WebhookValue(text: String, buttons: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f)) {
            SelectionContainer { Text(text, style = Type.callout.monospaced, color = Palette.text, maxLines = 2, overflow = TextOverflow.MiddleEllipsis) }
        }
        buttons()
    }
}
