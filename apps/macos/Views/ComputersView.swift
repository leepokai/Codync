import AppKit
import CodyncKit
import CodyncUI
import SwiftUI
import UniformTypeIdentifiers

/// A computer's name over its bots in the sidebar, with how it's reached.
struct ComputerHeader: View {
    let store: BotStore
    let ssh: Bool

    var body: some View {
        HStack(spacing: 6) {
            ComputerBadge(store.computer, size: 16)
            Text(store.hostName).font(.system(size: 11, weight: .semibold)).lineLimit(1)
            RouteLabel(store: store, ssh: ssh)
            Spacer()
            if store.connection != .online {
                Text(store.statusText).font(.system(size: 11)).foregroundStyle(store.isOffline ? Palette.warning : Palette.tertiary)
            }
        }
        .foregroundStyle(Palette.secondary)
        .padding(.horizontal, 8)
        .padding(.top, 10)
        .padding(.bottom, 2)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// How this Mac reaches a computer: itself, an SSH tunnel, direct LAN or the encrypted relay.
struct RouteLabel: View {
    let store: BotStore
    let ssh: Bool

    var body: some View {
        if ssh {
            Image(systemName: "terminal").help("Over SSH").accessibilityLabel("Over SSH")
        } else if store.connection == .online {
            RouteIcon(route: store.hostRoute).help(store.hostRoute == .relay ? "Through the encrypted relay" : "Direct connection")
        }
    }
}

// MARK: - Approval

/// A device asks for access through the account (spec §4.2 B). The code is the only thing
/// that tells a real request from a cloud swapping keys, so it's big and Approve is never the default.
struct ApprovalSheet: View {
    let approval: Approval
    @Environment(HostController.self) private var host
    @State private var busy = false
    @State private var error: String?

    private var request: AccessRequest { approval.request }

    var body: some View {
        VStack(spacing: 16) {
            HStack {
                Spacer()
                IconButton("Decide later", systemImage: "xmark") { host.deferApproval(approval) }
            }
            Image(systemName: request.platform == "macos" ? "laptopcomputer" : "iphone")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(Palette.secondary)
                .accessibilityHidden(true)
            VStack(spacing: 4) {
                Text("\(request.deviceName) wants to use \(approval.store.hostName)")
                    .font(.title3.weight(.semibold))
                    .multilineTextAlignment(.center)
                if let email = request.email {
                    Text("Signed in as \(email)").font(.callout).foregroundStyle(Palette.secondary)
                }
            }
            if let code = request.code {
                Text(Self.spaced(code))
                    .font(.system(size: 48, weight: .semibold, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .textSelection(.enabled)
                    .accessibilityLabel("Code \(code.map(String.init).joined(separator: " "))")
                Text("Approve only if \(request.deviceName) shows exactly this code. If it doesn't, deny: someone may be trying to get in.")
                    .font(.callout)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                ProgressView().controlSize(.small)
                Text("Waiting for \(request.deviceName) to show its code…")
                    .font(.callout)
                    .foregroundStyle(Palette.secondary)
            }
            if let error {
                Text(error).font(.caption).foregroundStyle(Palette.danger).multilineTextAlignment(.center)
            }
            HStack(spacing: 12) {
                Button("Deny", role: .destructive) { decide(false) }
                    .disabled(busy)
                Button("Approve") { decide(true) }
                    .buttonStyle(.borderedProminent)
                    .tint(Palette.accentFill)
                    .disabled(busy || request.code == nil)
            }
            .controlSize(.large)
        }
        .padding(24)
        .frame(width: 420)
    }

    private func decide(_ approve: Bool) {
        busy = true
        Task {
            defer { busy = false }
            do {
                try await host.decide(approval, approve: approve)
                // The host's accessRequests event removes it; don't wait for that to close.
                host.deferApproval(approval)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private static func spaced(_ code: String) -> String {
        code.count == 6 ? "\(code.prefix(3)) \(code.suffix(3))" : code
    }
}

// MARK: - Computers & devices

/// Computers this Mac manages (itself and SSH), with their account, relay and authorized devices;
/// SSH profiles; and the account's other computers.
struct ComputersView: View {
    let close: () -> Void
    @Environment(HostController.self) private var host
    @Environment(AccountSession.self) private var account
    @State private var editingSSH: SSHProfile?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Computers & devices").font(.title2.bold())
                Spacer()
                Button("Done", action: close).keyboardShortcut(.cancelAction)
            }
            .padding(20)
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    ForEach(host.managedStores, id: \.computer.id) { store in
                        ManagedComputerCard(store: store, ssh: host.isSSH(store.computer.id))
                    }
                    sshSection
                    accountSection
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 20)
            }
        }
        .background(Palette.background)
        .sheet(item: $editingSSH) { profile in
            SSHProfileEditor(profile: profile, isNew: !host.ssh.profiles.contains { $0.id == profile.id }) { editingSSH = nil }
        }
    }

    private var sshSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SectionTitle("Over SSH")
                Spacer()
                IconButton("Add SSH computer", systemImage: "plus") {
                    editingSSH = SSHProfile(host: "", name: "")
                }
            }
            if host.ssh.profiles.isEmpty {
                Text("Run bots on another computer you reach with SSH. It needs codync-host installed; your SSH keys stay on this Mac.")
                    .font(.callout)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(host.ssh.profiles) { profile in
                SSHRow(profile: profile) { editingSSH = profile }
            }
        }
    }

