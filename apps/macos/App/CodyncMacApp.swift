import CodyncKit
import CodyncUI
import CoreImage.CIFilterBuiltins
import SwiftUI

@main
struct CodyncMacApp: App {
    @NSApplicationDelegateAdaptor(CodyncAppDelegate.self) private var appDelegate
    @State private var updates = UpdatesManager()
    @State private var host: HostController
    @State private var account: AccountSession
    @Environment(\.openWindow) private var openWindow
    @AppStorage(ConversationTypography.preferenceKey)
    private var conversationFontSize = ConversationTypography.defaultSize

    init() {
        let account = AccountSession()
        _account = State(initialValue: account)
        _host = State(initialValue: HostController(account: account))
    }

    /// From both the window and the menu bar icon: macOS can hide the icon (no room, or not allowed in
    /// the menu bar), and its view then never appears. Both starts are no-ops after the first.
    private func launch() {
        host.start()
        appDelegate.updates = updates
        appDelegate.openChatWindow = { openWindow(id: "chat") }
        updates.start(host: host)
    }

    var body: some Scene {
        // First scene: the one SwiftUI opens at launch and when the Dock icon is clicked.
        Window("Codync", id: "chat") {
            ChatWindow()
                .environment(\.conversationTypography, ConversationTypography(pointSize: conversationFontSize))
                .task { launch() }
                .modalHost()
                .appFont(.body)
                .environment(\.appFontScale, ConversationTypography(pointSize: conversationFontSize).scale)
                .environment(host)
                .environment(account)
                .environment(updates)
        }
        .defaultSize(width: 1100, height: 760)
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(after: .toolbar) {
                Button("Bigger") { conversationFontSize = ConversationTypography.step(conversationFontSize, up: true) }
                    .keyboardShortcut("+")
                Button("Smaller") { conversationFontSize = ConversationTypography.step(conversationFontSize, up: false) }
                    .keyboardShortcut("-")
                Button("Actual Size") { conversationFontSize = ConversationTypography.defaultSize }
                    .keyboardShortcut("0")
            }
        }

        MenuBarExtra {
            MenuView()
                // The macOS 27 SDK drops menu item images unless the label asks for them.
                .labelStyle(.titleAndIcon)
                .environment(host)
                .environment(account)
                .environment(updates)
        } label: {
            // The Codync mark; a dot joins it when a bot needs you.
            Image(host.needsAttention ? "MenuBarIconAlert" : "MenuBarIcon")
                .task { launch() }
                .onChange(of: account.userID) { _, userID in host.switchAccount(to: userID) }
                // A device asking for access: bring up the window that holds the approval sheet.
                .onChange(of: host.currentApproval?.id) { _, id in
                    guard id != nil else { return }
                    openWindow(id: "chat")
                    NSApp.activate()
                }
        }
        .menuBarExtraStyle(.menu)

        Window("Pair iPhone", id: "pairing") {
            Group {
                if let store = host.store {
                    PairingPanel(store: store)
                        .id(store.computer.id)
                } else {
                    Text("Connect to this Mac's host before pairing your iPhone.")
                        .padding()
                }
            }
            .frame(width: 340)
            .appFont(.body)
            .environment(\.appFontScale, ConversationTypography(pointSize: conversationFontSize).scale)
            .environment(host)
        }
        .windowResizability(.contentSize)
    }
}

/// A system menu: macOS owns layout, selection, keyboard navigation and submenus.
struct MenuView: View {
    @Environment(UpdatesManager.self) private var updates
    @Environment(HostController.self) private var host
    @Environment(\.openWindow) private var openWindow
    @AppStorage(SharedStore.usageIconStyleKey, store: UserDefaults(suiteName: SharedStore.appGroup))
    private var usageIconStyle = UsageIconStyle.character.rawValue
    @AppStorage(ConversationTypography.preferenceKey)
    private var conversationFontSize = ConversationTypography.defaultSize
    @AppStorage(CodyncAppDelegate.showInDockKey) private var showInDock = false

