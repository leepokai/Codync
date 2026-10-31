import CodyncKit
import SwiftUI

/// One step's fields. Schedule and webhook use the same controls as a routine.
struct WorkflowStepSheet: View {
    let node: WorkflowNode
    var webhook: WorkflowWebhook?
    var onChange: (WorkflowNode) -> Void
    var onReveal: () async -> [WorkflowWebhook]
    var onRotate: () async -> String?

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var draft: WorkflowNode
    @State private var schedule = RoutineScheduleDraft()
    @State private var scheduleReady = false
    @State private var checked: RoutineScheduleDraft?
    @State private var preview: RoutineSchedulePreview?
    @State private var previewError: String?
    @State private var pickingZone = false
    @State private var revealed: [WorkflowWebhook] = []
    @State private var showKey = false
    @State private var copied: String?
    @State private var confirmRotate = false
    @State private var rotating = false

    init(node: WorkflowNode, webhook: WorkflowWebhook?, onChange: @escaping (WorkflowNode) -> Void,
         onReveal: @escaping () async -> [WorkflowWebhook], onRotate: @escaping () async -> String?) {
        self.node = node
        self.webhook = webhook
        self.onChange = onChange
        self.onReveal = onReveal
        self.onRotate = onRotate
        _draft = State(initialValue: node)
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(WorkflowCopy.title(draft, botName: botName))
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    fields
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .background(Palette.background)
        .codyncSheet(isPresented: $pickingZone) {
            TimeZonePicker(selection: schedule.zone.isEmpty ? TimeZone.current.identifier : schedule.zone) {
                schedule.zone = $0
                pickingZone = false
            }
        }
        .codyncDialog("Replace the key?", isPresented: $confirmRotate,
                      message: "Senders using the current key stop working until they get the new one.") {
            [DialogAction("Replace key", destructive: true) { Task { await rotate() } }]
        }
        .task { await hydrateSchedule() }
        .task(id: schedule) { await checkSchedule() }
    }

    @ViewBuilder private var fields: some View {
        switch draft.kind {
        case .trigger:
            Field("When to run") {
                ChoicePicker(selection: triggerKind, options: [
                    (id: "manual", label: "Manual"), (id: "schedule", label: "Schedule"), (id: "webhook", label: "Webhook"),
                ], fitsAvailableWidth: true)
            }
            if draft.string("trigger") == "schedule" { scheduleFields }
            if draft.string("trigger") == "webhook" { webhookFields }
        case .prompt:
            Field("Prompt") { textField("text", axis: .vertical) }
        case .agent:
            Field("Agent") { botPicker }
            Field("Task") { textField("task", axis: .vertical) }
        case .condition:
            Field("Condition") {
                ChoicePicker(selection: opKind, options: [
                    (id: "contains", label: "Contains"), (id: "equals", label: "Equals"),
                    (id: "regex", label: "Matches"), (id: "empty", label: "Empty"),
                ])
            }
            if draft.string("op") != "empty" {
                let label = draft.string("op") == "equals" ? "Equals" : draft.string("op") == "regex" ? "Matches" : "Contains"
                Field(label) { textField("value") }
            }
        case .approval:
            Field("Question") { textField("question", axis: .vertical) }
        case .output:
            Field("Notification") {
                ChoicePicker(selection: targetKind, options: [
                    (id: "notify", label: "Notification"), (id: "chat", label: "Chat"),
                ])
            }
            if draft.string("target") == "chat" {
                Field("Chat") { botPicker }
            }
        default:
            EmptyView()
        }
    }

    private var triggerKind: Binding<String> {
        Binding(get: { draft.string("trigger").isEmpty ? "manual" : draft.string("trigger") }, set: setTrigger)
    }

    private var opKind: Binding<String> {
        Binding(get: { draft.string("op").isEmpty ? "contains" : draft.string("op") }, set: { set("op", $0) })
    }

