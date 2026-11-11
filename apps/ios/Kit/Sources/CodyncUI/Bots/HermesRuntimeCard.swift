import CodyncKit
import Foundation
import SwiftUI

private struct HermesRuntimeConfig: Codable {
    var enabled = false
    var root = ""
    var python = ""
    var terminal = Terminal()
    var memoryProvider = "native"
    var delegation = Delegation()

    struct Terminal: Codable {
        var backend = ""
        var ssh_host = ""
        var ssh_user = ""
        var ssh_port: Int?
        var ssh_key = ""
        var docker_image = ""
        var modal_mode = ""
        var vercel_runtime = ""
    }
    struct Delegation: Codable {
        var routes: [Route] = []
        var provider = ""
        var model = ""
        var max_iterations: Int?
        var max_concurrent_children: Int?
        var child_timeout_seconds: Int?
        enum CodingKeys: String, CodingKey { case routes, provider, model, max_iterations, max_concurrent_children, child_timeout_seconds }
        init() {}
        init(from decoder: Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            routes = try values.decodeIfPresent([Route].self, forKey: .routes) ?? []
            provider = try values.decodeIfPresent(String.self, forKey: .provider) ?? ""
            model = try values.decodeIfPresent(String.self, forKey: .model) ?? ""
            max_iterations = try values.decodeIfPresent(Int.self, forKey: .max_iterations)
            max_concurrent_children = try values.decodeIfPresent(Int.self, forKey: .max_concurrent_children)
            child_timeout_seconds = try values.decodeIfPresent(Int.self, forKey: .child_timeout_seconds)
        }
    }
    struct Route: Codable, Identifiable {
        var id = UUID().uuidString
        var label = ""
        var provider = ""
        var model = ""
    }
}

private struct HermesRuntimeReply: Decodable {
    var revision: String?
    var config: HermesRuntimeConfig
    var installation: Installation?
    struct Installation: Decodable {
        var available: Bool
        var error: String?
        var profileSummary: String?
    }
}

struct HermesRuntimeCard: View {
    let botId: String
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var config = HermesRuntimeConfig()
    @State private var revision: String?
    @State private var loaded = false
    @State private var busy = false
    @State private var error: String?
    @State private var notice: String?
    @State private var advanced = false
    @State private var port = ""
    @State private var iterations = ""
    @State private var concurrency = ""
    @State private var inactivity = ""
    @State private var loadedComputer: String?
    @State private var loadEpoch = UUID()
    @State private var savedEnabled = false

