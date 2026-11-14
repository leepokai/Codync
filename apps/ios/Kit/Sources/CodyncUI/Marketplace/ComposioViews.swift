import CodyncKit
import SwiftUI

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

/// Connects one app: Composio's sign-in page (opened here, finished anywhere),
/// or a form for apps that use a key.
struct ComposioConnectSheet: View {
    let app: ComposioApp
    var requestId: String? = nil
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
                    Label(L10n.tr("Connected"), systemImage: "checkmark.circle.fill").foregroundStyle(Palette.added)
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
                        Button(L10n.tr("Open the sign-in page"), systemImage: "safari") { openURL(url) }
                            .buttonStyle(.secondary)
                    }
                    HStack(spacing: 8) {
                        Spinner()
                        Text(L10n.tr("Waiting for you to finish signing in…")).foregroundStyle(Palette.secondary)
                    }
                }
            } else if error == nil {
                CardSection {
                    HStack(spacing: 8) {
                        Spinner()
                        Text(L10n.tr("Asking Composio…")).foregroundStyle(Palette.secondary)
                    }
                }
            }
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
            }
        }
        .textFieldStyle(.plain)
        .task { await start() }
        .onDisappear { values.removeAll(); if connected { done() } }
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
                if let requestId {
                    do { try await client.finishConnectionRequest(requestId, connectorId: "composio-\(app.slug)") }
                    catch { self.error = error.localizedDescription; return }
                }
                values.removeAll()
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
                    if let requestId { try await client.finishConnectionRequest(requestId, connectorId: "composio-\(app.slug)") }
                    values.removeAll()
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
