import CodyncKit
import SwiftUI

// Apps through Composio (host/src/composio.rs): set up a key once, connect
// apps with Composio's hosted sign-in, then turn them on per bot like any
// other connector.

/// The Marketplace's "Apps" section.
struct ComposioSection: View {
    let query: String
    /// Bumped by the Marketplace when the user submits a search.
    let searchToken: Int
    @Environment(BotStore.self) private var model
    @State private var status: ComposioStatus?
    @State private var apps: [ComposioApp] = []
    @State private var loading = true
    @State private var error: String?
    @State private var settingUp = false
    @State private var connecting: ComposioApp?

    var body: some View {
        MarketSection(title: "Apps") {
            VStack(alignment: .leading, spacing: 10) {
                if status?.configured != true {
                    ItemGrid {
                        MarketRow(
                            title: "Composio",
                            subtitle: "Gmail, Slack, GitHub, Notion and 1,000 more apps",
                            added: false,
                            busy: status == nil && loading,
                            addLabel: "Set up"
                        ) {
                            TileIcon(systemName: "square.grid.3x3")
                        } add: {
                            withAnimation(Motion.layout) { settingUp = true }
                        }
                    }
                } else if loading {
                    SkeletonGrid()
                } else if apps.isEmpty {
                    Text(query.isEmpty ? "Couldn't load apps." : "No apps match “\(query)”.")
                        .foregroundStyle(Palette.secondary)
                } else {
                    ItemGrid {
                        ForEach(apps) { app in
                            MarketRow(
                                title: app.name,
                                subtitle: app.description ?? app.slug,
                                added: app.connected,
                                addLabel: "Connect"
                            ) {
                                AppLogo(url: app.logo, name: app.name)
                            } add: {
                                withAnimation(Motion.layout) { connecting = app }
                            }
                        }
                    }
                }
                if let error {
                    Text(error).font(.footnote).foregroundStyle(Palette.danger)
                }
                if status?.configured == true {
                    Button("Composio settings", systemImage: "key") { withAnimation(Motion.layout) { settingUp = true } }
                        .labelStyle(.titleAndIcon)
                        .buttonStyle(.plain)
                        .font(.footnote)
                        .foregroundStyle(Palette.tertiary)
                }
            }
        }
        .task(id: searchToken) { await load() }
        .codyncSheet(isPresented: $settingUp) {
            ComposioKeySheet(status: status) { new in
                status = new
                Task { await load() }
            }
        }
        .codyncSheet(item: $connecting) { app in
            ComposioConnectSheet(app: app) { Task { await load(); await model.refreshPlugins() } }
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        loading = true
        defer { loading = false }
        do {
            let s = try await client.composioStatus()
            status = s
            apps = s.configured ? try await client.composioApps(search: query) : []
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// An app's logo from Composio, a letter tile until it loads.
struct AppLogo: View {
    let url: String?
    let name: String
    var size: CGFloat = 46

    var body: some View {
        AsyncImage(url: url.flatMap(URL.init(string:))) { image in
            image.resizable().scaledToFit().padding(size * 0.18)
        } placeholder: {
            Text(name.prefix(1).uppercased())
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(Palette.secondary)
        }
        .frame(width: size, height: size)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous))
        .accessibilityHidden(true)
    }
}

/// Paste (or remove) the Composio API key; the host checks it with Composio first.
struct ComposioKeySheet: View {
    let status: ComposioStatus?
    let done: (ComposioStatus) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var key = ""
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Composio") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") { save(key) }.disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var form: some View {
        CardForm {
            CardSection(
                "Composio API key",
                footer: "Composio signs you in to apps like Gmail and Slack and runs their tools for your bots. The key stays on \(model.hostName)."
            ) {
                SecureField("API key", text: $key, prompt: Text(status?.configured == true ? "Saved (paste to replace)" : "ak_…"))
                    .plainTextInput()
                if let url = URL(string: status?.keyUrl ?? "https://platform.composio.dev") {
                    WebLink("Get a key from Composio", url: url).font(.footnote)
                }
            }
            if status?.configured == true {
                CardSection(footer: "Bots lose the apps you connected until you add a key again.") {
                    Button("Remove key", role: .destructive) { save("") }
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.danger)
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
    }

    private func save(_ key: String) {
        guard let client = model.client else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                done(try await client.setComposioKey(key))
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

/// Connects one app: Composio's sign-in page (opened here, finished anywhere),
/// or a form for apps that use a key.
struct ComposioConnectSheet: View {
    let app: ComposioApp
    let done: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var plan: ComposioConnect?
    @State private var values: [String: String] = [:]
    @State private var connected = false
    @State private var waiting = false
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Connect \(app.name)") {
                if connected {
                    IconButton("Done", systemImage: "checkmark") { dismiss() }
                } else if plan?.status == "needsFields" {
                    if saving {
                        Spinner()
                    } else {
                        IconButton("Connect", systemImage: "checkmark") { submit() }
                            .disabled(plan?.fields?.contains { $0.required && (values[$0.name] ?? "").isEmpty } ?? true)
                    }
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var form: some View {
        CardForm {
            CardSection {
                HStack(spacing: 14) {
                    AppLogo(url: app.logo, name: app.name, size: 52)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(app.name).font(.title3.weight(.semibold))
                        if let d = app.description {
                            Text(d).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
            if connected {
                CardSection(footer: "Turn \(app.name) on for a bot in the bot's settings, under Connectors.") {
                    Label("Connected", systemImage: "checkmark.circle.fill").foregroundStyle(Palette.added)
                }
            } else if let plan, plan.status == "needsFields" {
                CardSection(footer: "Sent to Composio, which keeps it for \(app.name).") {
                    ForEach(plan.fields ?? [], id: \.name) { f in
                        VStack(alignment: .leading, spacing: 4) {
                            Group {
                                if f.secret {
                                    SecureField(f.label, text: binding(f.name))
                                } else {
                                    TextField(f.label, text: binding(f.name))
                                }
                            }
                            .plainTextInput()
                            if let d = f.description {
                                Text(d + (f.required ? "" : " (optional)")).font(.caption).foregroundStyle(Palette.secondary)
                            }
                        }
                    }
                }
            } else if let plan, plan.status == "redirect" {
                CardSection(footer: "Sign in on the page that opened. This updates by itself when you're done.") {
                    if let url = plan.url.flatMap(URL.init(string:)) {
                        Button("Open the sign-in page", systemImage: "safari") { openURL(url) }
                            .buttonStyle(.secondary)
                    }
                    HStack(spacing: 8) {
                        Spinner()
                        Text("Waiting for you to finish signing in…").foregroundStyle(Palette.secondary)
                    }
                }
            } else if error == nil {
                CardSection {
                    HStack(spacing: 8) {
                        Spinner()
                        Text("Asking Composio…").foregroundStyle(Palette.secondary)
                    }
                }
            }
            if let error {
                CardSection { Text(error).foregroundStyle(Palette.danger).textSelection(.enabled) }
            }
        }
        .textFieldStyle(.plain)
        .task { await start() }
        .onDisappear { if connected { done() } }
    }

    private func binding(_ name: String) -> Binding<String> {
        Binding(get: { values[name] ?? "" }, set: { values[name] = $0 })
    }

    private func start() async {
        guard let client = model.client else { return }
        do {
            let plan = try await client.composioConnect(app.slug)
            self.plan = plan
            guard plan.status == "redirect", let id = plan.connection else { return }
            if let url = plan.url.flatMap(URL.init(string:)) { openURL(url) }
            await wait(for: id, client: client)
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Polls until the sign-in finishes (or the sheet goes away).
    private func wait(for id: String, client: HostClient) async {
        waiting = true
        defer { waiting = false }
        for _ in 0..<300 {
            try? await Task.sleep(for: .seconds(2))
            if Task.isCancelled { return }
            guard let state = try? await client.composioConnection(id) else { continue }
            switch state.status {
            case "active":
                connected = true
                return
            case "failed", "expired":
                error = "Signing in to \(app.name) didn't work (\(state.status)). Close and try again."
                return
            default:
                continue
            }
        }
        error = "Timed out waiting for the sign-in."
    }

    private func submit() {
        guard let client = model.client, let mode = plan?.mode else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                let state = try await client.composioConnect(app.slug, mode: mode, fields: values.filter { !$0.value.isEmpty })
                if state.status == "active" {
                    connected = true
                } else {
                    error = "\(app.name) says the connection is \(state.status)."
                }
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}
