import CodyncKit
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
/// The Marketplace (Grok Bot's layout): one page of agents, connectors and
/// skills for the bots on the paired computer. Installing happens on that
/// computer; each bot turns connectors and skills on in its settings.
public struct MarketplaceView: View {
    @Environment(BotStore.self) private var model
    @State private var search = ""
    @State private var connectors: [MarketConnector] = []
    @State private var skills: [MarketSkill] = []
    @State private var loadingConnectors = true
    @State private var loadingSkills = true
    @State private var error: String?
    @State private var installing: MarketConnector?
    @State private var busySkill: String?
    @State private var addingConnector = false
    @State private var writingSkill = false
    @State private var agent: Backend?
    @State private var showInstalled = false
    /// Shown as a sheet: a close button in the header instead of the navigation bar.
    let close: (() -> Void)?

    public init(close: (() -> Void)? = nil) { self.close = close }

    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    private var agents: [Backend] {
        let all = (model.hello?.backends ?? []).filter { $0.available || $0.installed == true || $0.curated == true }
        let hits = query.isEmpty ? all : all.filter { $0.name.localizedCaseInsensitiveContains(query) }
        return Array(hits.prefix(query.isEmpty ? 8 : 12))
    }

    private var shownSkills: [MarketSkill] {
        query.isEmpty ? skills : skills.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.description.localizedCaseInsensitiveContains(query) }
    }

    private var installedCount: Int { model.installedConnectors.count + model.installedSkills.count }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                header
                searchField
                if !agents.isEmpty {
                    MarketSection(title: query.isEmpty ? "Agents" : "Agents matching “\(query)”") {
                        #if os(iOS)
                        // A swipeable row on the phone, a grid on the Mac.
                        ScrollView(.horizontal, showsIndicators: false) {
                            LazyHStack(spacing: 10) {
                                ForEach(agents) { b in agentCard(b).frame(width: 128) }
                            }
                            .padding(.horizontal, 24)
                        }
                        .padding(.horizontal, -24)
                        #else
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                            ForEach(agents) { agentCard($0) }
                        }
                        #endif
                    }
                }
                MarketSection(title: query.isEmpty ? "Connectors" : "Connectors") {
                    if loadingConnectors {
                        SkeletonGrid()
                    } else if connectors.isEmpty {
                        Text(query.isEmpty ? "Couldn't load connectors." : "No connectors match “\(query)”.")
                            .foregroundStyle(Palette.secondary)
                    } else {
                        ItemGrid {
                            ForEach(connectors) { c in
                                MarketRow(title: c.title, subtitle: c.description ?? c.name, added: c.installed) {
                                    ServiceLogo(website: c.website, name: c.title, registryName: c.name)
                                } add: {
                                    installing = c
                                }
                            }
                        }
                    }
                }
                MarketSection(title: "Skills") {
                    if loadingSkills {
                        SkeletonGrid()
                    } else {
                        ItemGrid {
                            ForEach(shownSkills) { s in
                                let added = s.installed || model.installedSkills.contains { $0.id == s.source.lowercased() }
                                MarketRow(title: s.name, subtitle: s.description, added: added, busy: busySkill == s.source) {
                                    SkillGlyph()
                                } add: {
                                    busySkill = s.source
                                    Task {
                                        do { try await model.installSkill(source: s.source) } catch { self.error = error.localizedDescription }
                                        busySkill = nil
                                    }
                                }
                            }
                        }
                    }
                }
                MarketSection(title: "Make your own") {
                    ItemGrid {
                        MarketRow(title: "Custom connector", subtitle: "Any MCP server: a command or a URL", added: false, addLabel: "New") {
                            TileIcon(systemName: "point.3.connected.trianglepath.dotted")
                        } add: {
                            addingConnector = true
                        }
                        MarketRow(title: "Your own skill", subtitle: "Write instructions a bot can follow", added: false, addLabel: "New") {
                            TileIcon(systemName: "square.and.pencil")
                        } add: {
                            writingSkill = true
                        }
                    }
                }
                if let error {
                    Text(error).font(.footnote).foregroundStyle(Palette.danger)
                }
                Text("Connectors come from the official MCP Registry, skills from Anthropic, agents from the ACP registry. Everything installs on \(model.hostName).")
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, close == nil ? 24 : 44)
            .padding(.vertical, 20)
            .frame(maxWidth: 980)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .overlay(alignment: .topTrailing) {
            if let close {
                Button("Close", systemImage: "xmark", action: close)
                    .labelStyle(.iconOnly)
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
                    .buttonStyle(.plain)
                    .keyboardShortcut(.cancelAction)
                    .help("Close")
                    .padding(8)
            }
        }
        .navigationTitle(close == nil ? "Marketplace" : "")
        #if os(iOS)
        .toolbar(close == nil ? .automatic : .hidden, for: .navigationBar)
        #else
        // The page draws its own title and close button; no empty bar above it.
        .toolbar(close == nil ? .automatic : .hidden, for: .windowToolbar)
        #endif
        .navigationDestination(isPresented: $showInstalled) { InstalledView(ownsHeader: close != nil) }
        .task {
            await model.refreshPlugins()
        }
        // Picks up CLIs installed or signed in outside Codync.
        .task { await model.refreshBackends() }
        .task { await loadConnectors() }
        .task {
            do { skills = try await model.marketSkills() } catch { self.error = error.localizedDescription }
            loadingSkills = false
        }
        .sheet(item: $installing) { item in
            NavigationStack { InstallConnectorSheet(item: item) { Task { await loadConnectors() } } }
                #if os(macOS)
                .frame(minWidth: 460, minHeight: 420)
                #endif
        }
        .sheet(isPresented: $addingConnector) {
            NavigationStack { CustomConnectorSheet() }
                #if os(macOS)
                .frame(minWidth: 460, minHeight: 420)
                #endif
        }
        .sheet(isPresented: $writingSkill) {
            NavigationStack { NewSkillSheet() }
                #if os(macOS)
                .frame(minWidth: 480, minHeight: 480)
                #endif
        }
        .sheet(item: $agent) { b in
            NavigationStack { AgentSheet(initial: b) }
                #if os(macOS)
                .frame(minWidth: 640, minHeight: 460)
                #endif
        }
    }

    private var header: some View {
        HStack(alignment: .center) {
            Text("Marketplace")
                .font(.title2.weight(.semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                .layoutPriority(1)
            Spacer()
            if installedCount > 0 {
                Button { showInstalled = true } label: {
                    HStack(spacing: 8) {
                        HStack(spacing: -8) {
                            ForEach(model.installedConnectors.prefix(3)) { c in
                                ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 26)
                            }
                        }
                        Text("\(installedCount) installed").foregroundStyle(Palette.secondary).lineLimit(1).fixedSize()
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.top, close == nil ? 8 : 28)
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField("Search agents, connectors and skills", text: $search)
                .textFieldStyle(.plain)
                .plainTextInput()
                .onSubmit { Task { await loadConnectors() } }
            if !search.isEmpty {
                Button { search = ""; Task { await loadConnectors() } } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.tertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
        .background(Palette.bubbleAgent, in: Capsule())
    }

    private func agentCard(_ b: Backend) -> some View {
        AgentCard(backend: b) { agent = b }
    }

    private func loadConnectors() async {
        loadingConnectors = true
        do {
            connectors = try await model.marketConnectors(search: query)
        } catch {
            connectors = []
            self.error = error.localizedDescription
        }
        loadingConnectors = false
    }
}

// MARK: - Installed

private struct InstalledView: View {
    /// In the Marketplace sheet the bar is hidden, so the page draws its own back button.
    let ownsHeader: Bool
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var removing: Removal?

    private struct Removal: Identifiable {
        let id: String
        let name: String
        let isSkill: Bool
    }

    var body: some View {
        CardForm {
            if ownsHeader {
                HStack(spacing: 6) {
                        Button("Back", systemImage: "chevron.left") { dismiss() }
                            .labelStyle(.iconOnly)
                            .font(.system(size: 15, weight: .semibold))
                            .foregroundStyle(Palette.text)
                            .frame(width: 44, height: 44)
                            .contentShape(Rectangle())
                            .buttonStyle(.plain)
                            .keyboardShortcut(.cancelAction)
                            .help("Back to Marketplace")
                        Text("Installed").font(.title2.weight(.semibold)).foregroundStyle(Palette.text)
                }
                .padding(.leading, -12)
            }
            if model.installedConnectors.isEmpty && model.installedSkills.isEmpty {
                ContentUnavailableView("Nothing installed", systemImage: "shippingbox", description: Text("Connectors and skills you add show up here."))
            }
            if !model.installedConnectors.isEmpty {
                CardSection("Connectors") {
                    ForEach(model.installedConnectors) { c in
                        InstalledRow(title: c.name, subtitle: c.command ?? c.url ?? c.description) {
                            ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 32)
                        } remove: {
                            removing = Removal(id: c.id, name: c.name, isSkill: false)
                        }
                    }
                }
            }
            if !model.installedSkills.isEmpty {
                CardSection("Skills") {
                    ForEach(model.installedSkills) { s in
                        InstalledRow(title: s.name, subtitle: s.description) {
                            SkillGlyph().frame(width: 32, height: 32)
                        } remove: {
                            removing = Removal(id: s.id, name: s.name, isSkill: true)
                        }
                    }
                }
            }
        }
        .navigationTitle("Installed")
        .inlineNavigationTitle()
        .codyncDialog(
            "Remove \(removing?.name ?? "")?",
            isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
            message: removing.map { $0.isSkill ? "Bots stop using this skill." : "Bots lose this connector, and the keys saved for it are deleted." }
        ) {
            guard let r = removing else { return [] }
            return [DialogAction("Remove", destructive: true) {
                Task {
                    do {
                        if r.isSkill { try await model.removeSkill(r.id) } else { try await model.removeConnector(r.id) }
                    } catch {
                        model.lastError = error.localizedDescription
                    }
                }
            }]
        }
    }
}

private struct InstalledRow<Icon: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder let icon: Icon
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            icon
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(Palette.text).lineLimit(1)
                Text(subtitle).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            Spacer(minLength: 12)
            Button("Remove \(title)", systemImage: "trash", role: .destructive, action: remove)
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
                .frame(minWidth: 44, minHeight: 44)
                .contentShape(Rectangle())
                .help("Remove")
        }
        .help(subtitle)
    }
}

