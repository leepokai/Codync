import CodyncKit
import SwiftUI

/// The step sheet's fields, split out so one view function does not hold every control.
struct WorkflowStepFields: View {
    var draft: WorkflowNodeEdit
    var webhook: WorkflowWebhook?
    var workflowId: String
    var bots: [(id: String, label: String)]
    var schedule: Binding<RoutineScheduleDraft>
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var revealed: [WorkflowWebhook]
    var showKey: Binding<Bool>
    var copied: Binding<String?>
    var rotating: Bool
    var offline: Bool
    var onPickZone: () -> Void
    var onReveal: () -> Void
    var onRotate: () -> Void
    var onTrigger: (String) -> Void
    var prepare: () async -> String? = { nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            kind
            if !workflowId.isEmpty {
                WorkflowSoloRun(workflowId: workflowId, nodeId: draft.id, kind: draft.kind, prepare: prepare)
            }
        }
    }

    @ViewBuilder private var kind: some View {
        switch draft.kind {
        case .trigger:
            WorkflowTriggerFields(draft: draft, webhook: webhook, revealed: revealed, schedule: schedule, checked: checked,
                                  preview: preview, previewError: previewError, showKey: showKey, copied: copied,
                                  rotating: rotating, offline: offline, onPickZone: onPickZone, onReveal: onReveal,
                                  onRotate: onRotate, onTrigger: onTrigger)
        case .prompt:
            WorkflowTextCard("Prompt", draft: draft, key: "text")
        case .agent:
            WorkflowAgentFields(draft: draft, bots: bots)
        case .condition:
            WorkflowConditionFields(draft: draft)
        case .approval:
            WorkflowTextCard("Question", draft: draft, key: "question")
        case .output:
            WorkflowOutputFields(draft: draft, bots: bots)
        default:
            EmptyView()
        }
    }
}

private struct WorkflowTriggerFields: View {
    var draft: WorkflowNodeEdit
    var webhook: WorkflowWebhook?
    var revealed: [WorkflowWebhook]
    var schedule: Binding<RoutineScheduleDraft>
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var showKey: Binding<Bool>
    var copied: Binding<String?>
    var rotating: Bool
    var offline: Bool
    var onPickZone: () -> Void
    var onReveal: () -> Void
    var onRotate: () -> Void
    var onTrigger: (String) -> Void

    var body: some View {
        WorkflowPickerField("When to run", selection: triggerKind, options: [
            (id: "manual", label: "Manual"), (id: "schedule", label: "Schedule"), (id: "webhook", label: "Webhook"),
        ])
        if draft.string("trigger") == "schedule" {
            WorkflowScheduleFields(schedule: schedule, checked: checked, preview: preview, previewError: previewError, onPickZone: onPickZone)
        }
        if draft.string("trigger") == "webhook" {
            WorkflowWebhookFields(hook: revealed.first { $0.nodeId == draft.id } ?? webhook, showKey: showKey, copied: copied,
                                  rotating: rotating, offline: offline, onReveal: onReveal, onRotate: onRotate)
        }
    }

    private var triggerKind: Binding<String> {
        Binding(get: { draft.string("trigger").isEmpty ? "manual" : draft.string("trigger") }, set: onTrigger)
    }
}

private struct WorkflowAgentFields: View {
    var draft: WorkflowNodeEdit
    var bots: [(id: String, label: String)]

    var body: some View {
        WorkflowPickerField("Agent", selection: Binding(get: { draft.string("botId") }, set: { draft.setString("botId", $0, keepEmpty: true) }),
                            options: [(id: "", label: "Choose a bot")] + bots)
        WorkflowTextCard("Task", draft: draft, key: "task")
    }
}

private struct WorkflowConditionFields: View {
    var draft: WorkflowNodeEdit

    var body: some View {
        WorkflowPickerField("Condition", selection: opKind, options: [
            (id: "contains", label: "Contains"), (id: "equals", label: "Equals"),
            (id: "regex", label: "Matches"), (id: "empty", label: "Empty"),
        ])
        if draft.string("op") != "empty" {
            WorkflowTextCard(valueLabel, draft: draft, key: "value", axis: .horizontal)
        }
    }

    private var opKind: Binding<String> {
        Binding(get: { draft.string("op").isEmpty ? "contains" : draft.string("op") }, set: { set("op", $0) })
    }

    private var valueLabel: String {
        switch draft.string("op") {
        case "equals": "Equals"
        case "regex": "Matches"
        default: "Contains"
        }
    }

    private func set(_ key: String, _ value: String) {
        draft.setString(key, value, keepEmpty: true)
    }
}

private struct WorkflowOutputFields: View {
    var draft: WorkflowNodeEdit
    var bots: [(id: String, label: String)]

    var body: some View {
        WorkflowPickerField("Notification", selection: targetKind, options: [
            (id: "notify", label: "Notification"), (id: "chat", label: "Chat"),
        ])
        if draft.string("target") == "chat" {
            WorkflowPickerField("Chat", selection: Binding(get: { draft.string("botId") }, set: { draft.setString("botId", $0, keepEmpty: true) }),
                                options: [(id: "", label: "Choose a bot")] + bots)
        }
    }

    private var targetKind: Binding<String> {
        Binding(get: { draft.string("target").isEmpty ? "notify" : draft.string("target") }, set: { value in
            draft.setString("target", value, keepEmpty: true)
            if value != "chat" { draft.setString("botId", "") }
        })
    }
}

struct WorkflowTextField: View {
    var draft: WorkflowNodeEdit
    var key: String
    var axis: Axis = .horizontal

    var body: some View {
        TextField("", text: Binding(get: { draft.string(key) }, set: { draft.setString(key, $0, keepEmpty: true) }), axis: axis)
            .workflowInput()
    }
}