    @ViewBuilder private var accountSection: some View {
        let managed = Set(host.managedStores.map(\.computer.id))
        let reached = host.accounts.computers.filter { !managed.contains($0.id) }
        let others = host.accounts.cloudComputers.filter { c in !managed.contains(c.id) && !reached.contains { $0.id == c.id } }
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SectionTitle("In your account")
                Spacer()
                if account.isSignedIn {
                    IconButton("Refresh", systemImage: "arrow.clockwise") { Task { await host.accounts.refreshCloud() } }
                }
            }
            if !account.isSignedIn {
                Text("Sign in to reach the other computers in your account from this Mac.")
                    .font(.callout).foregroundStyle(Palette.secondary)
            } else if reached.isEmpty && others.isEmpty {
                Text("No other computers in your account yet.").font(.callout).foregroundStyle(Palette.secondary)
            }
            ForEach(reached) { computer in
                if let store = host.accounts.store(for: computer.id) {
                    HStack(spacing: 10) {
                        ComputerBadge(store.computer, size: 22)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(store.hostName).font(.callout.weight(.medium))
                            Text(store.statusText).font(.caption).foregroundStyle(Palette.secondary)
                        }
                        RouteLabel(store: store, ssh: false).foregroundStyle(Palette.tertiary)
                        Spacer()
                        IconButton("Remove from this Mac", systemImage: "minus.circle") { host.accounts.forget(computer.id) }
                    }
                    .card()
                }
            }
            ForEach(others) { computer in
                AccountComputerRow(computer: computer)
            }
        }
    }
}

private struct SectionTitle: View {
    let title: String
    init(_ title: String) { self.title = title }
    var body: some View {
        Text(title).font(.headline).foregroundStyle(Palette.text).accessibilityAddTraits(.isHeader)
    }
}

/// An account computer this Mac isn't authorized on: ask, then compare the code shown there.
private struct AccountComputerRow: View {
    let computer: CloudComputer
    @Environment(HostController.self) private var host
    @State private var busy = false