// MARK: - Pieces

private struct MarketSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline).foregroundStyle(Palette.text).padding(.leading, 4)
            content
        }
    }
}

/// Two columns on wide screens, one on a phone.
private struct ItemGrid<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 10)], spacing: 4) { content }
    }
}

private struct AgentCard: View {
    let backend: Backend
    let action: () -> Void
    @State private var hovering = false

    private var status: String {
        switch (backend.installed == true, backend.signedIn) {
        case (true, false?): "Not signed in"
        case (true, _): "Installed"
        case (false, _): backend.curated == true ? "Not installed" : "Sets up on first use"
        }
    }

    var body: some View {
        Button(action: action) {
            VStack(spacing: 12) {
                AgentIcon(registry: backend.registry, size: 34)
                    .frame(width: 64, height: 64)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                VStack(spacing: 2) {
                    Text(backend.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 18)
            .padding(.horizontal, 10)
            .background(Palette.bubbleAgent.opacity(hovering ? 0.8 : 0.45), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .buttonStyle(PressScale())
        .onHover { h in withAnimation(Motion.hover) { hovering = h } }
        .help(backend.name)
    }
}

/// Getting an agent ready on the computer: install it, then sign in. Sign-in
/// options come from the agent itself (ACP), plus Codync's own command for
/// CLIs it knows. Bots are made from New chat, not here.
private struct AgentSheet: View {
    let initial: Backend
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var route: SetupRoute?
    @State private var auth: AgentAuth?
    @State private var checking = false
    @State private var authenticating: String?
    @State private var error: String?

    /// Live: an install or sign-in updates the host's list.
    private var backend: Backend { model.hello?.backends.first { $0.id == initial.id } ?? initial }
    private var installed: Bool { backend.installed == true }
    private var curated: Bool { backend.curated == true }
    private var signedIn: Bool? { auth?.signedIn ?? backend.signedIn }

    var body: some View {
        CardForm {
            CardSection {
                HStack(spacing: 14) {
                    AgentIcon(registry: backend.registry, size: 30)
                        .frame(width: 52, height: 52)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(backend.name).font(.title3.weight(.semibold))
                        if let d = backend.description, !d.isEmpty {
                            Text(d).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
            if curated {
                CardSection {
                    SetupStepRow(
                        number: 1,
                        title: "Install",
                        detail: installed
                            ? "Installed on \(model.hostName)."
                            : backend.canInstall == true ? "Runs the official installer on \(model.hostName)." : backend.installHint,
                        done: installed,
                        action: installed || backend.canInstall != true ? nil : ("Install", { route = .install })
                    )
                }
            }
            signInSection
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger).textSelection(.enabled) }
            }
        }
        .navigationTitle(backend.name)
        .inlineNavigationTitle()
        .navigationDestination(item: $route) { route in
            switch route {
            case .install: SetupTerminalView(backend: backend, step: .install)
            case .terminal(let method): SetupTerminalView(backend: backend, step: .login, method: method)
            case .keys(let method): AgentKeysForm(backend: backend, method: method, saved: auth?.savedEnv ?? []) { auth = $0 }
            }
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        // A known "signed in" needs no agent start; everything else asks the agent.
        .task { if backend.signedIn != true { await check() } }
        .onChange(of: route) { old, new in
            // Back from a terminal or key form: see what changed.
            if new == nil, old != nil { Task { await check() } }
        }
    }

    private var signInSection: some View {
        CardSection(footer: signedIn != true && auth?.methods.contains(where: { $0.kind == .agent }) == true
            ? "Browser sign-ins open on \(model.hostName) itself. Away from it? Use Remote screen to finish there."
            : nil) {
            HStack(spacing: 0) {
                SetupStepRow(number: curated ? 2 : 1, title: "Sign in", detail: statusText, done: signedIn == true, action: nil)
                if checking {
                    ThinkingOrb(state: .connecting, size: 16, color: Palette.secondary)
                } else {
                    Button("Check again", systemImage: "arrow.clockwise") { Task { await check() } }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.secondary)
                        .help("Check again")
                }
            }
            if signedIn != true, let detail = auth?.detail, !detail.isEmpty {
                Text(detail).font(.footnote).foregroundStyle(Palette.secondary).textSelection(.enabled)
            }
            // Some CLIs drop the current sign-in the moment a new one starts: no options once signed in.
            if signedIn != true, !checking {
                if auth?.login == true {
                    optionRow("Sign in", detail: "In a terminal on \(model.hostName). Links open here.", icon: "terminal") {
                        route = .terminal(nil)
                    }
                }
                ForEach(auth?.methods ?? []) { m in
                    switch m.kind {
                    case .terminal?:
                        optionRow(m.name, detail: m.description ?? "In a terminal on \(model.hostName).", icon: "terminal") {
                            route = .terminal(m)
                        }
                    case .envVar?:
                        optionRow(m.name, detail: m.description ?? "Saved on \(model.hostName) only.", icon: "key") {
                            route = .keys(m)
                        }
                    case .agent?, nil:
                        optionRow(
                            m.name,
                            detail: authenticating == m.id
                                ? "Finish signing in in the browser on \(model.hostName)…"
                                : m.description ?? "Opens a browser on \(model.hostName).",
                            icon: "safari",
                            busy: authenticating == m.id
                        ) {
                            Task { await authenticate(m) }
                        }
                    }
                }
            }
        }
    }

    private var statusText: String {
        if checking { return auth == nil ? "Checking with \(backend.name)… The first check can download it." : "Checking…" }
        switch signedIn {
        case true?: return "Signed in."
        case false?: return "Not signed in yet."
        case nil: return "Couldn't tell. Skip this if you've already signed in on \(model.hostName)."
        }
    }

    private func optionRow(_ title: String, detail: String, icon: String, busy: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: icon)
                    .font(.body)
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 26)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                    Text(detail).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
                }
                Spacer(minLength: 8)
                if busy {
                    ThinkingOrb(size: 16, color: Palette.secondary)
                } else {
                    Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(authenticating != nil)
    }

    private func check() async {
        guard let client = model.client, !checking else { return }
        checking = true
        defer { checking = false }
        do {
            auth = try await client.agentAuth(backend.id)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
        await model.refreshBackends()
    }

    private func authenticate(_ m: AuthMethod) async {
        guard let client = model.client else { return }
        authenticating = m.id
        defer { authenticating = nil }
        do {
            auth = try await client.agentAuthenticate(backend.id, method: m.id)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
        await model.refreshBackends()
    }
}

private enum SetupRoute: Hashable {
    case install
    case terminal(AuthMethod?)
    case keys(AuthMethod)
}

/// Keys an agent reads from its environment, kept on the computer.
private struct AgentKeysForm: View {
    let backend: Backend
    let method: AuthMethod
    let saved: [String]
    let done: (AgentAuth) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var values: [String: String] = [:]
    @State private var saving = false
    @State private var error: String?

    private var vars: [AuthMethod.Var] { method.vars ?? [] }

    var body: some View {
        CardForm {
            VStack(alignment: .leading, spacing: 6) {
                CardSection(method.name) {
                    ForEach(vars, id: \.name) { v in
                        VStack(alignment: .leading, spacing: 4) {
                            let prompt = Text(saved.contains(v.name) ? "Saved (type to replace)" : v.name)
                            Group {
                                if v.secret {
                                    SecureField(v.label, text: binding(v.name), prompt: prompt)
                                } else {
                                    TextField(v.label, text: binding(v.name), prompt: prompt)
                                }
                            }
                            .plainTextInput()
                            Text(v.label + (v.optional ? " (optional)" : "")).font(.caption).foregroundStyle(Palette.secondary)
                        }
                    }
                }
                VStack(alignment: .leading, spacing: 6) {
                    if let d = method.description { Text(d) }
                    Text("Saved on \(model.hostName) only and given to \(backend.name) when it starts.")
                    if let link = method.link, let url = URL(string: link) {
                        Link("Get a key", destination: url)
                    }
                }
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .padding(.horizontal, 4)
            }
            if !saved.isEmpty {
                CardSection {
                    Button("Remove saved keys", role: .destructive) {
                        save(Dictionary(uniqueKeysWithValues: vars.map { ($0.name, "") }))
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.danger)
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .navigationTitle(backend.name)
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    Spinner()
                } else {
                    Button("Save") { save(values.filter { !$0.value.isEmpty }) }
                        .disabled(vars.contains { !$0.optional && (values[$0.name] ?? "").isEmpty && !saved.contains($0.name) })
                }
            }
        }
    }

    private func binding(_ name: String) -> Binding<String> {
        Binding(get: { values[name] ?? "" }, set: { values[name] = $0 })
    }

    private func save(_ vars: [String: String]) {
        guard let client = model.client else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                done(try await client.setAgentEnv(backend.id, vars: vars))
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

private struct SetupStepRow: View {
    let number: Int
    let title: String
    let detail: String
    let done: Bool
    let action: (label: String, run: () -> Void)?

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle().fill(done ? Palette.accentFill : Palette.bubbleAgent)
                if done {
                    Image(systemName: "checkmark").font(.caption.weight(.bold)).foregroundStyle(Palette.onAccent)
                } else {
                    Text("\(number)").font(.caption.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
            }
            .frame(width: 26, height: 26)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                Text(detail).font(.subheadline).foregroundStyle(Palette.secondary).textSelection(.enabled)
            }
            Spacer(minLength: 8)
            if let action {
                Button(action.label, action: action.run)
                    .buttonStyle(.plain)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Palette.bubbleAgent, in: Capsule())
                    .fixedSize()
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityValue(done ? "Done" : "")
    }
}

private struct MarketRow<Icon: View>: View {
    let title: String
    let subtitle: String
    let added: Bool
    var busy = false
    var addLabel = "Add"
    @ViewBuilder let icon: Icon
    let add: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 14) {
            icon.frame(width: 46, height: 46)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.text).lineLimit(1)
                Text(subtitle).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if busy {
                ThinkingOrb(size: 18, color: Palette.secondary).frame(width: 60)
            } else if added {
                Label("Added", systemImage: "checkmark")
                    .labelStyle(.iconOnly)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 60)
                    .accessibilityLabel("Added")
            } else {
                Button(addLabel, action: add)
                    .buttonStyle(.plain)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 7)
                    .background(Palette.bubbleAgent, in: Capsule())
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(hovering ? Palette.bubbleAgent.opacity(0.6) : .clear, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .onHover { h in withAnimation(Motion.hover) { hovering = h } }
    }
}

/// Placeholder rows while a list loads, shaped like the real ones.
private struct SkeletonGrid: View {
    @State private var dim = false

    var body: some View {
        ItemGrid {
            ForEach(0..<4, id: \.self) { _ in
                HStack(spacing: 14) {
                    RoundedRectangle(cornerRadius: 12, style: .continuous).frame(width: 46, height: 46)
                    VStack(alignment: .leading, spacing: 8) {
                        Capsule().frame(width: 120, height: 12)
                        Capsule().frame(width: 190, height: 10)
                    }
                    Spacer()
                }
                .foregroundStyle(Palette.bubbleAgent)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
            }
        }
        .opacity(dim ? 0.5 : 1)
        .animation(.easeInOut(duration: 0.9).repeatForever(), value: dim)
        .onAppear { dim = true }
    }
}

/// A service's logo from its website favicon, on a white tile; its initial otherwise.
private struct ServiceLogo: View {
    let website: String?
    let name: String
    /// MCP Registry names are reverse-DNS ("com.notion/mcp"): the owner's domain.
    var registryName: String?
    var size: CGFloat = 46

    private var domain: String? {
        if let website, let host = URL(string: website)?.host(), host != "github.com" { return host }
        guard let owner = registryName?.split(separator: "/").first else { return nil }
        let labels = owner.split(separator: ".")
        // io.github.<user> is a GitHub account, not the service's own site.
        guard labels.count >= 2, !(labels[0] == "io" && labels[1] == "github") else {
            return name.localizedCaseInsensitiveContains("github") ? "github.com" : nil
        }
        return "\(labels[1]).\(labels[0])"
    }

    private var faviconURL: URL? {
        domain.flatMap { URL(string: "https://www.google.com/s2/favicons?domain=\($0)&sz=128") }
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
        ZStack {
            shape.fill(Palette.bubbleAgent)
            Text(name.first.map { String($0).uppercased() } ?? "?")
                .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
                .foregroundStyle(Palette.text)
            if let faviconURL {
                AsyncImage(url: faviconURL) { phase in
                    if let image = phase.image {
                        ZStack {
                            shape.fill(.white)
                            image.resizable().interpolation(.high).scaledToFit().padding(size * 0.2)
                        }
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .clipShape(shape)
    }
}

private struct TileIcon: View {
    let systemName: String

    var body: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(Palette.bubbleAgent)
            .overlay(Image(systemName: systemName).font(.system(size: 18, weight: .medium)).foregroundStyle(Palette.text))
    }
}

private struct InstallConnectorSheet: View {
    let item: MarketConnector
    let done: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var optionId = ""
    @State private var values: [String: String] = [:]
    @State private var saving = false
    @State private var error: String?

    private var option: MarketConnector.InstallOption? {
        item.options.first { $0.id == optionId } ?? item.options.first
    }

    var body: some View {
        CardForm {
            CardSection {
                VStack(alignment: .leading, spacing: 6) {
                    Text(item.title).font(.title3.weight(.semibold))
                    if let d = item.description { Text(d).foregroundStyle(Palette.secondary) }
                    if let w = item.website, let url = URL(string: w) {
                        Link(w, destination: url).font(.footnote).lineLimit(1)
                    }
                }
                .padding(.vertical, 4)
            }
            if item.options.count > 1 {
                CardSection("Runs") {
                    ChoiceList(selection: $optionId, options: item.options.map { o in
                        (id: o.id, label: o.kind == "remote" ? "Hosted by \(item.title)" : "On \(model.hostName) (\(o.kind))", detail: nil)
                    })
                }
            }
            if let option {
                CardSection("Setup", footer: "Keys are saved on \(model.hostName) only.") {
                    if option.inputs.isEmpty {
                        Text(option.kind == "remote"
                            ? "No keys needed here. If the service asks you to sign in, the agent shows how the first time it's used."
                            : "No setup needed.")
                            .foregroundStyle(Palette.secondary)
                    }
                    ForEach(option.inputs) { input in
                        VStack(alignment: .leading, spacing: 4) {
                            Group {
                                if input.secret {
                                    SecureField(input.name, text: binding(input))
                                } else {
                                    TextField(input.name, text: binding(input), prompt: Text(input.default ?? input.placeholder ?? input.name))
                                }
                            }
                            .plainTextInput()
                            if let d = input.description {
                                Text(d + (input.required ? "" : " (optional)")).font(.caption).foregroundStyle(Palette.secondary)
                            }
                        }
                    }
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .navigationTitle("Add connector")
        .inlineNavigationTitle()
        .onAppear { optionId = item.options.first?.id ?? "" }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    Spinner()
                } else {
                    Button("Add") { install() }
                        .disabled(option?.inputs.contains { $0.required && (values[$0.name] ?? "").isEmpty } ?? true)
                }
            }
        }
    }

    private func binding(_ input: MarketConnector.Input) -> Binding<String> {
        Binding(get: { values[input.name] ?? "" }, set: { values[input.name] = $0 })
    }

    private func install() {
        guard let option else { return }
        saving = true
        error = nil
        Task {
            do {
                try await model.installConnector(item, option: option.id, inputs: values)
                done()
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}

private struct CustomConnectorSheet: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var remote = false
    @State private var target = ""
    @State private var envText = ""
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        CardForm {
            CardSection(footer: remote ? "A remote MCP server (streamable HTTP)." : "Runs on \(model.hostName) in the bot's project folder.") {
                TextField("Name", text: $name)
                SegmentedChoice(selection: $remote, options: [(id: false, label: "Command"), (id: true, label: "URL")])
                TextField(remote ? "https://example.com/mcp" : "npx -y @scope/server", text: $target)
                    .font(.callout.monospaced())
                    .plainTextInput()
            }
            if !remote {
                CardSection("Environment", footer: "One KEY=value per line. Saved on \(model.hostName) only.") {
                    TextField("API_KEY=…", text: $envText, axis: .vertical)
                        .lineLimit(2...6)
                        .font(.callout.monospaced())
                        .plainTextInput()
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .navigationTitle("Custom connector")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    Spinner()
                } else {
                    Button("Add") { save() }
                        .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || target.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func save() {
        saving = true
        error = nil
        var env: [String: String] = [:]
        for line in envText.split(separator: "\n") {
            let parts = line.split(separator: "=", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
            if parts.count == 2, !parts[0].isEmpty { env[parts[0]] = parts[1] }
        }
        Task {
            do {
                try await model.addConnector(name: name, command: remote ? nil : target, url: remote ? target : nil, env: env)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}

private struct NewSkillSheet: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var summary = ""
    @State private var instructions = ""
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        CardForm {
            CardSection(footer: "The bot sees the name and when to use it, and reads the instructions only when a task fits.") {
                TextField("Name", text: $name)
                TextField("When to use it", text: $summary, axis: .vertical).lineLimit(2...4)
            }
            CardSection("Instructions") {
                TextField("Step by step, in plain words…", text: $instructions, axis: .vertical)
                    .lineLimit(6...20)
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
        .navigationTitle("New skill")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    Spinner()
                } else {
                    Button("Save") {
                        saving = true
                        Task {
                            do {
                                try await model.addSkill(name: name, description: summary, instructions: instructions)
                                dismiss()
                            } catch {
                                self.error = error.localizedDescription
                            }
                            saving = false
                        }
                    }
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || instructions.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }
}

private struct SkillGlyph: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(Palette.bubbleAgent)
            .overlay(Image(systemName: "book.pages").foregroundStyle(Palette.text))
    }
}

/// A coding agent's logo from the ACP registry, tinted like text; a terminal glyph otherwise.
public struct AgentIcon: View {
    let registry: String?
    let size: CGFloat

    public init(registry: String?, size: CGFloat = 20) {
        self.registry = registry
        self.size = size
    }

    public var body: some View {
        Group {
            if let name = registry.map({ "agent-\($0)" }), Self.exists(name) {
                Image(name, bundle: .module).resizable().scaledToFit()
            } else {
                Image(systemName: "terminal").resizable().scaledToFit().padding(size * 0.1)
            }
        }
        .frame(width: size, height: size)
        .foregroundStyle(tint)
    }

    /// Registry logos are one-color; paint the ones with a known brand color in it.
    private var tint: AnyShapeStyle {
        switch registry {
        case "claude-acp": AnyShapeStyle(Color(red: 0.85, green: 0.47, blue: 0.34))
        case "gemini": AnyShapeStyle(LinearGradient(colors: [Color(red: 0.28, green: 0.59, blue: 0.89), Color(red: 0.57, green: 0.47, blue: 0.78), Color(red: 0.79, green: 0.40, blue: 0.45)], startPoint: .bottomLeading, endPoint: .topTrailing))
        case "antigravity-acp": AnyShapeStyle(Color(red: 0.26, green: 0.52, blue: 0.96))
        case "mistral-vibe": AnyShapeStyle(Color(red: 0.98, green: 0.32, blue: 0.06))
        case "qwen-code": AnyShapeStyle(Color(red: 0.38, green: 0.36, blue: 0.93))
        case "amp-acp": AnyShapeStyle(Color(red: 0.95, green: 0.31, blue: 0.25))
        case "kiro": AnyShapeStyle(Color(red: 0.56, green: 0.27, blue: 1.0))
        case "cortex-code": AnyShapeStyle(Color(red: 0.16, green: 0.71, blue: 0.91))
        default: AnyShapeStyle(Palette.text)
        }
    }

    private static func exists(_ name: String) -> Bool {
        #if canImport(UIKit)
        UIImage(named: name, in: .module, with: nil) != nil
        #else
        Bundle.module.image(forResource: name) != nil
        #endif
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