    var body: some View {
        Text(status)
        Button("Open Codync") { open("chat") }
            .keyboardShortcut("o")
        if host.state == .running {
            Button("Pair iPhone…") {
                host.requestPairing()
                open("pairing")
            }
                .disabled(host.store == nil)
        }
        Divider()
        hostItems
        Divider()
        Menu("Settings") {
            Picker("Usage icons", selection: $usageIconStyle) {
                Text("Character").tag(UsageIconStyle.character.rawValue)
                Text("Original").tag(UsageIconStyle.original.rawValue)
            }
            Picker("Text Size", selection: $conversationFontSize) {
                ForEach(ConversationTypography.sizes, id: \.self) { size in
                    Text(size == ConversationTypography.defaultSize ? "\(Int(size)) pt (Default)" : "\(Int(size)) pt")
                        .tag(size)
                }
            }
            Toggle("Show in Dock", isOn: $showInDock)
                .onChange(of: showInDock) { _, visible in
                    let window = NSApp.windows.first { $0.isVisible && !$0.isMiniaturized && $0.canBecomeMain }
                    NSApp.setActivationPolicy(visible ? .regular : .accessory)
                    if visible || window != nil { NSApp.activate() }
                    window?.makeKeyAndOrderFront(nil)
                }
            Toggle("Open at login", isOn: Binding(
                get: { host.launchAtLogin },
                set: { host.setLaunchAtLogin($0) }
            ))
            Divider()
            Menu("Updates") {
                Button(updates.availableVersion.map { "Update to \($0)…" } ?? "Check for Updates…") {
                    updates.checkForUpdates()
                }
                .disabled(!updates.canCheckForUpdates && !updates.hasStagedUpdate)
                if let app = updates.waitingForApp {
                    // The release waits until paired iPhones can get the app it needs.
                    Text("Waiting for iPhone app \(app) to pass App Store review")
                }
                Toggle("Automatically check for updates", isOn: Binding(
                    get: { updates.automaticallyChecksForUpdates },
                    set: { updates.automaticallyChecksForUpdates = $0 }
                ))
                .disabled(!updates.isSupported)
                Toggle("Automatically download and install", isOn: Binding(
                    get: { updates.automaticallyDownloadsUpdates },
                    set: { updates.automaticallyDownloadsUpdates = $0 }
                ))
                .disabled(!updates.isSupported)
                if !updates.isSupported { Text("Updates are available in release builds.") }
                if let date = updates.lastUpdateCheckDate { Text("Last checked: \(date.formatted())") }
                if let error = updates.errorMessage {
                    Text(error)
                    Button("Retry installing update") { updates.retryInstallation() }
                        .disabled(updates.preparingInstallation)
                }
            }
            Divider()
            Button("Restart host") { host.restart() }
            Button("Open log") { NSWorkspace.shared.open(host.logURL) }
            Divider()
            Button("Uninstall host service") { host.uninstall() }
            Divider()
            Button("Reset all data…") {
                host.confirmsReset = true
                open("chat")
            }
        }
        if let version = host.version { Text("Version \(version)") }
        Button("Quit Codync") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private var status: String {
        switch host.state {
        case .missingBinary: "Host not found"
        case .notInstalled: "Host not installed"
        case .starting: "Connecting…"
        case .failed: "Host problem"
        case .running:
            if !host.approvals.isEmpty {
                "A device asks for access"
            } else if host.needsAttention {
                "A bot needs you"
            } else if host.working > 0 {
                "\(host.working) bot\(host.working == 1 ? "" : "s") working"
            } else {
                "Connected"
            }
        }
    }

    @ViewBuilder private var hostItems: some View {
        switch host.state {
        case .missingBinary:
            Text("Reinstall Codync or install the host with Homebrew.")
            Button("Copy host install command") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString("brew install leepokai/codync/codync-host", forType: .string)
            }
        case .notInstalled:
            Button("Install host") { host.install() }
        case .starting:
            Text("Starting the host…")
        case let .failed(message):
            Text(message)
            Button("Restart host") { host.restart() }
            Button("Open log") { NSWorkspace.shared.open(host.logURL) }
        case .running:
            if let approval = host.approvals.first {
                Button("Review access request from \(approval.request.deviceName)…") {
                    host.reviewApprovals()
                    open("chat")
                }
                Divider()
            }
            bots
            if host.screen != nil {
                Divider()
                RemoteScreenMenu()
            }
            if !host.usage.providers.isEmpty {
                Divider()
                ForEach(host.usage.providers) { provider in
                    // Buttons, not text: the menu dims a disabled item's image. They open the app.
                    Button { open("chat") } label: {
                        Label {
                            Text(provider.name)
                        } icon: {
                            menuIcon(ProviderMascot(provider, size: 16, style: UsageIconStyle(rawValue: usageIconStyle) ?? .character),
                                     key: "provider|\(provider.id)|\(usageIconStyle)|\((provider.tightest?.percent ?? 0) >= 90)")
                        }
                    }
                    // A bar per limit, with its numbers as the item's title (what VoiceOver reads too).
                    ForEach(provider.windows) { window in
                        Button { open("chat") } label: {
                            Label {
                                Text(usageLine(window))
                            } icon: {
                                menuIcon(UsageBar(window: window, tint: provider.tint),
                                         key: "bar|\(provider.id)|\(Int(window.percent.rounded()))")
                            }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder private var bots: some View {
        let roster = host.roster.filter { !$0.bot.isGroup }
        if roster.isEmpty {
            Text("No bots yet")
        } else {
            ForEach(roster) { item in
                Menu {
                    Text(item.bot.needsInput ? "Needs your response" : item.bot.isWorking
                        ? (item.bot.activity.isEmpty ? "Working…" : item.bot.activity)
                        : (item.bot.lastMessage ?? item.bot.folderName))
                    Button("Open conversation") {
                        host.accounts.selection = item.ref
                        open("chat")
                    }
                    if item.bot.isWorking {
                        Button("Stop task") { host.stop(item) }
                    }
                } label: {
                    Label {
                        Text(host.accounts.computers.count > 1 ? "\(item.bot.name) · \(item.computer.name)" : item.bot.name)
                    } icon: {
                        menuIcon(AvatarWithStatus(bot: item.bot, size: 20), key: avatarKey(item.bot))
                    }
                }
            }
        }
    }

    private func open(_ id: String) {
        openWindow(id: id)
        NSApp.activate()
    }

    private func usageLine(_ window: UsageWindow) -> String {
        let reset = window.resetDate.map { " · resets in \(RelativeTime.until($0))" } ?? ""
        return "\(window.title) · \(Int(window.percent.rounded()))%\(reset)"
    }

    /// Everything the avatar image shows; the activity text changes often and isn't drawn.
    private func avatarKey(_ bot: Bot) -> String {
        "bot|\(bot.id)|\(bot.avatarShape)|\(bot.avatarColor)|\(bot.status)|\(bot.needsInput)|\(bot.unread > 0)"
    }

    /// Rendered icons by what they show and the appearance: a bot's status change redraws one image,
    /// not the whole menu, and nothing is drawn again while the menu is closed and nothing changed.
    @MainActor private static let iconCache = NSCache<NSString, NSImage>()

    /// NSMenu items only take text and an image, so custom drawing goes in as a rendered image.
    private func menuIcon(_ view: some View, key: String) -> Image {
        // The menu follows the system appearance; this view's environment doesn't. Reading
        // `host.menuIsDark` here redraws the menu when the system switches.
        let dark = host.menuIsDark
        let cacheKey = "\(key)|\(dark)" as NSString
        if let cached = Self.iconCache.object(forKey: cacheKey) { return Image(nsImage: cached) }
        let appearance = NSAppearance(named: dark ? .darkAqua : .aqua) ?? NSApp.effectiveAppearance
        let renderer = ImageRenderer(content: view.environment(\.colorScheme, dark ? .dark : .light))
        renderer.scale = NSScreen.main?.backingScaleFactor ?? 2
        var image: NSImage?
        appearance.performAsCurrentDrawingAppearance { image = renderer.nsImage }
        guard let image else { return Image(systemName: "circle") }
        Self.iconCache.setObject(image, forKey: cacheKey)
        return Image(nsImage: image)
    }
}

/// One usage limit's bar in the provider's color: amber from 70%, red from 90%.
private struct UsageBar: View {
    let window: UsageWindow
    let tint: Color

    var body: some View {
        let color = window.percent >= 90 ? Palette.danger : window.percent >= 70 ? Palette.warning : tint
        ZStack(alignment: .leading) {
            Capsule().fill(Color.primary.opacity(0.12))
            if window.percent > 0 {
                Capsule().fill(color).frame(width: max(5, 60 * min(1, window.percent / 100)))
            }
        }
        .frame(width: 60, height: 5)
        .frame(height: 16)
    }
}

private struct RemoteScreenMenu: View {
    @Environment(HostController.self) private var host

    var body: some View {
        let screen = host.screen ?? ScreenState()
        Menu("Remote screen") {
            Toggle("Enable remote screen", isOn: Binding(
                get: { host.screen?.enabled == true },
                set: { host.setRemoteScreen($0) }
            ))
            Text(subtitle(screen))
            if screen.enabled {
                if host.screenAgentNeedsApproval {
                    Button("Allow Codync Screen in Login Items…") { host.openLoginItemsSettings() }
                } else if screen.connected {
                    if !screen.capture {
                        Button("Allow Screen Recording…") {
                            open("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                        }
                    }
                    if !screen.input {
                        Button("Allow Accessibility…") {
                            open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                        }
                    }
                }
            }
            if let error = host.screenError { Text(error) }
        }
    }

    private func subtitle(_ screen: ScreenState) -> String {
        guard screen.enabled else { return "Control this Mac from your iPhone" }
        if host.screenAgentNeedsApproval { return "Needs approval in System Settings" }
        if !screen.connected { return "Starting Codync Screen…" }
        if !screen.capture || !screen.input { return "Needs permission" }
        if screen.viewers > 0 { return screen.viewers == 1 ? "Your iPhone is viewing" : "\(screen.viewers) viewers" }
        if let bot = screen.agentBot, let name = host.store?.bots[bot]?.name { return "\(name) is using it" }
        let display = screen.displays.first(where: \.main)?.name ?? screen.displays.first?.name
        return display.map { "Ready · \($0)" } ?? "Ready"
    }

    private func open(_ url: String) {
        if let url = URL(string: url) { NSWorkspace.shared.open(url) }
    }
}

/// A one-time pairing QR (v3) from a computer this Mac manages: its keys, a code, and how to reach it.
struct PairingPanel: View {
    let store: BotStore
    @Environment(HostController.self) private var host
    @State private var info: PairingInfo?
    @State private var error: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 12) {
            if let info {
                if let image = qr(info.pairingUrl) {
                    Image(nsImage: image)
                        .interpolation(.none)
                        .resizable()
                        .frame(width: 200, height: 200)
                        .padding(10)
                        .background(.white, in: RoundedRectangle(cornerRadius: 12))
                        .accessibilityLabel("Pairing code for \(store.hostName)")
                }
                Text("Scan with the Codync app or the iPhone Camera. No Tailscale or open ports needed: your iPhone connects directly on the same Wi-Fi, and through the encrypted relay anywhere else.")
                    .appFont(.caption)
                    .foregroundStyle(Palette.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                if store.cloud?.enabled != true {
                    Text(info.urls.isEmpty
                        ? "No network address found and “Reach from anywhere” is off. Connect to Wi-Fi or turn it on."
                        : "“Reach from anywhere” is off, so the iPhone only reaches \(store.hostName) on the same network.")
                        .appFont(.caption)
                        .foregroundStyle(Palette.warning)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 4) {
                    if let expires = info.expiresAt {
                        Text("Works once, until \(Date(milliseconds: expires).formatted(date: .omitted, time: .shortened))")
                            .appFont(.caption2).foregroundStyle(Palette.tertiary)
                    }
                    IconButton("New code", systemImage: "arrow.clockwise") { Task { await load() } }
                    IconButton("Copy pairing link", systemImage: "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(info.pairingUrl, forType: .string)
                    }
                }
            } else if let error {
                Text(error).appFont(.caption).foregroundStyle(Palette.danger).multilineTextAlignment(.center)
                IconButton("Try again", systemImage: "arrow.clockwise") { Task { await load() } }
            } else {
                Spinner(size: 20)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(16)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: info?.pairingUrl)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: error)
        // A new code each time "Pair iPhone…" is chosen (the window stays open between uses).
        .task(id: "\(store.computer.id)#\(host.pairingRequest)") { await load() }
        // And a fresh one when this code expires while it's on screen.
        .task(id: info?.expiresAt) {
            guard let expires = info?.expiresAt else { return }
            let wait = Date(milliseconds: expires).timeIntervalSinceNow
            try? await Task.sleep(for: .seconds(max(1, wait)))
            if !Task.isCancelled { await load() }
        }
    }

    private func load() async {
        guard let client = store.client else {
            error = "\(store.hostName) isn't connected."
            return
        }
        do {
            info = try await client.pairing()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func qr(_ string: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(string.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let rep = NSCIImageRep(ciImage: output)
        let image = NSImage(size: rep.size)
        image.addRepresentation(rep)
        return image
    }
}
