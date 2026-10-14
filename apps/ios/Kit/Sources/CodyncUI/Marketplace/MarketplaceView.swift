import CodyncKit
import SwiftUI

/// The Marketplace (Grok Bot's layout): one page of agents, connectors and
/// skills for the bots on one computer (the store in the environment). Installing happens on that
/// computer; each bot turns connectors and skills on in its settings.
public struct MarketplaceView: View {
    @Environment(BotStore.self) private var model
    @State private var search = ""
    /// Bumped on each submitted search, so every section searches again.
    @State private var searchToken = 0
    @State private var connectors = MarketplacePage<MarketConnector>()
    @State private var connectorRequest = UUID()
    @State private var connectorQuery = ""
    /// The registry page after the ones shown; nil at the end.
    @State private var connectorCursor: String?
    @State private var loadingMore = false
    @State private var skills: [MarketSkill] = []
    @State private var loadingConnectors = true
    @State private var loadingSkills = true
    @State private var error: String?
    @State private var installing: MarketConnector?
    @State private var busySkill: String?
    @State private var addingConnector = false
    @State private var writingSkill = false
    @State private var viewingSkill: SkillRef?
    @State private var agent: Backend?
    @State private var addingNode = false
    /// Agents shown before "Load more agents".
    @State private var agentLimit = 10
    @State private var showInstalled = false
    @State private var showCredentials = false
    @Environment(\.dismissModal) private var dismiss

    private let computers: [(id: ComputerID, label: String)]
    private let computer: Binding<ComputerID>?

    /// Each computer has its own marketplace; with `computer` set and more than one computer,
    /// the header switches between them (the caller swaps the `BotStore`).
    public init(computers: [(id: ComputerID, label: String)] = [], computer: Binding<ComputerID>? = nil) {
        self.computers = computers
        self.computer = computer
    }

    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    private var agents: [Backend] {
        let all = (model.hello?.backends ?? []).filter { $0.available || $0.installed == true || $0.curated == true }
        return query.isEmpty ? all : all.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    private var shownSkills: [MarketSkill] {
        query.isEmpty ? skills : skills.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.description.localizedCaseInsensitiveContains(query) }
    }

    private var installedCount: Int { model.installedConnectors.count + model.installedSkills.count }

