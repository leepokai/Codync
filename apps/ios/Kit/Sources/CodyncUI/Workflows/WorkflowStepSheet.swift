import CodyncKit
import SwiftUI

/// The step being edited. Field views call `setString` and never hold a `Binding<WorkflowNode>`,
/// whose setter is the closure Xcode 26.6 fails to lower.
@MainActor
@Observable
final class WorkflowNodeEdit {
    var node: WorkflowNode

    init(_ node: WorkflowNode) { self.node = node }

    var id: String { node.id }
    var kind: WorkflowNodeKind { node.kind }

    var stamp: String {
        guard let data = try? JSONEncoder().encode(node) else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    func string(_ key: String) -> String { node.string(key) }

    func setString(_ key: String, _ value: String, keepEmpty: Bool = false) {
        node.setString(key, value, keepEmpty: keepEmpty)
    }
}

/// One step's fields. Schedule and webhook use the same controls as a routine.
struct WorkflowStepSheet: View {
    let node: WorkflowNode
    var webhook: WorkflowWebhook?
    var onChange: (WorkflowNodeEdit) -> Void
    var onReveal: () async -> [WorkflowWebhook]
    var onRotate: () async -> String?
    var workflowId: String
    var sourceSelection: String = ""
    var sourceOptions: [(id: String, label: String)] = []
    var onSource: (String) -> Void = { _ in }
    var prepare: () async -> String? = { nil }

    @Environment(BotStore.self) private var model
    @State private var draft: WorkflowNodeEdit
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

    init(node: WorkflowNode, webhook: WorkflowWebhook?, onChange: @escaping (WorkflowNodeEdit) -> Void,
         onReveal: @escaping () async -> [WorkflowWebhook], onRotate: @escaping () async -> String?, workflowId: String,
         sourceSelection: String = "", sourceOptions: [(id: String, label: String)] = [], onSource: @escaping (String) -> Void = { _ in },
         prepare: @escaping () async -> String? = { nil }) {
        self.node = node
        self.webhook = webhook
        self.onChange = onChange
        self.onReveal = onReveal
        self.onRotate = onRotate
        self.workflowId = workflowId
        self.sourceSelection = sourceSelection
        self.sourceOptions = sourceOptions
        self.onSource = onSource
        self.prepare = prepare
        _draft = State(initialValue: WorkflowNodeEdit(node))
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader { Text(WorkflowCopy.heading(draft.node, botName: botName)) }
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if !sourceOptions.isEmpty {
                        WorkflowPickerField("Source", selection: Binding(get: { sourceSelection }, set: onSource), options: sourceOptions)
                    }
                    WorkflowStepFields(draft: draft, webhook: webhook, workflowId: workflowId, bots: botChoices,
                                   schedule: $schedule, checked: checked, preview: preview, previewError: previewError,
                                   revealed: revealed, showKey: $showKey, copied: $copied, rotating: rotating,
                                   offline: model.isOffline, onPickZone: { pickingZone = true },
                                   onReveal: { Task { await reveal() } }, onRotate: { confirmRotate = true },
                                   onTrigger: setTrigger, prepare: prepare)
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
        .onChange(of: draft.stamp) { onChange(draft) }
    }

    private var botChoices: [(id: String, label: String)] {
        model.bots.values.filter { !$0.isGroup }.sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }.map { (id: $0.id, label: $0.name) }
    }

    private var botName: (String) -> String? {{ model.bots[$0]?.name }}

    private func setTrigger(_ kind: String) {
        var next = draft.node
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
        draft.node = next
    }

    private func hydrateSchedule() async {
        guard draft.string("trigger") == "schedule", let client = model.client else { return }
        let zone = draft.node.schedule?.timeZone ?? draft.string("timezone")
        let timeZone = zone.isEmpty ? TimeZone.current.identifier : zone
        let triggers = draft.node.schedule.map { [$0] } ?? []
        let result: RoutineSchedulePreview
        do {
            result = try await client.routineSchedule(triggers: triggers, timeZone: timeZone)
        } catch is CancellationError {
            return
        } catch {
            previewError = error.localizedDescription
            return
        }
        previewError = nil
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
        var next = draft.node
        var cron = trigger
        if cron.timeZone == nil || cron.timeZone?.isEmpty == true { cron.timeZone = schedule.zone }
        next.schedule = cron
        if let zone = cron.timeZone, !zone.isEmpty { next.setString("timezone", zone, keepEmpty: true) }
        guard next != draft.node else { return }
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
            if let index = revealed.firstIndex(where: { $0.nodeId == draft.node.id }) {
                revealed[index].secret = secret
            } else if var hook = webhook {
                hook.secret = secret
                revealed = [hook]
            }
            showKey = true
        }
    }
}
