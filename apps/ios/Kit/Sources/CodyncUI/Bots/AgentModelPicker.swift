import CodyncKit
import SwiftUI

/// Models come from the selected computer's agent, never a bundled provider list.
/// The summary expands into one searchable agent/model picker and its settings.
struct AgentModelPicker: View {
    let backend: String
    /// The login profile its models are listed for; nil for the default login.
    var loginProfile: String?
    @Binding var selection: String?
    /// The selected model's settings (effort, fast…); they belong to one model.
    @Binding var options: [String: String]
    /// Nil when picking a fallback's model: the bot's own Fast mode applies to it, and no session restarts.
    var fastMode: Binding<Bool>?
    /// The saved bot: whether its own sessions offer Fast mode, and why the agent refused it.
    var bot: Bot?
    /// Each agent once per login; picking one sets both.
    var agents: [LoginSeat] = []
    var onAgentChange: ((AgentLogin) -> Void)?
    var command: Binding<String?>?
    var installHint: String?
    var restLine: String?
    /// Retry model list, for the rows that read the same list (Permissions).
    var onReload: (() -> Void)?
    /// Starts open on its choices (a fallback just added).
    var expanded = false
    private enum Stage { case closed, agents, models, settings }
    /// A row of the agent or model list.
    private struct Choice: Identifiable {
        let id: String
        let name: String
        let on: Bool
        let pick: () -> Void
    }
    @State private var stage = Stage.closed
    @State private var query = ""
    @Environment(BotStore.self) private var store
    @State private var models = AgentModelCatalog()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var adding: Backend?

    private var modelChoices: [(String, String)] { AgentModels.choices(models.catalog, selection: selection).map { ($0.id, $0.name) } }

    private var value: Binding<String> {
        Binding(
            get: { AgentModels.listValue(models.catalog, model: selection) },
            set: {
                let model = $0.isEmpty ? nil : $0
                guard model != selection else { return }
                selection = model
                options = [:]
            }
        )
    }

    /// Settings the selected model offers.
    private var selectedModel: AgentModels.Model? { AgentModels.selected(models.catalog, model: selection) }

    private var modelOptions: [AgentModels.Option] { AgentModels.settings(models.catalog, model: selection) }

    private var showsFastMode: Bool {
        guard let fastMode else { return false }
        return AgentModels.showsFast(models.catalog, backend: backend, on: fastMode.wrappedValue, bot: bot)
    }

    private var current: AgentLogin { AgentLogin(backend: backend, loginProfile: loginProfile) }
    private var agentName: String { agents.first { $0.login == current }?.name ?? backend }
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
                if let failure = models.failure { Text(L10n.tr(failure)).font(.caption).foregroundStyle(Palette.warning) }
                if fastMode != nil {
                    Text(L10n.tr("Changing the model starts a new agent session. Chat history is kept."))
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
            }
        }
        .task(id: models.key(store, backend: backend, loginProfile: loginProfile)) {
            await models.load(store, backend: backend, loginProfile: loginProfile, animation: Motion.reduced(Motion.fade, reduceMotion))
        }
        .codyncSheet(item: $adding) { AgentSheet(initial: $0) }
        .onAppear { if expanded, stage == .closed { stage = .settings } }
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
            let found = (stage == .agents ? agentChoices : modelChoiceRows).filter {
                query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
            }
            let choices = found + (stage == .agents ? addLoginChoice : [])
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(choices) { choice in
                        Button(action: choice.pick) {
                            HStack {
                                Text(choice.name).lineLimit(2)
                                Spacer(minLength: 0)
                                if choice.on {
                                    Image(systemName: "checkmark").font(.caption)
                                }
                            }
                            .foregroundStyle(Palette.text)
                            .padding(.vertical, 9).padding(.horizontal, 6)
                            .contentShape(Rectangle())
                        }.buttonStyle(.plain).scrollEdgeBlur()
                    }
                    if found.isEmpty { Text(L10n.tr("No results")).font(.caption).foregroundStyle(Palette.secondary) }
                }
            }
            .scrollEdges()
            .frame(maxHeight: 180)
            if stage == .models, backend == "custom" || (!models.loading && models.catalog?.models.isEmpty != false) {
                TextField("Default", text: value).plainTextInput().fieldBox()
            }
        }
    }

    private var agentChoices: [Choice] {
        agents.map { seat in
            Choice(id: "\(seat.login.backend)/\(seat.login.loginProfile ?? "")", name: seat.name, on: seat.login == current) {
                if seat.login != current { onAgentChange?(seat.login) }
                open(.models)
            }
        }
    }

    /// The list ends with "Add login profile…" for an agent that has login profiles.
    private var addLoginChoice: [Choice] {
        guard let addsLoginTo = store.hello?.backends.first(where: { $0.id == backend && $0.loginProfiles != nil }) else { return [] }
        return [Choice(id: "+", name: L10n.tr("Add login profile…"), on: false) { adding = addsLoginTo }]
    }

    private var modelChoiceRows: [Choice] {
        modelChoices.map { id, name in
            Choice(id: id, name: L10n.tr(name), on: id == value.wrappedValue) { value.wrappedValue = id; open(.settings) }
        }
    }

    /// Loading and refresh share one fixed slot, so the row never grows for loading.
    private var refreshControl: some View {
        ZStack {
            IconButton(models.failure == nil ? "Refresh model list" : "Retry model list", systemImage: "arrow.clockwise") {
                models.retry += 1
                onReload?()
            }
            .opacity(models.loading ? 0 : 1)
            .disabled(models.loading)
            .accessibilityHidden(models.loading)
            if models.loading {
                Spinner(size: 12)
                    .accessibilityLabel("Loading available models")
                    .transition(.opacity)
            }
        }
        .frame(width: 36, height: 36)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: models.loading)
    }
}

private extension View {
    /// Rows fade out under either edge while there is more past it, into the card behind (the desktop's
    /// `.scroll-edges`); each row adds `scrollEdgeBlur()` to blur there too.
    func scrollEdges() -> some View { modifier(ScrollEdges()) }

    /// Blurs a row while an edge cuts it. A blur of the row itself, not a material over it, so nothing
    /// tints the edge and it washes into the card like the bot list's.
    func scrollEdgeBlur() -> some View {
        scrollTransition(.interactive) { content, phase in content.blur(radius: abs(phase.value) * 6) }
    }
}

private struct ScrollEdges: ViewModifier {
    @State private var more: Edge.Set = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .onScrollGeometryChange(for: Edge.Set.self) { geometry in
                var more: Edge.Set = []
                if geometry.visibleRect.minY > 1 { more.insert(.top) }
                if geometry.visibleRect.maxY < geometry.contentSize.height - 1 { more.insert(.bottom) }
                return more
            } action: { _, now in
                withAnimation(Motion.reduced(Motion.fade, reduceMotion)) { more = now }
            }
            .mask {
                VStack(spacing: 0) {
                    LinearGradient(colors: [.black.opacity(more.contains(.top) ? 0 : 1), .black], startPoint: .top, endPoint: .bottom).frame(height: 32)
                    Color.black
                    LinearGradient(colors: [.black, .black.opacity(more.contains(.bottom) ? 0 : 1)], startPoint: .top, endPoint: .bottom).frame(height: 32)
                }
            }
    }
}
