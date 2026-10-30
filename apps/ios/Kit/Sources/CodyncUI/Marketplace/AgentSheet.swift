import CodyncKit
import SwiftUI

/// Above the agents while some of them run on Node.js and the computer has none: one place
/// to add Codync's copy for all of them.
struct NodeNotice: View {
    let add: () -> Void
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "shippingbox")
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .padding(.top, 1)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr("Node.js isn't on {name}").replacingOccurrences(of: "{name}", with: model.hostName)).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text)
                    Text(L10n.tr("Some agents need it to run. Codync can add a copy just for them."))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                }
                // Compact on a phone: under the text, so the text keeps the row's width.
                Button(L10n.tr("Add Node.js"), action: add)
                    .buttonStyle(.plain)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.onAccent)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Palette.accentFill, in: Capsule())
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(Palette.bubbleAgent.opacity(0.45), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .transition(.opacity)
    }
}

/// Getting an agent ready on the computer: install it, add Node.js when it runs on Node and
/// the computer has none, then sign in. Sign-in options come from the agent itself (ACP),
/// plus Codync's own command for CLIs it knows. Bots are made from New bot, not here.
struct AgentSheet: View {
    let initial: Backend
    /// Opened from a chat's sign-in request, whose agent refused a turn: the agent is asked
    /// again however signed in it looked.
    var recheck = false
    @Environment(BotStore.self) private var model
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
    /// It has run on this computer: its CLI is there, or an earlier check started it. Until then, a check
    /// can download it, so only Set up starts one.
    private var setUp: Bool { installed || backend.signedIn != nil || auth != nil }

    var body: some View {
        ZStack {
            if let route {
                routed(route).transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                VStack(spacing: 0) {
                    ModalHeader(backend.name)
                    overview
                }
                .transition(.move(edge: .leading).combined(with: .opacity))
            }
        }
        .frame(minWidth: 0, minHeight: 0)
        // A known "signed in" needs no agent start; one set up asks the agent (which can't start
        // without its Node.js). Opening never downloads one: that waits for Set up.
        .task { if recheck || (setUp && backend.signedIn != true), !backend.needsNode { await check() } }
        .onChange(of: route) { old, new in
            // Back from a terminal or key form: see what changed.
            if new == nil, old != nil { Task { await check() } }
        }
    }

    @ViewBuilder private func routed(_ route: SetupRoute) -> some View {
        let back = { withAnimation(Motion.layout) { self.route = nil } }
        switch route {
        case .install: SetupTerminalView(backend: backend, step: .install, back: back)
        case .node: SetupTerminalView(backend: backend, step: .node, back: back)
        case .terminal(let method): SetupTerminalView(backend: backend, step: .login, method: method, back: back)
        case .keys(let method): AgentKeysForm(backend: backend, method: method, saved: auth?.savedEnv ?? [], back: back) { auth = $0 }
        }
    }

    private func go(_ route: SetupRoute) {
        withAnimation(Motion.layout) { self.route = route }
    }

