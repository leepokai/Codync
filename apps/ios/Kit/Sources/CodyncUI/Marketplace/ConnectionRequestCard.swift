import AuthenticationServices
import CodyncKit
import SwiftUI

/// Credentials go through the setup API; only the request's state is a chat entry.
struct ConnectionRequestCard: View {
    let entry: Entry
    let request: ConnectionRequest
    @Environment(BotStore.self) private var model
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State private var app: ComposioApp?
    @State private var item: MarketConnector?
    @State private var value = ""
    @State private var username = ""
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(request.title, systemImage: "lock.shield")
                .font(.headline)
            if let text = entry.data.text { Text(text).foregroundStyle(Palette.secondary) }
            if request.status == "pending" {
                if request.kind == "login" {
                    TextField("Username or email", text: $username)
                        .plainTextInput()
                    SecureField("Password or op:// reference", text: $value)
                        .plainTextInput()
                    Text("Saved securely on this computer. The bot types it into the sign-in page but never sees it.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                } else if request.kind == "secret" {
                    Text("\(request.field ?? "Credential") · \(request.location ?? "")")
                        .font(.caption).foregroundStyle(Palette.secondary)
                    SecureField("Credential or op:// reference", text: $value)
                        .plainTextInput()
                    Text("Saved securely on this computer. Never added to the conversation.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
                HStack {
                    Button(request.kind == "login" ? "Save login" : request.kind == "secret" ? "Save and connect" : "Connect") { connect() }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(busy || (["secret", "login"].contains(request.kind) && value.isEmpty))
                    Button("Cancel") { cancel() }.buttonStyle(SecondaryButtonStyle()).disabled(busy)
                    if busy { Spinner() }
                }
            } else {
                Text(request.status == "ready" ? (request.kind == "login" ? "Saved" : "Connected") : "Cancelled").foregroundStyle(Palette.secondary)
            }
            if let error { Text(error).font(.footnote).foregroundStyle(Palette.danger) }
        }
        .padding(16)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 18))
        .textFieldStyle(.plain)
        .codyncSheet(item: $item) { item in
            InstallConnectorSheet(item: item, requestId: entry.id) { self.item = nil }
        }
        .codyncSheet(item: $app) { app in
            ComposioConnectSheet(app: app, requestId: entry.id) { self.app = nil }
        }
        .onDisappear { value = ""; username = "" }
    }

    private func connect() {
        guard let client = model.client else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                if request.kind == "app", let toolkit = request.toolkit {
                    withAnimation(Motion.layout) { app = ComposioApp(slug: toolkit, name: request.title) }
                } else if request.kind == "login" {
                    try await client.finishConnectionRequest(entry.id, value: value, username: username)
                    value = ""
                } else if request.kind == "secret" {
                    try await client.finishConnectionRequest(entry.id, value: value)
                    value = ""
                } else if let id = request.connectorId {
                    let connectors = try await client.connectors()
                    if connectors.first(where: { $0.id == id })?.needsSignIn == true {
                        try await ConnectorSignInFlow(model: model, webAuthenticationSession: webAuthenticationSession, openURL: openURL).signIn(id)
                    }
                    try await client.finishConnectionRequest(entry.id, connectorId: id)
                } else if let name = request.registryName {
                    let loaded = try await client.connectorInfo(name)
                    withAnimation(Motion.layout) { item = loaded }
                }
            } catch { self.error = error.localizedDescription }
        }
    }

    private func cancel() {
        guard let client = model.client else { return }
        busy = true
        Task {
            defer { busy = false }
            do { try await client.finishConnectionRequest(entry.id, cancel: true); value = "" }
            catch { self.error = error.localizedDescription }
        }
    }
}