    private var targetKind: Binding<String> {
        Binding(get: { draft.string("target").isEmpty ? "notify" : draft.string("target") }, set: { value in
            var next = draft
            next.setString("target", value, keepEmpty: true)
            if value != "chat" { next.setString("botId", "") }
            commit(next)
        })
    }

    private var botPicker: some View {
        let bots = model.bots.values.filter { !$0.isGroup }.sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
        let options = [(id: "", label: "Choose a bot")] + bots.map { (id: $0.id, label: $0.name) }
        return ChoicePicker(selection: Binding(get: { draft.string("botId") }, set: { set("botId", $0) }), options: options, fitsAvailableWidth: true)
    }

    private var scheduleFields: some View {
        VStack(alignment: .leading, spacing: 14) {
            Field("Repeat") {
                ChoicePicker(selection: Binding(get: { schedule.calendarStyle }, set: { style in
                    let compiled = checked == schedule ? preview?.cronExpression : nil
                    schedule = schedule.repeating(style, compiled: compiled)
                }), options: RoutineScheduleDraft.repeatChoices)
            }
            switch schedule.calendarStyle {
            case "hourly":
                Field("At minute") {
                    ChoicePicker(selection: $schedule.minute, options: (0..<60).map { (id: $0, label: String(format: ":%02d", $0)) })
                }
            case "daily", "weekdays":
                timeField
            case "weekly":
                Field("On") {
                    ChoicePicker(selection: $schedule.weekday,
                                 options: [1, 2, 3, 4, 5, 6, 0].map { (id: $0, label: Calendar.current.standaloneWeekdaySymbols[$0]) })
                }
                timeField
            case "custom":
                Field("Cron") {
                    TextField("Cron", text: $schedule.expression, prompt: Text(L10n.tr("0 9 * * 1-5")).foregroundStyle(Palette.secondary))
                        .font(.body.monospaced())
                        .autocorrectionDisabled()
                        .workflowInput()
                }
            default:
                EmptyView()
            }
            Field("Time zone") {
                Button {
                    pickingZone = true
                } label: {
                    HStack(spacing: 6) {
                        Text(schedule.zone.isEmpty ? TimeZone.current.identifier : schedule.zone).lineLimit(1)
                        Text(TimeZones.offset(schedule.zone.isEmpty ? TimeZone.current.identifier : schedule.zone))
                            .foregroundStyle(Palette.secondary)
                        Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                    }
                    .foregroundStyle(Palette.text)
                    .outlinedPill()
                }
                .buttonStyle(.plain)
            }
            if let previewError {
                Text(L10n.tr(previewError)).font(.caption).foregroundStyle(Palette.danger)
            } else if let preview, checked == schedule {
                Text(preview.summary).font(.caption.weight(.medium))
                if let next = preview.nextRunAt {
                    Text(L10n.tr("Next \(Date(milliseconds: next).formatted(date: .abbreviated, time: .shortened))"))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                }
            }
        }
    }

    private var timeField: some View {
        Field("At") {
            DatePicker("At", selection: $schedule.timeOfDay, displayedComponents: .hourAndMinute)
                .labelsHidden()
                .environment(\.timeZone, .gmt)
        }
    }

    private var webhookFields: some View {
        let hook = revealed.first { $0.nodeId == draft.id } ?? webhook
        return VStack(alignment: .leading, spacing: 14) {
            if let hook {
                Field("URL") { hookValue(hook.url, copy: "url") }
                Field("Key") {
                    hookValue(showKey ? (hook.secret ?? "") : String(repeating: "•", count: 16), copy: "key") {
                        IconButton(showKey ? "Hide key" : "Show key", systemImage: showKey ? "eye.slash" : "eye") {
                            if hook.secret == nil {
                                Task { await reveal() }
                            } else {
                                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { showKey.toggle() }
                            }
                        }
                        IconButton("Replace key", systemImage: "arrow.triangle.2.circlepath") { confirmRotate = true }
                            .disabled(rotating || model.isOffline)
                    }
                }
            } else {
                Text(L10n.tr("Loading")).font(.caption).foregroundStyle(Palette.secondary)
            }
        }
    }

