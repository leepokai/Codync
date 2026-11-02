import CodyncKit
import SwiftUI

struct RoutineEditorView: View {
    let botId: String
    let routine: Routine?
    /// Called after a save (the routine) or a delete (nil).
    let saved: (Routine?) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var name = ""
    @State private var instruction = ""
    @State private var schedule = RoutineScheduleDraft()
    @State private var timeout = "3600"
    @State private var busy = false
    @State private var error: String?
    @State private var advanced = false
    @State private var loaded = false
    @State private var preview: RoutineSchedulePreview?
    @State private var previewError: String?
    @State private var checkedSchedule: RoutineScheduleDraft?
    @State private var confirmDelete = false
    @State private var started = false
    @State private var pickingZone = false
    /// Any text field is being edited; cleared on save so nothing sits under the keyboard.
    @FocusState private var typing: Bool

    /// Schedule or webhook, as in Claude Code routines. Anything else the bot set up
    /// (a one-off time, an interval, events, several triggers) stays as it is until replaced.
    private var options: [(String, String)] {
        var values = [("cron", "Schedule"), ("webhook", "Webhook")]
        if let routine, keepsOriginal {
            values.insert(("keep", routine.triggerDescriptions.joined(separator: " · ")), at: 0)
        }
        return values
    }

    private var keepsOriginal: Bool {
        guard let routine, routine.triggers.count == 1 else { return routine != nil && !(routine?.triggers.isEmpty ?? true) }
        return !["cron", "webhook"].contains(routine.triggers[0].type)
    }

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        !instruction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !model.isOffline && !busy &&
        loaded && checkedSchedule == schedule && previewError == nil
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(routine == nil ? "Set up a routine" : "Edit routine")
            Group {
                ScrollView { form }.scrollDismissesKeyboard(.interactively)
            }
            .disabled(busy || !loaded)
            footer
        }
        .font(.system(size: 16))
        .foregroundStyle(Palette.text)
        .background(Palette.background)
        .codyncDialog("Delete routine?", isPresented: $confirmDelete, message: "This deletes the routine and stops its future runs. This can't be undone.") {
            [DialogAction("Delete routine", destructive: true) {
                guard let routine else { return }
                Task {
                    do {
                        try await model.client?.routineAction("deleteRoutine", botId: botId, id: routine.id)
                        saved(nil)
                    } catch { self.error = error.localizedDescription }
                }
            }]
        }
        .codyncSheet(isPresented: $pickingZone) {
            TimeZonePicker(selection: schedule.zone) {
                schedule.zone = $0
                pickingZone = false
            }
        }
        .task {
            if let routine {
                name = routine.name
                instruction = routine.instruction
                timeout = String(routine.timeoutSeconds ?? 3600)
            }
            await loadSchedule()
        }
        .task(id: schedule) {
            guard loaded else { return }
            do { try await Task.sleep(for: .milliseconds(300)) }
            catch { return }
            await checkSchedule()
        }
    }

    private var form: some View {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 14) {
                    Field("Name") {
                        TextField("Name", text: $name, prompt: Text(L10n.tr("e.g. Morning summary")).foregroundStyle(Palette.secondary)).routineInput().focused($typing)
                    }
                    Field("Instruction") {
                        TextField("Instruction", text: $instruction, prompt: Text(L10n.tr("Describe what this bot should do each time it runs.")).foregroundStyle(Palette.secondary), axis: .vertical)
                            .lineLimit(3...8).routineInput().focused($typing)
                    }
                }
                VStack(alignment: .leading, spacing: 14) {
                    Field("When to run") {
                        ChoicePicker(selection: $schedule.kind, options: options.map { (id: $0.0, label: $0.1) }, fitsAvailableWidth: true)
                    }
                    if loaded { triggerFields }
                    else { Label(L10n.tr("Loading schedule…"), systemImage: "clock").foregroundStyle(Palette.secondary) }
                }
                VStack(alignment: .leading, spacing: 12) {
                    Button {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { advanced.toggle() }
                    } label: {
                        HStack(spacing: 6) {
                            Text(L10n.tr("Run settings"))
                            Image(systemName: "chevron.down").font(.caption2.weight(.semibold))
                                .rotationEffect(.degrees(advanced ? 180 : 0))
                            Spacer()
                            if !advanced { Text(L10n.tr("Timeout \(Routine.timeoutText(timeout))")).foregroundStyle(Palette.tertiary) }
                        }
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .padding(.leading, 4)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityValue(advanced ? "Expanded" : "Collapsed")
                    if advanced {
                        Field("Timeout (seconds)") { TextField("3600", text: $timeout).routineInput().focused($typing) }
                        Text(L10n.tr("A run is stopped after this long. 1 second to 24 hours."))
                            .font(.caption).foregroundStyle(Palette.secondary).padding(.leading, 4)
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func checkSchedule() async {
        let requested = schedule
        preview = nil
        previewError = nil
        checkedSchedule = nil
        do {
            guard let client = model.client else { throw URLError(.notConnectedToInternet) }
            let result = try await client.routineSchedule(draft: requested)
            try Task.checkCancellation()
            guard schedule == requested else { return }
            preview = result
            checkedSchedule = requested
        } catch {
            guard !Task.isCancelled, schedule == requested else { return }
            previewError = error.localizedDescription
        }
    }

    private func loadSchedule() async {
        guard let client = model.client else { error = "Connect to this computer to load the schedule."; return }
        do {
            let result = try await client.routineSchedule(triggers: routine?.triggers ?? [], timeZone: TimeZone.current.identifier)
            schedule = result.draft.forMobileEditor()
            preview = result
            checkedSchedule = result.draft
            loaded = true
            error = nil
        } catch { self.error = error.localizedDescription }
    }

    @ViewBuilder private var triggerFields: some View {
        switch schedule.kind {
        case "cron":
            cronFields
        case "webhook":
            if let routine, routine.triggers.contains(where: { $0.type == "webhook" || $0.type == "event" }) {
                RoutineWebhookPanel(botId: botId, routineId: routine.id)
            } else {
                Text(L10n.tr("Runs each time something is posted to its URL. Save to get the URL and key."))
                    .font(.caption).foregroundStyle(Palette.secondary).padding(.leading, 4)
                    .fixedSize(horizontal: false, vertical: true)
            }
        default:
            Text(L10n.tr("Set up by the bot. Pick Schedule or Webhook to replace it."))
                .font(.caption).foregroundStyle(Palette.secondary).padding(.leading, 4)
        }
    }

    /// A frequency with its time, or cron written directly; the host compiles, checks and describes it.
    private var cronFields: some View {
        VStack(alignment: .leading, spacing: 14) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 10) { frequencyFields }
                VStack(alignment: .leading, spacing: 14) { frequencyFields }
            }
            VStack(alignment: .leading, spacing: 8) {
                if schedule.calendarStyle == "custom" {
                    ViewThatFits(in: .horizontal) {
                        HStack(alignment: .top, spacing: 10) { expressionField; zoneField }
                        VStack(alignment: .leading, spacing: 14) { expressionField; zoneField }
                    }
                } else {
                    zoneField
                }
                schedulePreview.padding(.leading, 4)
            }
        }
    }

    @ViewBuilder private var frequencyFields: some View {
        Field("Repeat") {
            ChoicePicker(selection: Binding(get: { schedule.calendarStyle }, set: { selectFrequency($0) }), options: RoutineScheduleDraft.repeatChoices)
        }
        switch schedule.calendarStyle {
        case "hourly":
            Field("At minute") {
                ChoicePicker(selection: $schedule.minute, options: (0..<60).map { (id: $0, label: String(format: ":%02d", $0)) })
            }
        case "daily", "weekdays":
            timeField
        case "weekly":
            Field("Weekday") {
                // Monday first; the host counts 0 = Sunday … 6 = Saturday, like cron.
                ChoicePicker(selection: $schedule.weekday,
                             options: [1, 2, 3, 4, 5, 6, 0].map { (id: $0, label: Calendar.current.standaloneWeekdaySymbols[$0]) })
            }
            timeField
        default:
            EmptyView()
        }
    }

    private var timeField: some View {
        Field("At") {
            DatePicker("At", selection: $schedule.timeOfDay, displayedComponents: .hourAndMinute)
                .labelsHidden()
                .environment(\.timeZone, .gmt)
        }
    }

    /// Custom starts from the cron the host compiled for the checked choice it replaces.
    private func selectFrequency(_ style: String) {
        let compiled = checkedSchedule == schedule ? preview?.cronExpression : nil
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { schedule = schedule.repeating(style, compiled: compiled) }
    }

    private var expressionField: some View {
        Field("Cron") {
            TextField("Cron", text: $schedule.expression, prompt: Text(L10n.tr("0 9 * * 1-5")).foregroundStyle(Palette.secondary))
                .font(.body.monospaced()).autocorrectionDisabled().routineInput().focused($typing)
                .frame(minWidth: 220)
        }
    }

    private var zoneField: some View {
        Field("Time zone") {
            Button { pickingZone = true } label: {
                HStack(spacing: 6) {
                    Text(schedule.zone).lineLimit(1)
                    Text(TimeZones.offset(schedule.zone)).foregroundStyle(Palette.secondary)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
                .foregroundStyle(Palette.text)
                .outlinedPill()
            }
            .buttonStyle(.plain)
        }
    }

    /// One quiet line under the fields: what the host read, or why it can't.
    @ViewBuilder private var schedulePreview: some View {
        let preview = checkedSchedule == schedule ? preview : nil
        if let previewError {
            Text(previewError).font(.caption).foregroundStyle(Palette.danger)
                .fixedSize(horizontal: false, vertical: true)
        } else if let preview {
            VStack(alignment: .leading, spacing: 2) {
                Text(preview.draft.spoken).font(.caption.weight(.medium))
                if let next = preview.nextRunAt {
                    Text(scheduleNextLine(next))
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
                if let warning = preview.warning {
                    Text(warning).font(.caption).foregroundStyle(Palette.secondary)
                }
            }
        } else if schedule.calendarStyle == "custom" {
            Text(L10n.tr("minute  hour  day  month  weekday")).font(.caption.monospaced()).foregroundStyle(Palette.tertiary)
        }
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: 12) {
            if previewError != nil {
                Button(L10n.tr("Check schedule again")) { Task { await checkSchedule() } }
                    .buttonStyle(.plain).disabled(model.isOffline)
            }
            if !loaded, error != nil {
                Button(L10n.tr("Retry loading schedule")) { Task { await loadSchedule() } }.buttonStyle(.plain)
            }
            if schedule.kind != "cron", let previewError {
                Text(previewError).font(.caption).foregroundStyle(Palette.danger)
            }
            if let error {
                Label(error, systemImage: "exclamationmark.circle")
                    .font(.callout).foregroundStyle(Palette.danger).textSelection(.enabled)
            }
            if model.isOffline {
                Label(L10n.tr("Connect to this computer to save your routine."), systemImage: "wifi.slash")
                    .font(.caption).foregroundStyle(Palette.danger)
            }
            if loaded, !model.isOffline,
               name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
               instruction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(L10n.tr("Enter a name and instruction above to save."))
                    .font(.caption).foregroundStyle(Palette.secondary)
            }
            HStack(spacing: 16) { routineActions; saveButton }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 16)
        .background(Palette.background)
    }

    /// An existing routine's own actions: delete and run it now.
    @ViewBuilder private var routineActions: some View {
        if let routine {
            IconButton("Delete routine", systemImage: "trash") { confirmDelete = true }
            IconButton(started ? "Started" : "Test run", systemImage: started ? "checkmark" : "play.circle") {
                Task {
                    do {
                        try await model.client?.routineAction("runRoutine", botId: botId, id: routine.id)
                        withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { started = true }
                    } catch { self.error = error.localizedDescription }
                }
            }
            .disabled(started || model.isOffline)
        }
    }

    private var saveButton: some View {
        Button(action: save) {
            HStack(spacing: 8) {
                if busy { Spinner() }
                Text(busy ? "Saving…" : routine == nil ? "Create routine" : "Save changes")
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, minHeight: 26)
        }
        .buttonStyle(.primary)
        .disabled(!canSave)
        .keyboardShortcut("s", modifiers: .command)
    }

    private func save() {
        guard let client = model.client, canSave else { return }
        // Saving ends editing (a new webhook routine stays open on its URL and key, not under the keyboard).
        typing = false
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                let result = try await client.saveRoutine(botId: botId, id: routine?.id, name: name,
                    instruction: instruction, schedule: schedule, timeoutSeconds: timeout)
                saved(result)
            } catch { self.error = error.localizedDescription }
        }
    }
}

