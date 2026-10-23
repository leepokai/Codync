import CodyncKit
import SwiftUI

/// Models come from the selected computer's agent, never a bundled provider list. The model's
/// settings follow it: the Effort level row first (the agent's `thought_level` setting), then the
/// agent's others under their own names. Fast mode is a second card row, shown while the agent
/// offers it or while it's on (so it can be turned off).
struct AgentModelPicker: View {
    let backend: String
    @Binding var selection: String?
    /// The selected model's settings (effort, fast…); they belong to one model.
    @Binding var options: [String: String]
    /// Nil when picking a fallback's model: the bot's own Fast mode applies to it, and no session restarts.
    var fastMode: Binding<Bool>?
    /// The saved bot: whether its own sessions offer Fast mode, and why the agent refused it.
    var bot: Bot?
    /// For the Effort level's line: what this step's own agent reports while it runs, and what the host saved.
    /// Nil: no line (a fallback being added, an unsaved agent or model).
    var status: (report: SessionReport?, saved: [String: String])?
    /// The bot is replying: a change waits for the reply.
    var working = false
    @Environment(BotStore.self) private var store
    @State private var catalog: AgentModels?
    @State private var catalogScope: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var loading = false
    @State private var failure: String?
    @State private var retry = 0

    private var requestKey: String {
        "\(store.computer.id)/\(backend)/\(store.connection == .online)/\(retry)"
    }

    /// The agent's own "Default (recommended)" entry is the same choice as leaving the model unset,
    /// so it folds into the single Default row instead of showing twice.
    private var agentDefault: AgentModels.Model? {
        catalog?.models.first { $0.name.lowercased().hasPrefix("default") }
    }

    private var modelChoices: [(String, String)] {
        let available = (catalog?.models ?? []).filter { $0.id != agentDefault?.id }
        let current = available.first { $0.id == catalog?.currentModelId }?.name
        var result = [("", agentDefault?.name ?? current.map { "Default · \($0)" } ?? "Default")]
        result += available.map { ($0.id, $0.name) }
        if let selection, !selection.isEmpty, selection != agentDefault?.id,
           !available.contains(where: { $0.id == selection }) {
            result.append((selection, selection))
        }
        return result
    }

    private var value: Binding<String> {
        Binding(
            get: { selection == agentDefault?.id ? "" : selection ?? "" },
            set: {
                let model = $0.isEmpty ? nil : $0
                guard model != selection else { return }
                selection = model
                options = [:]
            }
        )
    }

    /// Settings the selected model offers (on Default, the agent's default model's).
    private var modelOptions: [AgentModels.Option] {
        catalog?.settings(for: value.wrappedValue) ?? []
    }

    private func effortLevel(saved: String?) -> EffortLevel {
        EffortLevel.row(catalog: modelOptions, saved: saved, report: status?.report, loading: loading,
                        failed: failure != nil, chosenModel: !value.wrappedValue.isEmpty)
    }

    /// The Effort level row, captioned from the host's saved value; nothing to report for a value not saved yet.
    private var effort: (level: EffortLevel, caption: String?) {
        guard case .row(let setting, _) = effortLevel(saved: nil), let id = setting?.id else {
            return (effortLevel(saved: nil), nil)
        }
        let level = effortLevel(saved: status?.saved[id])
        guard case .row(_, let caption) = level, let status, status.saved[id] == options[id] else { return (level, nil) }
        return (level, caption)
    }

    private func optionValue(_ id: String) -> Binding<String> {
        Binding(get: { options[id] ?? "" }, set: { options[id] = $0.isEmpty ? nil : $0 })
    }

    private func choices(_ option: AgentModels.Option) -> [(String, String)] {
        let fallback = option.values.first { $0.id == option.default }?.name
        var result = [("", fallback.map { "Default · \($0)" } ?? "Default")] + option.values.map { ($0.id, $0.name) }
        // A saved value the agent no longer lists stays visible until it's changed.
        if let value = options[option.id], !option.values.contains(where: { $0.id == value }) { result.append((value, value)) }
        return result
    }

    private var showsFastMode: Bool {
        guard let fastMode else { return false }
        return (backend != "custom" && catalog?.fastMode == true) || fastMode.wrappedValue
            || (bot?.backend == backend && bot?.fastModeShown == true)
    }

