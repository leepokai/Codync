import CodyncKit
import SwiftUI

/// The desktop's model panel (`ModelPicker`) as a sheet's body. On the left is a rail of the agents' logins, where a
/// tap switches agent and a "+" adds a login profile to an agent that has them. On the right are the agent's models
/// under a search field, with a check on the one in use; a typed ID the agent doesn't list is offered as a model ID.
/// Under the models are the model's effort (a slider) and its other settings (menus). The footer holds Standard / Fast
/// when the agent has Fast mode, then `notes` (why a login rests, a save that failed). Picking a model keeps the panel
/// open, so its settings can follow; another model starts without settings.
struct ModelPanel: View {
    let backend: String
    let loginProfile: String?
    @Binding var model: String?
    @Binding var options: [String: String]
    /// Fast mode, when this place can change it: shown where the agent offers it or it is on.
    var fast: Binding<Bool>?
    /// The saved bot, for whether its sessions offer Fast mode; nil for a thread without one.
    var bot: Bot?
    let seats: [LoginSeat]
    let pickSeat: (AgentLogin) -> Void
    var notes: [(text: String, tone: Color)] = []
    @Environment(BotStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var models = AgentModelCatalog()
    @State private var query = ""
    @State private var adding: Backend?

    private var current: AgentLogin { AgentLogin(backend: backend, loginProfile: loginProfile) }
    private var choices: [(id: String, name: String)] { AgentModels.choices(models.catalog, selection: model) }
    private var value: String { AgentModels.listValue(models.catalog, model: model) }
    private var selected: AgentModels.Model? { AgentModels.selected(models.catalog, model: model) }
    private var showsFast: Bool {
        guard let fast else { return false }
        return AgentModels.showsFast(models.catalog, backend: backend, on: fast.wrappedValue, bot: bot)
    }
    private var typed: String { query.trimmingCharacters(in: .whitespaces) }
    private var found: [(id: String, name: String)] {
        typed.isEmpty ? choices : choices.filter { L10n.tr($0.name).localizedCaseInsensitiveContains(typed) }
    }
    /// What's typed, offered as a model ID when nothing listed has it.
    private var custom: String? {
        typed.isEmpty || choices.contains(where: { $0.id == typed || $0.name == typed }) ? nil : typed
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 0) {
                rail
                VStack(alignment: .leading, spacing: 0) {
                    search
                    list
                    modelSettings
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
            footer
        }
        .task(id: models.key(store, backend: backend, loginProfile: loginProfile)) {
            await models.load(store, backend: backend, loginProfile: loginProfile, animation: Motion.reduced(Motion.fade, reduceMotion))
        }
        .codyncSheet(item: $adding) { AgentSheet(initial: $0) }
    }

    /// The agents' logins, each its logo; the one in use has a bar at the rail's edge.
    private var rail: some View {
        ScrollView {
            VStack(spacing: 4) {
                ForEach(seats) { seat in
                    let on = seat.login == current
                    Button { if !on { pickSeat(seat.login) } } label: {
                        AgentIcon(registry: store.agentRegistry(seat.login.backend), size: 22)
                            .frame(width: 44, height: 44)
                            .overlay(alignment: .bottomTrailing) { badge(seat) }
                            .overlay(alignment: .trailing) {
                                if on { Capsule().fill(Palette.text).frame(width: 3, height: 20).offset(x: 4) }
                            }
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(PressScale())
                    .accessibilityLabel(seat.name)
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
                if let addsTo = store.hello?.backends.first(where: { $0.id == backend && $0.loginProfiles != nil }) {
                    IconButton("Add login profile", systemImage: "plus") { adding = addsTo }
                }
            }
            .padding(4)
        }
        .scrollIndicators(.hidden)
        .frame(width: 52)
        .frame(maxHeight: .infinity)
        .background(Palette.surface)
    }

    /// A login profile beside the agent's default login: its name's first letter at the logo's corner.
    @ViewBuilder private func badge(_ seat: LoginSeat) -> some View {
        if seat.login.loginProfile != nil, let initial = seat.name.components(separatedBy: " · ").last?.first {
            Text(String(initial).uppercased())
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(Palette.background)
                .frame(minWidth: 14, minHeight: 14)
                .background(Palette.text, in: RoundedRectangle(cornerRadius: 4))
                .padding(4)
        }
    }

    private var search: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").font(.caption).foregroundStyle(Palette.tertiary)
            TextField(L10n.tr("Search models"), text: $query).plainTextInput()
            ZStack {
                if models.loading {
                    Spinner(size: 12).accessibilityLabel("Loading available models")
                } else {
                    IconButton(models.failure == nil ? "Refresh model list" : "Retry model list", systemImage: "arrow.clockwise") { models.retry += 1 }
                }
            }
            .frame(width: 36, height: 36)
        }
        .padding(.leading, 16).padding(.trailing, 8)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1).padding(.horizontal, 12) }
    }

    private var list: some View {
        ScrollView {
            LazyVStack(spacing: 2) {
                ForEach(found, id: \.id) { choice in
                    row(L10n.tr(choice.name), on: choice.id == value) { choose(choice.id) }
                }
                if let custom {
                    Button { choose(custom); query = "" } label: {
                        HStack(spacing: 8) {
                            Image(systemName: "plus").font(.caption).foregroundStyle(Palette.secondary)
                            Text(custom).foregroundStyle(Palette.text).lineLimit(1)
                            Spacer(minLength: 8)
                            Text(L10n.tr("Model ID")).font(.caption).foregroundStyle(Palette.tertiary)
                        }
                        .padding(.horizontal, 10).frame(minHeight: 44).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if let empty { Text(empty).font(.caption).foregroundStyle(Palette.tertiary).frame(maxWidth: .infinity, alignment: .leading).padding(10) }
            }
            .padding(.horizontal, 8).padding(.vertical, 8)
        }
        .scrollIndicators(.hidden)
        .scrollDismissesKeyboard(.interactively)
    }

    /// Why the list shows nothing (more), once it has loaded.
    private var empty: String? {
        let listed = !(models.catalog?.models.isEmpty ?? true)
        if models.loading || custom != nil || (listed && !found.isEmpty) { return nil }
        return models.failure.map { L10n.tr($0) } ?? (listed ? L10n.tr("No results") : L10n.tr("This agent doesn't advertise a model list. Leave Default or enter a model ID."))
    }

    private func row(_ name: String, on: Bool, pick: @escaping () -> Void) -> some View {
        Button(action: pick) {
            HStack(spacing: 8) {
                AgentIcon(registry: store.agentRegistry(backend), size: 16)
                Text(name).foregroundStyle(Palette.text).lineLimit(1)
                if on { Image(systemName: "checkmark").font(.caption.weight(.semibold)).foregroundStyle(Palette.text) }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .frame(minHeight: 44)
            .background(on ? Palette.bubbleAgent : .clear, in: RoundedRectangle(cornerRadius: 10))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    /// The model's effort as a slider, then its other settings, each a menu of its values.
    @ViewBuilder private var modelSettings: some View {
        let shown = AgentModels.settings(models.catalog, model: model)
        if !shown.isEmpty || selected?.optionsError != nil {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(shown) { option in
                    if option.isReasoning {
                        ReasoningSlider(option: option, selection: Binding(get: { options[option.id] }, set: { options[option.id] = $0 }))
                    } else {
                        DropdownMenu {
                            option.values.map { item in
                                MenuItem(item.name, selected: item.id == (options[option.id] ?? option.default)) { options[option.id] = item.id }
                            }
                        } label: {
                            HStack(spacing: 6) {
                                Text(option.name).foregroundStyle(Palette.secondary)
                                Spacer(minLength: 8)
                                Text(option.values.first { $0.id == (options[option.id] ?? option.default) }?.name ?? L10n.tr("Default"))
                                    .foregroundStyle(Palette.text).lineLimit(1)
                                Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(Palette.tertiary)
                            }
                            .font(.subheadline)
                            .frame(minHeight: 36)
                            .contentShape(Rectangle())
                        }
                    }
                }
                if let error = selected?.optionsError { Text(error).font(.caption).foregroundStyle(Palette.warning) }
            }
            .padding(.horizontal, 16).padding(.vertical, 10)
            .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1).padding(.horizontal, 12) }
        }
    }

    @ViewBuilder private var footer: some View {
        if showsFast || !notes.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                if showsFast, let fast {
                    SegmentedChoice(selection: fast, options: [(id: false, label: "Standard"), (id: true, label: "Fast")])
                    if let refusal = bot?.fastModeRefusal { Text(refusal).font(.caption).foregroundStyle(Palette.secondary) }
                }
                ForEach(Array(notes.enumerated()), id: \.offset) { _, note in
                    Text(note.text).font(.caption).foregroundStyle(note.tone)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16).padding(.vertical, 12)
            .background(Palette.surface)
        }
    }

    private func choose(_ id: String) {
        guard id != value else { return }
        withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
            model = id.isEmpty ? nil : id
            options = [:]
        }
    }
}