    var body: some View {
        CardSection("Hermes runtime") {
            VStack(alignment: .leading, spacing: 12) {
                Text(L10n.tr("Keep Hermes connections and choose this bot’s environment, memory and subagent route together."))
                    .font(.caption).foregroundStyle(Palette.secondary)
                SwitchRow("Use managed Hermes runtime", isOn: $config.enabled)
                if config.enabled {
                    ValueRow("Environment") {
                        ChoicePicker(selection: $config.terminal.backend, options: [
                            ("", "From Hermes profile"), ("local", "Local"), ("docker", "Docker"),
                            ("ssh", "SSH"), ("modal", "Modal"), ("daytona", "Daytona"), ("vercel_sandbox", "Vercel Sandbox"),
                        ])
                    }
                    if config.terminal.backend == "ssh" {
                        input("SSH host or alias", $config.terminal.ssh_host)
                        input("SSH user", $config.terminal.ssh_user)
                        input("SSH port (blank uses Hermes default)", $port)
                        input("SSH key path (blank uses SSH configuration)", $config.terminal.ssh_key)
                    }
                    if config.terminal.backend == "docker" {
                        input("Docker image (blank uses Hermes profile)", $config.terminal.docker_image)
                    }
                    if config.terminal.backend == "modal" {
                        ValueRow("Modal mode") {
                            ChoicePicker(selection: $config.terminal.modal_mode, options: [("", "From Hermes profile"), ("auto", "Auto"), ("managed", "Managed"), ("direct", "Direct")])
                        }
                    }
                    if config.terminal.backend == "vercel_sandbox" {
                        ValueRow("Vercel runtime") {
                            ChoicePicker(selection: $config.terminal.vercel_runtime, options: [("", "From Hermes profile"), ("node24", "Node 24"), ("node22", "Node 22"), ("python3.13", "Python 3.13")])
                        }
                    }
                    ValueRow("Memory") {
                        ChoicePicker(selection: $config.memoryProvider, options: [
                            ("native", "From Hermes profile"), ("builtin", "Built-in"), ("honcho", "Honcho"),
                        ])
                    }
                    Text(L10n.tr("Honcho uses its existing Hermes profile and credentials. It must already be configured."))
                        .font(.caption).foregroundStyle(Palette.secondary)
                    input("Subagent provider (blank inherits)", $config.delegation.provider)
                    input("Subagent model (blank inherits)", $config.delegation.model)
                    input("Iterations per child (1–200)", $iterations)
                    input("Concurrent children (1–10)", $concurrency)
                    input("Child inactivity timeout, seconds (30–3600)", $inactivity)
                    Text(L10n.tr("Saved task routes let the agent choose a provider and model per task. Each child keeps the limits above."))
                        .font(.caption).foregroundStyle(Palette.secondary)
                    ForEach(config.delegation.routes.map(\.id), id: \.self) { id in
                        VStack(alignment: .leading, spacing: 8) {
                            input("Route name", routeText(id, \.label))
                            input("Route provider", routeText(id, \.provider))
                            input("Route model", routeText(id, \.model))
                            Button(L10n.tr("Remove route")) {
                                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { config.delegation.routes.removeAll { $0.id == id } }
                            }.buttonStyle(.secondary)
                        }
                    }
                    if config.delegation.routes.count < 16 {
                        Button(L10n.tr("Add task route")) {
                            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { config.delegation.routes.append(.init()) }
                        }.buttonStyle(.secondary)
                    }
                    Button(L10n.tr(advanced ? "Hide installation options" : "Installation options")) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { advanced.toggle() }
                    }.buttonStyle(.secondary)
                    if advanced {
                        input("Hermes source directory (blank detects installation)", $config.root)
                        input("Hermes Python interpreter (blank detects environment)", $config.python)
                    }
                }
                if let error { Text(error).font(.caption).foregroundStyle(Palette.danger) }
                if let notice { Text(notice).font(.caption).foregroundStyle(Palette.secondary) }
                HStack {
                    Button(L10n.tr("Check runtime")) { Task { await check() } }.buttonStyle(.secondary)
                    Spacer()
                    Button(L10n.tr("Apply")) { Task { await apply() } }.buttonStyle(.primary)
                }
                Button(L10n.tr("Check saved environment (may start sandbox)")) { Task { await checkEnvironment() } }
                    .buttonStyle(.secondary).disabled(!savedEnabled)
                Text(L10n.tr("Applies after the current turn. Installation checks do not establish remote backend connectivity."))
                    .font(.caption).foregroundStyle(Palette.secondary)
            }.disabled(!loaded || busy || model.connection != .online)
            if !loaded {
                if error != nil {
                    Button(L10n.tr("Retry")) { Task { await load() } }
                        .buttonStyle(.secondary).disabled(busy || model.connection != .online)
                } else {
                    HStack { Spinner(); Text(L10n.tr("Loading Hermes settings…")) }
                }
            }
        }.task(id: botId) { await load() }
            .onChange(of: model.computer.id) { _, _ in Task { await load() } }
    }

    private func input(_ title: String, _ binding: Binding<String>) -> some View {
        Field(title) { TextField(L10n.tr(title), text: binding).autocorrectionDisabled().fieldBox() }
    }

    private func routeText(_ id: String, _ path: WritableKeyPath<HermesRuntimeConfig.Route, String>) -> Binding<String> {
        Binding(get: { config.delegation.routes.first { $0.id == id }?[keyPath: path] ?? "" }, set: { value in
            guard let index = config.delegation.routes.firstIndex(where: { $0.id == id }) else { return }
            config.delegation.routes[index][keyPath: path] = value
        })
    }

    private func adopt(_ reply: HermesRuntimeReply) {
        config = reply.config
        savedEnabled = reply.config.enabled
        revision = reply.revision
        port = reply.config.terminal.ssh_port.map(String.init) ?? ""
        iterations = reply.config.delegation.max_iterations.map(String.init) ?? ""
        concurrency = reply.config.delegation.max_concurrent_children.map(String.init) ?? ""
        inactivity = reply.config.delegation.child_timeout_seconds.map(String.init) ?? ""
    }

    private func load() async {
        struct Body: Encodable { let botId: String }
        let epoch = UUID()
        loadEpoch = epoch
        loaded = false
        busy = false
        loadedComputer = nil
        savedEnabled = false
        notice = nil
        error = nil
        let route = model.computer.id
        do {
            let client = try await model.ready()
            guard route == model.computer.id, epoch == loadEpoch else { return }
            let reply: HermesRuntimeReply = try await client.call("hermesRuntime", Body(botId: botId))
            try Task.checkCancellation()
            guard route == model.computer.id, epoch == loadEpoch else { return }
            adopt(reply)
            loadedComputer = route
            loaded = true
            error = nil
        } catch is CancellationError {
            return
        } catch {
            guard route == model.computer.id, epoch == loadEpoch else { return }
            self.error = "\(error.localizedDescription) Update the host if Hermes runtime settings are unavailable."
        }
    }

    private func number(_ value: String, _ range: ClosedRange<Int>, _ name: String) throws -> Int? {
        let text = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { return nil }
        guard let number = Int(text), range.contains(number) else {
            throw NSError(domain: "HermesRuntime", code: 1, userInfo: [NSLocalizedDescriptionKey: "\(name) must be \(range.lowerBound)–\(range.upperBound)."])
        }
        return number
    }

    private func apply() async {
        struct Body: Encodable { let botId: String; let config: HermesRuntimeConfig; let ifRevision: String? }
        guard loaded, !busy, loadedComputer == model.computer.id else { return }
        let route = loadedComputer
        let epoch = loadEpoch
        busy = true
        defer { if route == model.computer.id && epoch == loadEpoch { busy = false } }
        do {
            var next = config
            next.terminal.ssh_port = try number(port, 1...65535, "SSH port")
            next.delegation.max_iterations = try number(iterations, 1...200, "Iterations")
            next.delegation.max_concurrent_children = try number(concurrency, 1...10, "Concurrency")
            next.delegation.child_timeout_seconds = try number(inactivity, 30...3600, "Inactivity timeout")
            let client = try await model.ready()
            guard route == model.computer.id, epoch == loadEpoch else { return }
            let reply: HermesRuntimeReply = try await client.call("setHermesRuntime", Body(botId: botId, config: next, ifRevision: revision))
            guard route == model.computer.id, epoch == loadEpoch else { return }
            adopt(reply)
            error = nil
            notice = L10n.tr("Saved. Applies after the current turn.")
        } catch { if route == model.computer.id && epoch == loadEpoch { self.error = error.localizedDescription } }
    }

    private func check() async {
        struct Body: Encodable { let botId: String }
        guard loaded, !busy, loadedComputer == model.computer.id else { return }
        let route = loadedComputer
        let epoch = loadEpoch
        busy = true
        defer { if route == model.computer.id && epoch == loadEpoch { busy = false } }
        do {
            let client = try await model.ready()
            guard route == model.computer.id, epoch == loadEpoch else { return }
            let reply: HermesRuntimeReply = try await client.call("hermesRuntimeStatus", Body(botId: botId), timeout: 30)
            guard route == model.computer.id, epoch == loadEpoch else { return }
            error = nil
            notice = reply.installation?.available == true
                ? reply.installation?.profileSummary ?? L10n.tr("Pinned Hermes installation found.")
                : reply.installation?.error ?? L10n.tr("Hermes installation is unavailable.")
        } catch { if route == model.computer.id && epoch == loadEpoch { self.error = error.localizedDescription } }
    }

    private func checkEnvironment() async {
        struct Body: Encodable { let botId: String; let ifRevision: String? }
        struct Health: Decodable { let backend: String; let status: String; let detail: String }
        guard loaded, savedEnabled, !busy, loadedComputer == model.computer.id else { return }
        let route = loadedComputer
        let epoch = loadEpoch
        let expected = revision
        busy = true
        defer { if route == model.computer.id && epoch == loadEpoch { busy = false } }
        do {
            let client = try await model.ready()
            guard route == model.computer.id, epoch == loadEpoch else { return }
            let reply: Health = try await client.call("hermesEnvironmentHealth", Body(botId: botId, ifRevision: expected), timeout: 65)
            guard route == model.computer.id, epoch == loadEpoch else { return }
            error = nil
            notice = "\(reply.backend): \(reply.status). \(reply.detail)"
        } catch { if route == model.computer.id && epoch == loadEpoch { self.error = error.localizedDescription } }
    }
}