    var body: some View {
        let ticket = host.accounts.pendingAccess[computer.id]
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                ComputerBadge(Computer(id: computer.id, name: computer.name, signKey: computer.signKey, device: computer.device), size: 22)
                VStack(alignment: .leading, spacing: 1) {
                    Text(computer.name).font(.callout.weight(.medium))
                    Text(computer.isOnline ? "Online" : "Offline").font(.caption).foregroundStyle(Palette.secondary)
                }
                Spacer()
                if ticket == nil {
                    Button(busy ? "Asking…" : "Ask for access") {
                        busy = true
                        Task {
                            defer { busy = false }
                            do { _ = try await host.accounts.requestAccess(computer.id) } catch {
                                host.accounts.lastError = error.localizedDescription
                            }
                        }
                    }
                    .disabled(busy)
                }
            }
            if let ticket {
                HStack(spacing: 12) {
                    Text(ticket.code.count == 6 ? "\(ticket.code.prefix(3)) \(ticket.code.suffix(3))" : ticket.code)
                        .font(.system(size: 26, weight: .semibold, design: .monospaced))
                        .accessibilityLabel("Code \(ticket.code.map(String.init).joined(separator: " "))")
                    Text("Approve on \(computer.name) only if it shows this code.")
                        .font(.callout).foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .card()
    }
}

/// This Mac or an SSH computer: relay, account membership, pairing and authorized devices.
private struct ManagedComputerCard: View {
    let store: BotStore
    let ssh: Bool
    @Environment(HostController.self) private var host
    @Environment(AccountSession.self) private var account
    @State private var devices: [AuthorizedDevice]?
    @State private var busy = false
    @State private var showPairing = false
    @State private var confirmRevoke: AuthorizedDevice?
    @State private var confirmUnclaim = false

    private var cloud: CloudStatus? { store.cloud }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                ComputerBadge(store.computer, size: 26)
                VStack(alignment: .leading, spacing: 1) {
                    Text(store.hostName).font(.headline)
                    Text(ssh ? "Over SSH · \(store.statusText)" : "This Mac · \(store.statusText)")
                        .font(.caption).foregroundStyle(Palette.secondary)
                }
                Spacer()
                IconButton("Pair iPhone", systemImage: "qrcode", selected: showPairing) { showPairing.toggle() }
                    .disabled(store.connection != .online)
                    .popover(isPresented: $showPairing, arrowEdge: .bottom) {
                        PairingPanel(store: store).frame(width: 300)
                    }
            }

            Toggle(isOn: Binding(get: { cloud?.enabled ?? false }, set: { on in run { await host.setCloud(store, enabled: on) } })) {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Reach from anywhere")
                    Text(cloudLine).font(.caption).foregroundStyle(cloud?.lastError == nil ? Palette.secondary : Palette.warning)
                }
            }
            .toggleStyle(.switch)
            .disabled(busy || store.connection != .online)

            accountLine

            VStack(alignment: .leading, spacing: 6) {
                Text("Devices that can use \(store.hostName)").font(.subheadline.weight(.semibold))
                if let devices {
                    if devices.isEmpty {
                        Text("None yet. Pair an iPhone, or approve one from your account.").font(.caption).foregroundStyle(Palette.secondary)
                    }
                    ForEach(devices) { device in deviceRow(device) }
                } else {
                    ProgressView().controlSize(.small)
                }
            }
        }
        .card()
        .task(id: "\(store.computer.id)/\(store.connection == .online)/\(store.accessRequests.count)") { await loadDevices() }
        .confirmationDialog("Revoke \(confirmRevoke?.name ?? "device")?", isPresented: Binding(
            get: { confirmRevoke != nil }, set: { if !$0 { confirmRevoke = nil } }
        ), titleVisibility: .visible) {
            if let device = confirmRevoke {
                Button("Revoke access", role: .destructive) {
                    run {
                        do { try await store.client?.revokeDevice(device.key) } catch { host.accounts.lastError = error.localizedDescription }
                        await loadDevices()
                    }
                }
            }
        } message: {
            Text("It disconnects right away and has to pair or ask again.")
        }
        .confirmationDialog("Remove \(store.hostName) from the account?", isPresented: $confirmUnclaim, titleVisibility: .visible) {
            Button("Remove from account", role: .destructive) { run { await host.unclaim(store) } }
        } message: {
            Text("Devices that were approved through the account lose access. Devices paired with a QR code keep it.")
        }
    }

    private var cloudLine: String {
        guard let cloud, cloud.enabled else { return "Off: only devices on the same network reach it." }
        if let error = cloud.lastError { return error }
        return cloud.connected == true ? "Connected to the encrypted relay." : "Connecting to the relay…"
    }

    @ViewBuilder private var accountLine: some View {
        HStack(spacing: 8) {
            if let owner = cloud?.owner {
                Image(systemName: "person.crop.circle.badge.checkmark").foregroundStyle(Palette.secondary).accessibilityHidden(true)
                Text(owner.userId == account.userID ? "In your account" : "In \(owner.email ?? "another") account")
                    .font(.callout)
                Spacer()
                Button("Remove from account") { confirmUnclaim = true }
                    .disabled(busy || store.connection != .online)
            } else {
                Image(systemName: "person.crop.circle.badge.plus").foregroundStyle(Palette.secondary).accessibilityHidden(true)
                Text(account.isSignedIn ? "Not in your account" : "Sign in to add it to your account").font(.callout)
                Spacer()
                if account.isSignedIn {
                    Button(busy ? "Adding…" : "Add to account") { run { await host.claim(store) } }
                        .disabled(busy || store.connection != .online)
                }
            }
        }
    }

    private func deviceRow(_ device: AuthorizedDevice) -> some View {
        HStack(spacing: 8) {
            Image(systemName: device.platform == "macos" ? "laptopcomputer" : "iphone")
                .frame(width: 18)
                .foregroundStyle(Palette.secondary)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(device.name).font(.callout)
                Text(detail(device)).font(.caption).foregroundStyle(Palette.tertiary)
            }
            Spacer()
            IconButton("Revoke access", systemImage: "xmark.circle") { confirmRevoke = device }
        }
        .accessibilityElement(children: .combine)
    }

    private func detail(_ device: AuthorizedDevice) -> String {
        var parts = [device.source == "account" ? "Approved through the account" : "Paired with a QR code"]
        if device.connected == true {
            parts.append("connected")
        } else if let seen = device.lastSeenAt {
            parts.append("seen \(RelativeTime.day(Date(milliseconds: seen)))")
        }
        return parts.joined(separator: " · ")
    }

    private func loadDevices() async {
        guard let client = store.client, store.connection == .online else { return }
        devices = (try? await client.devices()) ?? devices
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        busy = true
        Task {
            await work()
            busy = false
        }
    }
}

