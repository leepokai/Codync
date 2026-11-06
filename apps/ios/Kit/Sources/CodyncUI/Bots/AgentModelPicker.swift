import CodyncKit
import SwiftUI

/// Models come from the selected computer's agent, never a bundled provider list.
/// The summary expands into one searchable agent/model picker and its settings.
struct AgentModelPicker: View {
    let backend: String
    @Binding var selection: String?
    /// The selected model's settings (effort, fast…); they belong to one model.
    @Binding var options: [String: String]
    /// Nil when picking a fallback's model: the bot's own Fast mode applies to it, and no session restarts.
    var fastMode: Binding<Bool>?
    /// The saved bot: whether its own sessions offer Fast mode, and why the agent refused it.
    var bot: Bot?
    var agents: [(String, String)] = []
    var onAgentChange: ((String) -> Void)?
    var command: Binding<String?>?
    var installHint: String?
    var restLine: String?
    /// Retry model list, for the rows that read the same list (Permissions).
    var onReload: (() -> Void)?
    private enum Stage { case closed, agents, models, settings }
    @State private var stage = Stage.closed
    @State private var query = ""
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

    /// Settings the selected model offers.
    private var selectedModel: AgentModels.Model? {
        catalog?.models.first { $0.id == (selection ?? agentDefault?.id ?? catalog?.currentModelId) }
    }

    private var modelOptions: [AgentModels.Option] {
        guard !value.wrappedValue.isEmpty || catalog?.defaultModelOptions == true else { return [] }
        return selectedModel?.options ?? []
    }

    private var showsFastMode: Bool {
        guard let fastMode else { return false }
        return (backend != "custom" && catalog?.fastMode == true) || fastMode.wrappedValue
            || (bot?.backend == backend && bot?.fastModeShown == true)
    }

    private var agentName: String { agents.first { $0.0 == backend }?.1 ?? backend }
    private var modelName: String { value.wrappedValue.isEmpty ? L10n.tr("Default") : modelChoices.first { $0.0 == value.wrappedValue }?.1 ?? value.wrappedValue }
    private var summary: String {
        let settings = modelOptions.compactMap { option in
            option.values.first { $0.id == (options[option.id] ?? option.default) }?.name
        }
        return ([agentName, modelName] + settings + (fastMode?.wrappedValue == true ? ["Fast"] : [])).joined(separator: " · ")
    }

    private func open(_ next: Stage) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { query = ""; stage = next }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button { open(stage == .closed ? .models : .closed) } label: {
                HStack(spacing: 6) {
                    Text(summary).font(.subheadline).lineLimit(2)
                        .contentTransition(.opacity)
                        .animation(Motion.reduced(Motion.fade, reduceMotion), value: summary)
                    Spacer(minLength: 0)
                    Image(systemName: stage == .closed ? "chevron.down" : "chevron.up").font(.caption)
                }
                .foregroundStyle(Palette.text)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(stage == .closed ? "Collapsed" : "Expanded")
            if stage != .closed {
                HStack {
                    if onAgentChange != nil {
                        Button { open(stage == .agents ? .settings : .agents) } label: { Text(agentName).lineLimit(1); Image(systemName: "chevron.down") }
                    }
                    Spacer(minLength: 0)
                    Button { open(stage == .models ? .settings : .models) } label: { Text(modelName).lineLimit(1); Image(systemName: "chevron.down") }
                }
                .font(.subheadline)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.text)
                if let restLine { Text(restLine).font(.caption).foregroundStyle(Palette.secondary) }
                if let installHint { Text(installHint).font(.caption).foregroundStyle(Palette.warning) }
                if backend == "custom", let command {
                    TextField("ACP command, e.g. my-agent --acp", text: Binding(get: { command.wrappedValue ?? "" }, set: { command.wrappedValue = $0.isEmpty ? nil : $0 }))
                        .plainTextInput().fieldBox()
                }
                if stage == .agents || stage == .models { searchResults }
                ForEach(modelOptions) { option in
                    if option.isReasoning {
                        ReasoningSlider(option: option, selection: Binding(get: { options[option.id] }, set: { options[option.id] = $0 }))
                    } else {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(option.name).foregroundStyle(Palette.secondary)
                            Spacer(minLength: 0)
                            Button {
                                withAnimation(Motion.reduced(Motion.fade, reduceMotion)) { options[option.id] = nil }
                            } label: {
                                let fallback = option.values.first { $0.id == option.default }?.name
                                Text(L10n.tr("Default") + (fallback.map { " · \($0)" } ?? "") + (options[option.id] == nil ? " ✓" : ""))
                            }
                            .buttonStyle(.plain)
                        }.font(.caption)
                        ScrollView(.horizontal) {
                            SegmentedChoice(selection: Binding(get: { options[option.id] ?? option.default ?? "" }, set: { options[option.id] = $0 }),
                                            options: option.values.map { ($0.id, $0.name) })
                                .frame(minWidth: CGFloat(option.values.count) * 65)
                        }.scrollIndicators(.hidden)
                    }
                }
                }
                if showsFastMode, let fastMode {
                    SwitchRow("Fast mode", detail: bot?.fastModeRefusal, isOn: fastMode)
                }
                if let error = selectedModel?.optionsError { Text(error).font(.caption).foregroundStyle(Palette.warning) }
                if let failure { Text(L10n.tr(failure)).font(.caption).foregroundStyle(Palette.warning) }
                if fastMode != nil {
                    Text(L10n.tr("Changing the model starts a new agent session. Chat history is kept."))
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
            }
        }
        .task(id: requestKey) { await load() }
    }

    private var searchResults: some View {
        VStack(spacing: 4) {
            HStack(spacing: 6) {
                if stage == .models, onAgentChange != nil {
                    IconButton("Back", systemImage: "chevron.left") { open(.agents) }
                }
                TextField(stage == .agents ? "Search agents" : "Search models", text: $query).plainTextInput()
                refreshControl
            }
            let choices = (stage == .agents ? agents : modelChoices).filter {
                query.isEmpty || $0.1.localizedCaseInsensitiveContains(query)
            }
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(choices, id: \.0) { choice in
                        Button {
                            if stage == .agents {
                                if choice.0 != backend { onAgentChange?(choice.0) }
                                open(.models)
                            }
                            else { value.wrappedValue = choice.0; open(.settings) }
                        } label: {
                            HStack {
                                Text(L10n.tr(choice.1)).lineLimit(2)
                                Spacer(minLength: 0)
                                if choice.0 == (stage == .agents ? backend : value.wrappedValue) {
                                    Image(systemName: "checkmark").font(.caption)
                                }
                            }
                            .foregroundStyle(Palette.text)
                            .padding(.vertical, 9).padding(.horizontal, 6)
                            .contentShape(Rectangle())
                        }.buttonStyle(.plain)
                    }
                    if choices.isEmpty { Text(L10n.tr("No results")).font(.caption).foregroundStyle(Palette.secondary) }
                }
            }.frame(maxHeight: 180)
            if stage == .models, backend == "custom" || (!loading && catalog?.models.isEmpty != false) {
                TextField("Default", text: value).plainTextInput().fieldBox()
            }
        }
    }

    /// Loading and refresh share one fixed slot, so the row never grows for loading.
    private var refreshControl: some View {
        ZStack {
            IconButton(failure == nil ? "Refresh model list" : "Retry model list", systemImage: "arrow.clockwise") {
                retry += 1
                onReload?()
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