/// Caption above a full-width choice row.
struct WorkflowPickerField<ID: Hashable>: View {
    var label: String
    @Binding var selection: ID
    var options: [(id: ID, label: String)]

    init(_ label: String, selection: Binding<ID>, options: [(id: ID, label: String)]) {
        self.label = label
        _selection = selection
        self.options = options
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr(label)).font(.caption).foregroundStyle(Palette.secondary)
            ChoicePicker(selection: $selection, options: options, wide: true)
        }
    }
}

/// A surface card with the caption inside and the text on `bubbleUser`, as on the step sheet.
struct WorkflowTextCard: View {
    var label: String
    var draft: WorkflowNodeEdit
    var key: String
    var axis: Axis = .vertical

    init(_ label: String, draft: WorkflowNodeEdit, key: String, axis: Axis = .vertical) {
        self.label = label
        self.draft = draft
        self.key = key
        self.axis = axis
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L10n.tr(label)).font(.caption).foregroundStyle(Palette.secondary)
            WorkflowTextField(draft: draft, key: key, axis: axis)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}

private struct WorkflowScheduleFields: View {
    @Binding var schedule: RoutineScheduleDraft
    var checked: RoutineScheduleDraft?
    var preview: RoutineSchedulePreview?
    var previewError: String?
    var onPickZone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            WorkflowPickerField("Repeat", selection: Binding(get: { schedule.calendarStyle }, set: { style in
                let compiled = checked == schedule ? preview?.cronExpression : nil
                schedule = schedule.repeating(style, compiled: compiled)
            }), options: RoutineScheduleDraft.repeatChoices)
            repeatFields
            zoneField
            previewText
        }
    }

    @ViewBuilder private var repeatFields: some View {
        switch schedule.calendarStyle {
        case "hourly":
            WorkflowPickerField("At minute", selection: $schedule.minute, options: (0..<60).map { (id: $0, label: String(format: ":%02d", $0)) })
        case "daily", "weekdays":
            timeField
        case "weekly":
            WorkflowPickerField("Weekday", selection: $schedule.weekday,
                                options: [1, 2, 3, 4, 5, 6, 0].map { (id: $0, label: Calendar.current.standaloneWeekdaySymbols[$0]) })
            timeField
        case "custom":
            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.tr("Cron")).font(.caption).foregroundStyle(Palette.secondary)
                TextField("Cron", text: $schedule.expression, prompt: Text(L10n.tr("0 9 * * 1-5")).foregroundStyle(Palette.secondary))
                    .font(.body.monospaced())
                    .autocorrectionDisabled()
                    .workflowInput()
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        default:
            EmptyView()
        }
    }

    private var timeField: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("At")).font(.caption).foregroundStyle(Palette.secondary)
            DatePicker("At", selection: $schedule.timeOfDay, displayedComponents: .hourAndMinute)
                .labelsHidden()
                .environment(\.timeZone, .gmt)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
    }

    private var zoneField: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Time zone")).font(.caption).foregroundStyle(Palette.secondary)
            Button(action: onPickZone) {
                HStack(spacing: 8) {
                    Text(zoneName).lineLimit(1)
                    Spacer(minLength: 12)
                    Text(TimeZones.offset(zoneName)).foregroundStyle(Palette.secondary)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
                .font(.body)
                .foregroundStyle(Palette.text)
                .padding(.horizontal, 16)
                .padding(.vertical, 16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder private var previewText: some View {
        if let previewError {
            Text(L10n.tr(previewError)).font(.caption).foregroundStyle(Palette.danger)
        } else if let preview, checked == schedule {
            Text(preview.draft.spoken).font(.caption.weight(.medium))
            if let next = preview.nextRunAt {
                Text(scheduleNextLine(next))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private var zoneName: String {
        schedule.zone.isEmpty ? TimeZone.current.identifier : schedule.zone
    }
}

private struct WorkflowWebhookFields: View {
    var hook: WorkflowWebhook?
    @Binding var showKey: Bool
    @Binding var copied: String?
    var rotating: Bool
    var offline: Bool
    var onReveal: () -> Void
    var onRotate: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let hook {
                Field("URL") { valueRow(hook.url ?? hook.localUrl ?? "", copy: "url") }
                Field("Key") { keyRow(hook) }
            } else {
                Text(L10n.tr("Loading")).font(.caption).foregroundStyle(Palette.secondary)
            }
        }
    }

    private func keyRow(_ hook: WorkflowWebhook) -> some View {
        valueRow(showKey ? (hook.secret ?? "") : String(repeating: "•", count: 16), copy: "key") {
            IconButton(showKey ? "Hide key" : "Show key", systemImage: showKey ? "eye.slash" : "eye") {
                if hook.secret == nil {
                    onReveal()
                } else {
                    withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { showKey.toggle() }
                }
            }
            IconButton("Replace key", systemImage: "arrow.triangle.2.circlepath", action: onRotate)
                .disabled(rotating || offline)
        }
    }

    private func valueRow(_ text: String, copy: String, @ViewBuilder extra: () -> some View = { EmptyView() }) -> some View {
        HStack(spacing: 8) {
            Text(text.isEmpty ? " " : text)
                .font(.callout.monospaced())
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            extra()
            IconButton(copied == copy ? "Copied" : "Copy", systemImage: copied == copy ? "checkmark" : "doc.on.doc") {
                let value = copy == "key" ? hook?.secret : (hook?.url ?? hook?.localUrl)
                Pasteboard.copy(value)
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = copy }
            }
        }
        .padding(.vertical, 8)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}

extension View {
    func workflowInput() -> some View {
        textFieldStyle(.plain)
            .padding(12)
            .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 12))
    }
}