struct CredentialsView: View {
    @Environment(BotStore.self) private var model
    @State private var selected: InstalledConnector?
    @State private var token = ""
    @State private var status: CredentialStatus?
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Credentials") {}
            CardForm {
                CardSection("Storage") {
                    Text(status?.provider ?? "System credential store")
                    Text("Encrypted on the connected computer.")
                        .foregroundStyle(Palette.secondary)
                }
                CardSection("Saved connections") {
                    ForEach(model.installedConnectors.filter { !$0.keys.isEmpty }) { connector in
                        Button { withAnimation(Motion.layout) { selected = connector } } label: {
                            HStack { Text(connector.name); Spacer(); Image(systemName: "chevron.right") }
                        }
                        .buttonStyle(.plain)
                    }
                    if !model.installedConnectors.contains(where: { !$0.keys.isEmpty }) {
                        Text("No saved connection credentials yet.")
                            .font(.caption).foregroundStyle(Palette.secondary)
                    }
                    Text("Values stay hidden. Select a connection to replace its credentials.")
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
                LoginsSection()
                CardSection("1Password", footer: "Use a service account with read access only to a dedicated shared vault. The 1Password CLI must be installed on the computer.") {
                    Text(status?.onePasswordConnected == true ? "Connected" : "Connect a shared vault")
                    CredentialFieldRow("Service account token") {
                        SecureField("Paste your token", text: $token).plainTextInput()
                    }
                    Button("Connect") { save(token) }.buttonStyle(PrimaryButtonStyle()).disabled(busy || token.isEmpty)
                    if status?.onePasswordConnected == true {
                        Button("Disconnect") { save("") }.buttonStyle(SecondaryButtonStyle()).disabled(busy)
                    }
                    Text("Use op://vault/item/field in a connector's credential field. Codync retrieves the value when the connector runs.")
                        .font(.footnote).foregroundStyle(Palette.secondary)
                }
                if busy { Spinner() }
                if let error { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .background(Palette.background)
        .textFieldStyle(.plain)
        .codyncSheet(item: $selected) { ConnectorCredentialEditor(connector: $0) }
        .task {
            await model.refreshPlugins()
            do { status = try await model.client?.credentialStatus() }
            catch { self.error = error.localizedDescription }
        }
        .onDisappear { token = "" }
    }

    private func save(_ value: String) {
        guard let client = model.client else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do { status = try await client.setOnePasswordToken(value); token = "" }
            catch { self.error = error.localizedDescription }
        }
    }
}

/// Website and app logins bots type through the computer tool; passwords stay hidden.
private struct LoginsSection: View {
    @Environment(BotStore.self) private var model
    @State private var logins: [SavedLogin] = []
    @State private var site = ""
    @State private var username = ""
    @State private var password = ""
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        CardSection("Sign-ins", footer: "Bots type these into the matching website or app and never see the password. You can use an op:// reference.") {
            ForEach(logins) { login in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(login.site)
                        if !login.username.isEmpty { Text(login.username).font(.caption).foregroundStyle(Palette.secondary) }
                    }
                    Spacer()
                    IconButton("Remove", systemImage: "trash") { remove(login) }
                }
            }
            CredentialFieldRow("Website or app") {
                TextField("github.com", text: $site).plainTextInput()
            }
            CredentialFieldRow("Username or email") {
                TextField("name@example.com", text: $username).plainTextInput()
            }
            CredentialFieldRow("Password") {
                SecureField("Password or op:// reference", text: $password).plainTextInput()
            }
            Button("Save sign-in") { save() }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(busy || site.isEmpty || password.isEmpty)
            if let error { Text(error).font(.footnote).foregroundStyle(Palette.danger) }
        }
        .task { await load() }
        .onDisappear { password = "" }
    }

    private func load() async {
        do { logins = try await model.client?.logins() ?? [] } catch { self.error = error.localizedDescription }
    }

    private func save() {
        guard let client = model.client else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                try await client.saveLogin(site: site, username: username, password: password)
                site = ""; username = ""; password = ""
                await load()
            } catch { self.error = error.localizedDescription }
        }
    }

    private func remove(_ login: SavedLogin) {
        guard let client = model.client else { return }
        Task {
            do {
                try await client.removeLogin(login.id)
                withAnimation(Motion.layout) { logins.removeAll { $0.id == login.id } }
            } catch { self.error = error.localizedDescription }
        }
    }
}

private struct ConnectorCredentialEditor: View {
    let connector: InstalledConnector
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var fields: [String: String] = [:]
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(connector.name) {
                if saving { Spinner() }
                else { IconButton("Save credentials", systemImage: "checkmark") { save() }.disabled(fields.values.allSatisfy(\.isEmpty)) }
            }
            CardForm {
                CardSection(footer: "Enter only the values you want to replace. You can use an op:// reference.") {
                    ForEach(connector.keys, id: \.self) { key in
                        SecureField(key, text: Binding(get: { fields[key] ?? "" }, set: { fields[key] = $0 })).plainTextInput()
                    }
                }
                if let error { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .background(Palette.background)
        .textFieldStyle(.plain)
        .onDisappear { fields.removeAll() }
    }
    private func save() {
        guard let client = model.client else { return }
        saving = true
        error = nil
        Task {
            defer { saving = false }
            do {
                try await client.updateConnectorCredentials(connector.id, fields: fields.filter { !$0.value.isEmpty })
                fields.removeAll()
                dismiss()
            } catch { self.error = error.localizedDescription }
        }
    }
}

private struct CredentialFieldRow<Content: View>: View {
    let title: String
    let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.caption).foregroundStyle(Palette.secondary)
            content.accessibilityLabel(title)
        }
    }
}
