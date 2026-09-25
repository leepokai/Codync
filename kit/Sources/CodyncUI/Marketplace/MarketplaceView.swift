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
    @State private var newBot: EditorRequest?
    @State private var showInstalled = false
    /// Shown as a sheet: a close button in the header instead of the navigation bar.
    let close: (() -> Void)?

    public init(close: (() -> Void)? = nil) { self.close = close }

    private var query: String { search.trimmingCharacters(in: .whitespaces) }

    private var agents: [Backend] {
        let all = (model.hello?.backends ?? []).filter { $0.available || $0.installed == true }
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
            .padding(.horizontal, 24)
            .padding(.vertical, 20)
            .frame(maxWidth: 980)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
        .navigationTitle("Marketplace")
        #if os(iOS)
        .toolbar(close == nil ? .automatic : .hidden, for: .navigationBar)
        #endif
        .navigationDestination(isPresented: $showInstalled) { InstalledView() }
        .task {
            await model.refreshPlugins()
        }
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
        .sheet(item: $newBot) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
                #if os(macOS)
                .frame(minWidth: 520, minHeight: 640)
                #endif
        }
    }

    private var header: some View {
        HStack(alignment: .center) {
            Text("Marketplace")
                .font(.system(size: 30, weight: .bold))
                .tracking(-0.4)
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
                                    .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Palette.background, lineWidth: 2))
                            }
                        }
                        Text("\(installedCount) installed").foregroundStyle(Palette.secondary).lineLimit(1).fixedSize()
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            if let close {
                Button(action: close) {
                    Image(systemName: "xmark")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 36, height: 36)
                        .background(Palette.bubbleAgent, in: Circle())
                        .frame(width: 44, height: 44)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .keyboardShortcut(.cancelAction)
                .accessibilityLabel("Close")
                .padding(.leading, 12)
            }
        }
        .padding(.top, 8)
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
        .padding(.horizontal, 16)
        .frame(height: 46)
        .background(Palette.bubbleAgent, in: Capsule())
    }

    private func agentCard(_ b: Backend) -> some View {
        AgentCard(backend: b) {
            var d = BotDraft(name: b.name)
            d.backend = b.id
            newBot = EditorRequest(d)
        }
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
    @Environment(BotStore.self) private var model

    var body: some View {
        List {
            if !model.installedConnectors.isEmpty {
                Section("Connectors") {
                    ForEach(model.installedConnectors) { c in
                        InstalledRow(title: c.name, subtitle: c.command ?? c.url ?? c.description) {
                            ServiceLogo(website: nil, name: c.name, registryName: c.registryName, size: 34)
                        } remove: {
                            Task { try? await model.removeConnector(c.id) }
                        }
                    }
                }
            }
            if !model.installedSkills.isEmpty {
                Section("Skills") {
                    ForEach(model.installedSkills) { s in
                        InstalledRow(title: s.name, subtitle: s.description) {
                            SkillGlyph().frame(width: 34, height: 34)
                        } remove: {
                            Task { try? await model.removeSkill(s.id) }
                        }
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle("Installed")
        .inlineNavigationTitle()
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
                Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                Text(subtitle).font(.caption).foregroundStyle(Palette.secondary).lineLimit(2)
            }
            Spacer()
            Button("Remove", role: .destructive, action: remove)
                .buttonStyle(.plain)
                .font(.subheadline)
                .foregroundStyle(Palette.danger)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Pieces

private struct MarketSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.title3.weight(.semibold)).foregroundStyle(Palette.text).padding(.leading, 4)
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

    var body: some View {
        Button(action: action) {
            VStack(spacing: 12) {
                AgentIcon(registry: backend.registry, size: 34)
                    .frame(width: 64, height: 64)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                VStack(spacing: 2) {
                    Text(backend.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
                    Text(backend.installed == true ? "Installed" : "Sets up on first use")
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 18)
            .padding(.horizontal, 10)
            .background(hovering ? Palette.bubbleAgent.opacity(0.6) : Palette.background, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Palette.border))
            .contentShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .buttonStyle(PressScale())
        .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hovering = h } }
        .help("New bot with \(backend.name)")
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
        .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hovering = h } }
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
        .overlay(shape.stroke(Palette.border.opacity(0.6)))
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

/// Slight shrink while pressed.
struct PressScale: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.7), value: configuration.isPressed)
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
        Form {
            Section {
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
                Section("Runs") {
                    Picker("Runs", selection: $optionId) {
                        ForEach(item.options) { o in
                            Text(o.kind == "remote" ? "Hosted by \(item.title)" : "On \(model.hostName) (\(o.kind))").tag(o.id)
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.inline)
                }
            }
            if let option {
                Section {
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
                } header: {
                    Text("Setup")
                } footer: {
                    Text("Keys are saved on \(model.hostName) only.")
                }
            }
            if let error {
                Section { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Add connector")
        .inlineNavigationTitle()
        .onAppear { optionId = item.options.first?.id ?? "" }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    ProgressView()
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
        Form {
            Section {
                TextField("Name", text: $name)
                Picker("Type", selection: $remote) {
                    Text("Command").tag(false)
                    Text("URL").tag(true)
                }
                .pickerStyle(.segmented)
                TextField(remote ? "https://example.com/mcp" : "npx -y @scope/server", text: $target)
                    .font(.callout.monospaced())
                    .plainTextInput()
            } footer: {
                Text(remote ? "A remote MCP server (streamable HTTP)." : "Runs on \(model.hostName) in the bot's project folder.")
            }
            if !remote {
                Section {
                    TextField("API_KEY=…", text: $envText, axis: .vertical)
                        .lineLimit(2...6)
                        .font(.callout.monospaced())
                        .plainTextInput()
                } header: {
                    Text("Environment")
                } footer: {
                    Text("One KEY=value per line. Saved on \(model.hostName) only.")
                }
            }
            if let error {
                Section { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Custom connector")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    ProgressView()
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
        Form {
            Section {
                TextField("Name", text: $name)
                TextField("When to use it", text: $summary, axis: .vertical).lineLimit(2...4)
            } footer: {
                Text("The bot sees the name and when to use it, and reads the instructions only when a task fits.")
            }
            Section("Instructions") {
                TextField("Step by step, in plain words…", text: $instructions, axis: .vertical)
                    .lineLimit(6...20)
            }
            if let error {
                Section { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("New skill")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    ProgressView()
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
        .foregroundStyle(Palette.text)
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