// MARK: - SSH

private struct SSHRow: View {
    let profile: SSHProfile
    let edit: () -> Void
    @Environment(HostController.self) private var host

    var body: some View {
        let status = host.ssh.status(of: profile.id)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Image(systemName: "terminal").frame(width: 22).foregroundStyle(Palette.secondary).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 1) {
                    Text(profile.name.isEmpty ? profile.host : profile.name).font(.callout.weight(.medium))
                    Text(line(status)).font(.caption).foregroundStyle(isProblem(status) ? Palette.warning : Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                switch status {
                case .connected, .connecting, .retrying:
                    IconButton("Disconnect", systemImage: "stop.circle") { host.ssh.disconnect(profile.id) }
                default:
                    IconButton("Connect", systemImage: "arrow.clockwise") { host.ssh.connect(profile.id) }
                }
                IconButton("Edit", systemImage: "pencil", action: edit)
                IconButton("Remove", systemImage: "trash") { host.ssh.remove(profile.id) }
            }
            if case let .confirmHostKey(name, fingerprints, _) = status {
                VStack(alignment: .leading, spacing: 6) {
                    Text("First connection to \(name). Check that its host key fingerprint matches what the computer's owner sees (`ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`).")
                        .font(.caption).foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    ForEach(fingerprints, id: \.self) { Text($0).font(.caption.monospaced()).textSelection(.enabled) }
                    HStack {
                        Button("Trust and connect") { host.ssh.trustHostKey(profile.id) }
                        Button("Cancel") { host.ssh.disconnect(profile.id) }
                    }
                    .controlSize(.small)
                }
            }
            if status == .notInstalled {
                HStack(spacing: 6) {
                    Text(SSH.installCommand)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                        .padding(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 6))
                    IconButton("Copy install command", systemImage: "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(SSH.installCommand, forType: .string)
                    }
                }
            }
        }
        .card()
    }

    private func line(_ status: SSHComputers.Status) -> String {
        let target = [profile.user.map { "\($0)@" } ?? "", profile.host, profile.port.map { ":\($0)" } ?? ""].joined()
        switch status {
        case .idle: return "\(target) · Not connected"
        case let .connecting(step): return step
        case .confirmHostKey: return "\(target) · Confirm the host key"
        case .connected: return "\(target) · Connected"
        case let .retrying(message): return "\(message) Retrying…"
        case let .failed(message): return message
        case .notInstalled: return "codync-host isn't installed on \(profile.host). Install it there:"
        }
    }

    private func isProblem(_ status: SSHComputers.Status) -> Bool {
        switch status {
        case .retrying, .failed, .notInstalled: true
        default: false
        }
    }
}