    private func hookValue(_ text: String, copy: String, @ViewBuilder extra: () -> some View = { EmptyView() }) -> some View {
        HStack(spacing: 8) {
            Text(text.isEmpty ? " " : text)
                .font(.callout.monospaced())
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            extra()
            IconButton(copied == copy ? "Copied" : "Copy", systemImage: copied == copy ? "checkmark" : "doc.on.doc") {
                let hook = revealed.first { $0.nodeId == draft.id } ?? webhook
                let value = copy == "key" ? hook?.secret : hook?.url
                Pasteboard.copy(value)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = copy }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }

    private func textField(_ key: String, axis: Axis = .horizontal) -> some View {
        TextField("", text: Binding(get: { draft.string(key) }, set: { set(key, $0) }), axis: axis)
            .workflowInput()
    }

    private var botName: (String) -> String? {{ model.bots[$0]?.name }}

    private func set(_ key: String, _ value: String) {
        var next = draft
        next.setString(key, value, keepEmpty: true)
        commit(next)
    }

    private func setTrigger(_ kind: String) {
        var next = draft
        next.setString("trigger", kind, keepEmpty: true)
        if kind == "schedule" {
            if next.schedule == nil {
                scheduleReady = false
                Task { await hydrateSchedule() }
            }
        } else {
            next.schedule = nil
            next.fields.removeValue(forKey: "timezone")
        }
        commit(next)
    }

    private func commit(_ next: WorkflowNode) {
        draft = next
        onChange(next)
    }

    private func hydrateSchedule() async {
        guard draft.string("trigger") == "schedule", let client = model.client else { return }
        let zone = draft.schedule?.timeZone ?? draft.string("timezone")
        let timeZone = zone.isEmpty ? TimeZone.current.identifier : zone
        let triggers = draft.schedule.map { [$0] } ?? []
        guard let result = try? await client.routineSchedule(triggers: triggers, timeZone: timeZone) else { return }
        schedule = result.draft.forMobileEditor()
        if schedule.zone.isEmpty { schedule.zone = timeZone }
        preview = result
        checked = schedule
        scheduleReady = true
        applySchedule(result)
    }

    private func checkSchedule() async {
        guard scheduleReady, draft.string("trigger") == "schedule" else { return }
        let requested = schedule
        do {
            try await Task.sleep(for: .milliseconds(300))
        } catch { return }
        guard schedule == requested, let client = model.client else { return }
        do {
            let result = try await client.routineSchedule(draft: requested)
            guard schedule == requested else { return }
            preview = result
            previewError = nil
            checked = requested
            applySchedule(result)
        } catch {
            guard !Task.isCancelled, schedule == requested else { return }
            previewError = error.localizedDescription
        }
    }

    /// A schedule is written only after the host has described it, and its time zone matches the field.
    private func applySchedule(_ result: RoutineSchedulePreview) {
        guard draft.string("trigger") == "schedule", let trigger = result.triggers.first, trigger.type == "cron" else { return }
        var next = draft
        var cron = trigger
        if cron.timeZone == nil || cron.timeZone?.isEmpty == true { cron.timeZone = schedule.zone }
        next.schedule = cron
        if let zone = cron.timeZone, !zone.isEmpty { next.setString("timezone", zone, keepEmpty: true) }
        guard next != draft else { return }
        commit(next)
    }

    private func reveal() async {
        revealed = await onReveal()
        showKey = true
    }

    private func rotate() async {
        rotating = true
        defer { rotating = false }
        if let secret = await onRotate() {
            if let index = revealed.firstIndex(where: { $0.nodeId == draft.id }) {
                revealed[index].secret = secret
            } else if var hook = webhook {
                hook.secret = secret
                revealed = [hook]
            }
            showKey = true
        }
    }
}

private extension View {
    func workflowInput() -> some View {
        textFieldStyle(.plain)
            .padding(12)
            .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