extension RoutineScheduleDraft {
    /// The editor's line. The host's cron sentence stays English, so this uses the same words as the fields.
    var spoken: String {
        let time = String(format: "%02d:%02d", hour, minute)
        let minuteText = String(format: "%02d", minute)
        switch calendarStyle {
        case "hourly": return "\(L10n.tr("Hourly")) · :\(minuteText) · \(zone)"
        case "daily": return "\(L10n.tr("Daily")) · \(time) · \(zone)"
        case "weekdays": return "\(L10n.tr("Weekdays")) · \(time) · \(zone)"
        case "weekly":
            let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
            let day = names.indices.contains(weekday) ? L10n.tr(names[weekday]) : L10n.tr("Mon")
            return "\(L10n.tr("Weekly")) · \(day) · \(time) · \(zone)"
        default:
            let written = expression.trimmingCharacters(in: .whitespacesAndNewlines)
            return written.isEmpty ? zone : "\(written) · \(zone)"
        }
    }
}

func scheduleNextLine(_ at: Int64) -> String {
    let when = Date(milliseconds: at).formatted(date: .abbreviated, time: .shortened)
    return L10n.tr("Next {when}").replacingOccurrences(of: "{when}", with: when)
}

private extension View {
    func routineInput() -> some View {
        textFieldStyle(.plain)
            .padding(12)
            .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