/// Add or change an SSH computer. Only fields ssh takes as separate arguments; nothing goes through a shell.
private struct SSHProfileEditor: View {
    @State var profile: SSHProfile
    let isNew: Bool
    let close: () -> Void
    @Environment(HostController.self) private var host
    @State private var port = ""
    @State private var remotePort = ""
    @State private var user = ""
    @State private var chooseKey = false
    @State private var problem: String?

    init(profile: SSHProfile, isNew: Bool, close: @escaping () -> Void) {
        _profile = State(initialValue: profile)
        self.isNew = isNew
        self.close = close
        _port = State(initialValue: profile.port.map(String.init) ?? "")
        _remotePort = State(initialValue: String(profile.remotePort))
        _user = State(initialValue: profile.user ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(isNew ? "Add SSH computer" : "Edit SSH computer").font(.title3.bold())
            Form {
                TextField("Name", text: $profile.name, prompt: Text("Optional"))
                TextField("Host", text: $profile.host, prompt: Text("SSH alias or hostname"))
                TextField("User", text: $user, prompt: Text("From SSH config"))
                TextField("SSH port", text: $port, prompt: Text("From SSH config"))
                HStack {
                    Text("Key")
                    Spacer()
                    Text(profile.identityFile.map { URL(filePath: $0).lastPathComponent } ?? "ssh-agent / SSH config")
                        .foregroundStyle(Palette.secondary)
                    IconButton("Choose key file", systemImage: "key") { chooseKey = true }
                    if profile.identityFile != nil {
                        IconButton("Use ssh-agent", systemImage: "xmark.circle") { profile.identityFile = nil }
                    }
                }
                TextField("Codync port", text: $remotePort)
            }
            .formStyle(.grouped)
            Text("Codync opens an SSH tunnel to codync-host on that computer's loopback. It uses your SSH config and ssh-agent; agent forwarding stays off.")
                .font(.caption).foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let problem {
                Text(problem).font(.caption).foregroundStyle(Palette.danger)
            }
            HStack {
                Spacer()
                Button("Cancel", action: close).keyboardShortcut(.cancelAction)
                Button(isNew ? "Add" : "Save", action: save).keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
        .fileImporter(isPresented: $chooseKey, allowedContentTypes: [.item]) { result in
            if case let .success(url) = result { profile.identityFile = url.path }
        }
    }

    private func save() {
        var p = profile
        p.host = p.host.trimmingCharacters(in: .whitespaces)
        p.name = p.name.trimmingCharacters(in: .whitespaces)
        let u = user.trimmingCharacters(in: .whitespaces)
        p.user = u.isEmpty ? nil : u
        let portText = port.trimmingCharacters(in: .whitespaces)
        if portText.isEmpty {
            p.port = nil
        } else if let n = Int(portText) {
            p.port = n
        } else {
            problem = "The SSH port must be a number."
            return
        }
        guard let remote = Int(remotePort.trimmingCharacters(in: .whitespaces)) else {
            problem = "The Codync port must be a number."
            return
        }
        p.remotePort = remote
        if !isNew, let old = host.ssh.profiles.first(where: { $0.id == p.id }), old.host != p.host || old.port != p.port {
            // A different address may be a different computer: learn its identity again.
            p.computerId = nil
        }
        if let problem = SSH.validate(p) {
            self.problem = problem
            return
        }
        if isNew { host.ssh.add(p) } else { host.ssh.update(p) }
        close()
    }
}

private extension View {
    func card() -> some View {
        padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Palette.border, lineWidth: 0.5))
    }
}