    private var overview: some View {
        CardForm {
            CardSection {
                CustomIconEditor(target: "agent:\(backend.id)")
                HStack(spacing: 14) {
                    AgentIcon(registry: backend.registry ?? backend.id, size: 30)
                        .frame(width: 52, height: 52)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(backend.name).font(.title3.weight(.semibold))
                        if let d = backend.description, !d.isEmpty {
                            Text(L10n.tr(d)).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
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
                        action: installed || backend.canInstall != true ? nil : ("Install", { go(.install) })
                    )
                }
            }
            if backend.node != nil {
                CardSection {
                    SetupStepRow(
                        number: curated ? 2 : 1,
                        title: "Node.js",
                        detail: backend.needsNode
                            ? "\(backend.name) runs on Node.js. Codync adds its own copy."
                            : "Added on \(model.hostName).",
                        done: !backend.needsNode,
                        action: backend.needsNode ? ("Add", { go(.node) }) : nil
                    )
                }
            }
            signInSection
            AgentBrings(backendId: backend.id)
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger).textSelection(.enabled) }
            }
        }
    }

    private var signInSection: some View {
        CardSection(footer: signedIn != true && auth?.methods.contains(where: { $0.kind == .agent }) == true
            ? "Browser sign-ins open on \(model.hostName) itself. Away from it? Use Remote screen to finish there."
            : nil) {
            HStack(spacing: 0) {
                SetupStepRow(
                    number: 1 + (curated ? 1 : 0) + (backend.node != nil ? 1 : 0),
                    title: "Sign in",
                    detail: statusText,
                    done: signedIn == true,
                    action: nil
                )
                if checking {
                    ThinkingOrb(state: .connecting, size: 16, color: Palette.secondary)
                } else {
                    Button(L10n.tr("Check again"), systemImage: "arrow.clockwise") { Task { await check() } }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.secondary)
                        .help("Check again")
                }
            }
            if signedIn != true, let detail = auth?.detail, !detail.isEmpty {
                Text(detail).font(.footnote).foregroundStyle(Palette.secondary).textSelection(.enabled)
            }
            // A registry agent downloads on its first start; an app's own CLI comes from Install above.
            if !setUp, !curated, !checking, !backend.needsNode {
                optionRow("Set up", detail: "Downloads \(backend.name) on \(model.hostName) if it needs to, then checks its sign-in.",
                          icon: "arrow.down.circle") {
                    Task { await check() }
                }
            }
            // Some CLIs drop the current sign-in the moment a new one starts: no options once signed in.
            if signedIn != true, !checking {
                if auth?.login == true {
                    optionRow("Sign in", detail: "In a terminal on \(model.hostName). Links open here.", icon: "terminal") {
                        go(.terminal(nil))
                    }
                }
                ForEach(auth?.methods ?? []) { m in
                    switch m.kind {
                    case .terminal?:
                        optionRow(m.name, detail: m.description ?? "In a terminal on \(model.hostName).", icon: "terminal") {
                            go(.terminal(m))
                        }
                    case .envVar?:
                        optionRow(m.name, detail: m.description ?? "Saved on \(model.hostName) only.", icon: "key") {
                            go(.keys(m))
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
        if !setUp, !backend.needsNode { return curated ? "Install it first." : "Not set up on \(model.hostName) yet." }
        switch signedIn {
        case true?: return "Signed in."
        case _ where backend.needsNode: return "Add Node.js first."
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
                    Text(L10n.tr(title)).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                    Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(3)
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
    case node
    case terminal(AuthMethod?)
    case keys(AuthMethod)
}

/// Keys an agent reads from its environment, kept on the computer.
private struct AgentKeysForm: View {
    let backend: Backend
    let method: AuthMethod
    let saved: [String]
    let back: () -> Void
    let done: (AgentAuth) -> Void
    @Environment(BotStore.self) private var model
    @State private var values: [String: String] = [:]
    @State private var saving = false
    @State private var error: String?

    private var vars: [AuthMethod.Var] { method.vars ?? [] }

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                BackButton(action: back).keyboardShortcut(.cancelAction)
            } title: {
                Text(L10n.tr(method.name)).font(.body.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            } trailing: {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") { save(values.filter { !$0.value.isEmpty }) }
                        .disabled(vars.contains { !$0.optional && (values[$0.name] ?? "").isEmpty && !saved.contains($0.name) })
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var form: some View {
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
                    Text(L10n.tr("Saved on \(model.hostName) only and given to \(backend.name) when it starts."))
                    if let link = method.link, let url = URL(string: link) {
                        WebLink("Get a key", url: url)
                    }
                }
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .padding(.horizontal, 4)
            }
            if !saved.isEmpty {
                CardSection {
                    Button(L10n.tr("Remove saved keys"), role: .destructive) {
                        save(Dictionary(uniqueKeysWithValues: vars.map { ($0.name, "") }))
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.danger)
                }
            }
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
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
                back()
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
                    Text(L10n.tr("\(number)")).font(.caption.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
            }
            .frame(width: 26, height: 26)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr(title)).font(.body.weight(.medium)).foregroundStyle(Palette.text)
                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary).textSelection(.enabled)
            }
            Spacer(minLength: 8)
            if let action {
                Button(L10n.tr(action.label), action: action.run)
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