    var body: some View {
        content
            .onChange(of: backend) { _, _ in
                selection = nil
                options = [:]
            }
            .task(id: requestKey) { await load() }
        if showsFastMode, let fastMode {
            SwitchRow("Fast mode", detail: bot?.fastModeRefusal ?? "Faster replies for more usage, on models that offer it.",
                      isOn: fastMode)
                .transition(.opacity)
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 6) {
            modelRow
            effortRow
            otherRows
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: modelOptions.map(\.id))
    }

    @ViewBuilder private var effortRow: some View {
        let (level, caption) = effort
        switch level {
        case .later:
            Text("Effort level appears once the agent runs this model.")
                .font(.caption)
                .foregroundStyle(Palette.secondary)
        case .row(let setting, _):
            VStack(alignment: .leading, spacing: 2) {
                if let setting { settingRow("Effort level", setting) }
                if let caption {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                }
            }
            .transition(.opacity)
        case .none:
            EmptyView()
        }
    }

    private func settingRow(_ label: String, _ option: AgentModels.Option) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .foregroundStyle(Palette.text)
                .fixedSize()
            Spacer(minLength: 0)
            ChoicePicker(selection: optionValue(option.id), options: choices(option), fitsAvailableWidth: true)
        }
        .transition(.opacity)
    }

    @ViewBuilder private var otherRows: some View {
        // The rest of what the running session lists, and any saved setting nobody lists (so it can be cleared).
        let effortID: String? = if case .row(let setting?, _) = effortLevel(saved: nil) { setting.id } else { nil }
        ForEach(EffortLevel.shown(catalog: modelOptions, report: status?.report, saved: options).filter { $0.id != effortID }) { option in
            settingRow(option.name, option)
        }
        if backend != "custom", !loading {
            if let failure {
                Text(L10n.tr(failure))
                    .font(.caption)
                    .foregroundStyle(Palette.warning)
            } else if catalog?.models.isEmpty == true {
                Text(L10n.tr("This agent doesn't advertise a model list. Leave Default or enter a model ID."))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
            }
        }
        if fastMode != nil {
            Text(L10n.tr(working
                ? "Changes to the model or effort level apply after this reply, in a new agent session. Chat history is kept; the agent’s session context starts fresh."
                : "Changes to the model or effort level apply to the next message, in a new agent session. Chat history is kept; the agent’s session context starts fresh."))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
        }
    }

    private var modelRow: some View {
        HStack(spacing: 8) {
            Text(L10n.tr("Model"))
                .foregroundStyle(Palette.text)
                .fixedSize()
            Spacer(minLength: 0)
            HStack(spacing: 4) {
                if backend != "custom" { refreshControl }
                if backend == "custom" || (!loading && catalog?.models.isEmpty != false) {
                    TextField("Default", text: value)
                        .plainTextInput()
                        .multilineTextAlignment(.trailing)
                        .textFieldStyle(.plain)
                        .frame(minWidth: 0, maxWidth: 180)
                        .outlinedPill()
                } else {
                    ChoicePicker(selection: value, options: modelChoices, fitsAvailableWidth: true)
                }
            }
        }
    }

    /// Loading and refresh share one fixed slot, so the row never grows for loading.
    private var refreshControl: some View {
        ZStack {
            IconButton(failure == nil ? "Refresh model list" : "Retry model list", systemImage: "arrow.clockwise") {
                retry += 1
            }
            .opacity(loading ? 0 : 1)
            .disabled(loading)
            .accessibilityHidden(loading)
            if loading {
                Spinner(size: 12)
                    .accessibilityLabel("Loading available models")
                    .transition(.opacity)
            }
        }
        .frame(width: 36, height: 36)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: loading)
    }

    @MainActor private func load() async {
        let scope = "\(store.computer.id)/\(backend)"
        if catalogScope != scope {
            catalog = nil
            catalogScope = scope
        }
        failure = nil
        loading = false
        guard backend != "custom" else { return }
        guard store.connection == .online, let client = store.client else {
            failure = "Connect to this computer to load its models. You can still enter a model ID."
            return
        }
        loading = true
        do {
            let response = try await client.agentModels(backend)
            try Task.checkCancellation()
            withAnimation(Motion.reduced(Motion.fade, reduceMotion)) { catalog = response }
        } catch {
            guard !Task.isCancelled else { return }
            failure = "Couldn't load models: \(error.localizedDescription)"
        }
        loading = false
    }
}
