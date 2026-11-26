import CodyncKit
import SwiftUI

/// The model panel on a phone, as a sheet's body. At the top the agent in use (a menu of the agents' logins, and
/// adding a login profile to an agent that has them) and a search button. Then the agent's models, the newest
/// ones first and the rest under "More models", each with what it's for and a check on the one in use; a typed ID the
/// agent doesn't list is offered as a model ID. Under them one card holds the model's effort (a slider) and its other
/// settings, the approvals (where `approvals` can change them) and Fast mode (where the agent has it), then `notes`.
/// Picking a model keeps the panel open, so its settings can follow; another model starts without settings.
struct ModelPanel: View {
    let backend: String
    let loginProfile: String?
    @Binding var model: String?
    @Binding var options: [String: String]
    /// Fast mode, when this place can change it: shown where the agent offers it or it is on.
    var fast: Binding<Bool>?
    /// The saved bot, for whether its sessions offer Fast mode; nil for a thread without one.
    var bot: Bot?
    /// The bot's approvals, where this place can change them.
    var approvals: Binding<BotDraft>?
    let seats: [LoginSeat]
    let pickSeat: (AgentLogin) -> Void
    var notes: [(text: String, tone: Color)] = []
    @Environment(BotStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var models = AgentModelCatalog()
    @State private var query = ""
    @State private var searching = false
    @State private var showsOlder = false
    @State private var adding: Backend?

    /// How many models show before "More models": an agent lists its newest first.
    private static let newest = 6

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
    /// The models under "More models"; none while searching.
    private var older: [(id: String, name: String)] { typed.isEmpty ? Array(found.dropFirst(Self.newest)) : [] }
    private var shown: [(id: String, name: String)] {
        typed.isEmpty && !showsOlder ? Array(found.prefix(Self.newest)) : found
    }

    var body: some View {
        VStack(spacing: 0) {
            top
            if searching {
                SearchField(L10n.tr("Search or enter a model ID"), text: $query, autoFocus: true)
                    .padding(.horizontal, 16).padding(.bottom, 8)
                    .transition(.opacity)
            }
            list
            // While searching, the list gets the room the keyboard leaves.
            if !searching { settings }
            if !searching, !notes.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(notes.enumerated()), id: \.offset) { _, note in
                        Text(note.text).font(.caption).foregroundStyle(note.tone)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20).padding(.bottom, 12)
            }
        }
        .task(id: models.key(store, backend: backend, loginProfile: loginProfile)) { await load(refresh: false) }
        // The one in use is never folded away.
        .onChange(of: models.catalog == nil) { unfoldSelected() }
        .onAppear(perform: unfoldSelected)
        .codyncSheet(item: $adding) { AgentSheet(initial: $0) }
    }

    /// The agent in use, a menu of the agents' logins; then search.
    private var top: some View {
        HStack(spacing: 8) {
            DropdownMenu {
                seats.map { seat in
                    MenuItem(seat.name, selected: seat.login == current, verbatim: true) { if seat.login != current { pickSeat(seat.login) } }
                } + addLoginProfile
            } label: {
                ChipLabel {
                    AgentIcon(registry: store.agentRegistry(backend), size: 18)
                    Text(seats.first { $0.login == current }?.name ?? backend).lineLimit(1)
                }
            }
            .accessibilityLabel(L10n.tr("Agent"))
            Spacer(minLength: 8)
            if models.loading && models.catalog == nil { Spinner(size: 14).accessibilityLabel("Loading available models") }
            IconButton(searching ? "Close search" : "Search models", systemImage: searching ? "xmark" : "magnifyingglass") {
                withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
                    searching.toggle()
                    if !searching { query = "" }
                }
            }
        }
        .padding(.horizontal, 16).padding(.top, 16).padding(.bottom, 8)
    }

    private var addLoginProfile: [MenuItem] {
        guard let addsTo = store.hello?.backends.first(where: { $0.id == backend && $0.loginProfiles != nil }) else { return [] }
        return [MenuItem("Add login profile", icon: "plus", divider: true) { adding = addsTo }]
    }

    private var list: some View {
        ScrollView {
            LazyVStack(spacing: 2) {
                ForEach(shown, id: \.id) { choice in
                    row(choice) { choose(choice.id) }
                }
                if !older.isEmpty {
                    Button {
                        withAnimation(Motion.reduced(Motion.fade, reduceMotion)) { showsOlder.toggle() }
                    } label: {
                        HStack(spacing: 6) {
                            Text(showsOlder ? L10n.tr("Fewer models") : String(format: L10n.tr("More models (%d)"), older.count))
                            Image(systemName: showsOlder ? "chevron.up" : "chevron.down").font(.caption2)
                            Spacer(minLength: 0)
                        }
                        .font(.subheadline).foregroundStyle(Palette.secondary)
                        .padding(.horizontal, 12).frame(minHeight: 44).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if let custom {
                    Button { choose(custom); query = "" } label: {
                        HStack(spacing: 8) {
                            Image(systemName: "plus").font(.caption).foregroundStyle(Palette.secondary)
                            Text(custom).foregroundStyle(Palette.text).lineLimit(1)
                            Spacer(minLength: 8)
                            Text(L10n.tr("Model ID")).font(.caption).foregroundStyle(Palette.tertiary)
                        }
                        .padding(.horizontal, 12).frame(minHeight: 52).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if let empty {
                    Text(empty).font(.caption).foregroundStyle(Palette.tertiary).frame(maxWidth: .infinity, alignment: .leading).padding(12)
                }
            }
            .padding(.horizontal, 8).padding(.bottom, 8)
        }
        .scrollIndicators(.hidden)
        .scrollDismissesKeyboard(.interactively)
        // Pull down: the computer reads the list from the agent again; the indicator stays until it has.
        .refreshable { await load(refresh: true) }
    }

    /// Why reading the list failed (the list kept before stays above it), or why it shows nothing (more), once it has loaded.
    private var empty: String? {
        if models.loading { return nil }
        if let failure = models.failure { return L10n.tr(failure) }
        let listed = !(models.catalog?.models.isEmpty ?? true)
        if custom != nil || (listed && !found.isEmpty) { return nil }
        return listed ? L10n.tr("No results") : L10n.tr("This agent doesn't advertise a model list. Leave Default or enter a model ID.")
    }

    private func row(_ choice: (id: String, name: String), pick: @escaping () -> Void) -> some View {
        let on = choice.id == value
        let about = models.catalog?.models.first { $0.id == choice.id }?.description
        return Button(action: pick) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr(choice.name)).foregroundStyle(Palette.text).lineLimit(1)
                    if let about, !about.isEmpty {
                        Text(about).font(.footnote).foregroundStyle(Palette.secondary).lineLimit(2)
                    }
                }
                Spacer(minLength: 8)
                Image(systemName: "checkmark").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).opacity(on ? 1 : 0)
            }
            .padding(.horizontal, 12).padding(.vertical, 10)
            .frame(minHeight: 52)
            .background(on ? Palette.bubbleAgent : .clear, in: RoundedRectangle(cornerRadius: 14))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    /// The model's effort as a slider and its other settings as menus, then approvals and Fast mode: one card.
    @ViewBuilder private var settings: some View {
        let shownOptions = AgentModels.settings(models.catalog, model: model)
        if !shownOptions.isEmpty || selected?.optionsError != nil || approvals != nil || showsFast {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(shownOptions) { option in
                    if option.isReasoning {
                        ReasoningSlider(option: option, selection: Binding(get: { options[option.id] }, set: { options[option.id] = $0 }))
                    } else {
                        line(option.name) {
                            ChoicePicker(selection: Binding(get: { options[option.id] ?? option.default ?? "" }, set: { options[option.id] = $0 }),
                                         options: option.values.map { ($0.id, $0.name) })
                        }
                    }
                }
                if let error = selected?.optionsError { Text(error).font(.caption).foregroundStyle(Palette.warning) }
                if let approvals {
                    let (all, current) = ApprovalChoice.choices(permission: approvals.wrappedValue.permission, mode: approvals.wrappedValue.mode,
                                                                catalog: models.catalog)
                    line("Approvals") {
                        ChoicePicker(selection: Binding(get: { current.id }, set: { approvals.wrappedValue.setApproval($0) }),
                                     options: all.map { ($0.id, $0.label) })
                    }
                }
                if showsFast, let fast {
                    line("Speed") {
                        Button { fast.wrappedValue.toggle() } label: {
                            Label(L10n.tr("Fast"), systemImage: "bolt")
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(fast.wrappedValue ? Palette.background : Palette.secondary)
                                .padding(.horizontal, 12).padding(.vertical, 7)
                                .background(fast.wrappedValue ? Palette.text : Palette.background, in: Capsule())
                        }
                        .buttonStyle(PressScale())
                        .accessibilityAddTraits(fast.wrappedValue ? .isSelected : [])
                    }
                    if let refusal = bot?.fastModeRefusal { Text(refusal).font(.caption).foregroundStyle(Palette.secondary) }
                }
            }
            .padding(14)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20))
            .padding(.horizontal, 12).padding(.bottom, 10)
        }
    }

    private func line(_ label: String, @ViewBuilder value: () -> some View) -> some View {
        HStack(spacing: 10) {
            Text(L10n.tr(label)).font(.subheadline).foregroundStyle(Palette.secondary)
            Spacer(minLength: 8)
            value()
        }
        .frame(minHeight: 32)
    }

    private func load(refresh: Bool) async {
        await models.load(store, backend: backend, loginProfile: loginProfile, refresh: refresh, animation: Motion.reduced(Motion.fade, reduceMotion))
    }

    private func unfoldSelected() {
        if found.dropFirst(Self.newest).contains(where: { $0.id == value }) { showsOlder = true }
    }

    private func choose(_ id: String) {
        guard id != value else { return }
        withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
            model = id.isEmpty ? nil : id
            options = [:]
        }
    }
}