    public var body: some View {
        ZStack {
            if showInstalled {
                InstalledView { withAnimation(Motion.layout) { showInstalled = false } }
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                page.transition(.move(edge: .leading).combined(with: .opacity))
            }
        }
        .background(Palette.background)
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
        .codyncSheet(item: $installing) { item in
            InstallConnectorSheet(item: item) { Task { await loadConnectors() } }
        }
        .codyncSheet(isPresented: $showCredentials) { CredentialsView() }
        .codyncSheet(isPresented: $addingConnector) { CustomConnectorSheet() }
        .codyncSheet(item: $viewingSkill) { SkillSheet(skill: $0) }
        .codyncSheet(isPresented: $writingSkill) { NewSkillSheet() }
        .codyncSheet(item: $agent) { b in AgentSheet(initial: b) }
        .codyncSheet(isPresented: $addingNode) { SetupTerminalView(backend: nil, step: .node) { addingNode = false } }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                header
                searchField
                if !agents.isEmpty {
                    MarketSection(title: query.isEmpty ? "Agents" : "Agents matching “\(query)”") {
                        if model.hello?.backends.contains(where: \.needsNode) == true {
                            NodeNotice { addingNode = true }
                        }
                        ScrollView(.horizontal, showsIndicators: false) {
                            LazyHStack(spacing: 10) {
                                ForEach(agents.prefix(agentLimit)) { b in agentCard(b).frame(width: 128) }
                            }
                            .padding(.horizontal, 24)
                        }
                        .padding(.horizontal, -24)
                        if agents.count > agentLimit {
                            Button("Load more agents") {
                                withAnimation(Motion.fade) { agentLimit += 10 }
                            }
                            .buttonStyle(SecondaryButtonStyle())
                            .frame(maxWidth: .infinity)
                        }
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
                            ForEach(connectors.visible) { c in
                                MarketRow(title: c.title, subtitle: c.description ?? c.name, added: c.installed) {
                                    ServiceLogo(website: c.website, name: c.title, registryName: c.name)
                                } add: {
                                    installing = c
                                }
                            }
                        }
                        if connectors.hasHiddenItems || connectorCursor != nil {
                            Button(loadingMore ? "Loading…" : "Load more connectors") { Task { await loadMoreConnectors() } }
                                .buttonStyle(SecondaryButtonStyle())
                                .disabled(loadingMore)
                                .frame(maxWidth: .infinity)
                        }
                    }
                }
                ComposioSection(query: query, searchToken: searchToken)
                MarketSection(title: "Skills") {
                    if loadingSkills {
                        SkeletonGrid()
                    } else {
                        ItemGrid {
                            ForEach(shownSkills) { s in
                                let added = s.installed || model.installedSkills.contains { $0.id == s.source.lowercased() }
                                MarketRow(title: s.name, subtitle: s.description, added: added, busy: busySkill == s.source,
                                          open: { viewingSkill = SkillRef(s, installed: added) }) {
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
                Button { withAnimation(Motion.layout) { showCredentials = true } } label: {
                    Label("Credentials", systemImage: "lock.shield")
                }
                .buttonStyle(SecondaryButtonStyle())
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
                Text("Connectors come from the official MCP Registry, apps through Composio, skills from Anthropic, agents from the ACP registry. Everything installs on \(model.hostName).")
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 44)
            .padding(.vertical, 20)
            .frame(maxWidth: 980)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .overlay(alignment: .topTrailing) {
            // dismissModal slides the sheet away; clearing the binding would cut it off.
            IconButton("Close", systemImage: "xmark") { dismiss() }
                .keyboardShortcut(.cancelAction)
                .padding(8)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Everything here lives on one computer; say which.
            HStack(spacing: 10) {
                ComputerBadge(model.computer, size: 32)
                VStack(alignment: .leading, spacing: 0) {
                    Text("Extensions")
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(Palette.text)
                    if let computer, computers.count > 1 {
                        DropdownMenu {
                            computers.map { option in
                                MenuItem(option.label, selected: option.id == computer.wrappedValue) { computer.wrappedValue = option.id }
                            }
                        } label: {
                            HStack(spacing: 4) {
                                Text(model.hostName)
                                Image(systemName: "chevron.down").font(.caption2.weight(.semibold))
                            }
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .contentShape(Rectangle())
                        }
                        .accessibilityLabel("Computer: \(model.hostName)")
                        .help("Switch computer")
                    } else {
                        Text(model.hostName)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                }
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            }
            .layoutPriority(1)
            if installedCount > 0 {
                Button { withAnimation(Motion.layout) { showInstalled = true } } label: {
                    HStack(spacing: 8) {
                        HStack(spacing: -8) {
                            ForEach(model.installedConnectors.prefix(3)) { c in
                                ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 26)
                            }
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(installedCount) installed")
                            Text("Connectors & skills").font(.caption)
                        }
                        .foregroundStyle(Palette.secondary)
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.top, 28)
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField("Search agents, connectors and skills", text: $search)
                .textFieldStyle(.plain)
                .plainTextInput()
                .onSubmit { searchToken += 1; Task { await loadConnectors() } }
                .onChange(of: search) { agentLimit = 10 }
            if !search.isEmpty {
                Button { search = ""; searchToken += 1; Task { await loadConnectors() } } label: {
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
        let request = UUID()
        connectorRequest = request
        connectorQuery = query
        loadingConnectors = true
        loadingMore = false
        defer { if connectorRequest == request { loadingConnectors = false } }
        do {
            let page = try await model.marketConnectors(search: connectorQuery)
            guard !Task.isCancelled, connectorRequest == request else { return }
            connectors.replace(with: page.items)
            connectorCursor = page.next
            error = nil
        } catch {
            guard !Task.isCancelled, connectorRequest == request else { return }
            connectors.replace(with: [])
            connectorCursor = nil
            self.error = error.localizedDescription
        }
    }

    private func loadMoreConnectors() async {
        guard !loadingMore, !loadingConnectors else { return }
        if connectors.hasHiddenItems {
            withAnimation(Motion.fade) { connectors.revealMore() }
            return
        }
        guard let cursor = connectorCursor else { return }
        let request = connectorRequest
        loadingMore = true
        defer { if connectorRequest == request { loadingMore = false } }
        do {
            let page = try await model.marketConnectors(search: connectorQuery, cursor: cursor)
            guard !Task.isCancelled, connectorRequest == request else { return }
            withAnimation(Motion.fade) {
                connectors.append(page.items)
                connectors.revealMore()
            }
            connectorCursor = page.next
            error = nil
        } catch {
            guard !Task.isCancelled, connectorRequest == request else { return }
            self.error = error.localizedDescription
        }
    }

}
