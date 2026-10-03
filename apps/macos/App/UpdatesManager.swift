import AppKit
import CodyncKit
import Observation
import Sparkle

/// Sparkle owns download, signature verification and app replacement. Codync
/// coordinates its background services before letting the installer proceed.
@MainActor
@Observable
final class UpdatesManager: NSObject, SPUUpdaterDelegate, @preconcurrency SPUStandardUserDriverDelegate {
    private(set) var canCheckForUpdates = false
    private(set) var availableVersion: String?
    private(set) var lastUpdateCheckDate: Date?
    private(set) var errorMessage: String?
    private(set) var hasStagedUpdate = false
    private(set) var preparingInstallation = false
    /// The iPhone app the available release needs while the App Store doesn't have it yet.
    private(set) var waitingForApp: String?

    var automaticallyChecksForUpdates = true {
        didSet { controller?.updater.automaticallyChecksForUpdates = automaticallyChecksForUpdates }
    }
    var automaticallyDownloadsUpdates = false {
        didSet { controller?.updater.automaticallyDownloadsUpdates = automaticallyDownloadsUpdates }
    }

    #if DEBUG
    let isSupported = false
    #else
    let isSupported = true
    #endif

    @ObservationIgnored private weak var host: HostController?
    @ObservationIgnored private var controller: SPUStandardUpdaterController?
    @ObservationIgnored private var observations: [NSKeyValueObservation] = []
    @ObservationIgnored private var idleTask: Task<Void, Never>?
    @ObservationIgnored private var pendingInstall: (() -> Void)?
    @ObservationIgnored private var automaticInstall: (() -> Void)?
    @ObservationIgnored private var servicesPrepared = false
    @ObservationIgnored private var preparationTask: Task<Bool, Never>?
    /// For the App Store gate, refreshed before manual checks and every few hours: Sparkle asks
    /// its delegate synchronously (docs/reference/compatibility.md).
    @ObservationIgnored private var appStoreVersion: String?
    @ObservationIgnored private var iphonesPaired: Bool?
    @ObservationIgnored private var gateTask: Task<Void, Never>?

    func start(host: HostController) {
        guard controller == nil, isSupported else { return }
        self.host = host
        let controller = SPUStandardUpdaterController(startingUpdater: false, updaterDelegate: self, userDriverDelegate: self)
        self.controller = controller
        let updater = controller.updater
        observations = [
            updater.observe(\.canCheckForUpdates, options: [.initial, .new]) { [weak self] _, _ in
                Task { @MainActor in self?.readSettings() }
            },
            updater.observe(\.lastUpdateCheckDate, options: [.initial, .new]) { [weak self] _, _ in
                Task { @MainActor in self?.readSettings() }
            },
            updater.observe(\.automaticallyChecksForUpdates, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in self?.readSettings() }
            },
            updater.observe(\.automaticallyDownloadsUpdates, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in self?.readSettings() }
            },
        ]
        do {
            try updater.start()
            readSettings()
        } catch {
            errorMessage = error.localizedDescription
        }
        gateTask = Task { [weak self] in
            while !Task.isCancelled, let interval = await self?.gateTick() {
                try? await Task.sleep(for: interval)
            }
        }
    }

    /// Refreshes the gate; while waiting for the iPhone app it looks hourly and checks for the
    /// update as soon as the App Store has it, instead of at the next daily check.
    private func gateTick() async -> Duration {
        await refreshGate()
        if let required = waitingForApp, let store = appStoreVersion,
           AppVersion.parse(store) != nil, !AppVersion.isBelow(store, required) {
            controller?.updater.checkForUpdatesInBackground()
        }
        return .seconds(waitingForApp == nil ? 6 * 3600 : 3600)
    }

    private func refreshGate() async {
        if let version = try? await AppStoreRelease.latestVersion() { appStoreVersion = version }
        if let devices = try? await host?.store?.client?.devices() {
            iphonesPaired = devices.contains { $0.platform == "ios" }
        }
    }

    private func readSettings() {
        guard let updater = controller?.updater else { return }
        canCheckForUpdates = updater.canCheckForUpdates
        lastUpdateCheckDate = updater.lastUpdateCheckDate
        if automaticallyChecksForUpdates != updater.automaticallyChecksForUpdates {
            automaticallyChecksForUpdates = updater.automaticallyChecksForUpdates
        }
        if automaticallyDownloadsUpdates != updater.automaticallyDownloadsUpdates {
            automaticallyDownloadsUpdates = updater.automaticallyDownloadsUpdates
        }
    }

    func checkForUpdates() {
        if let pendingInstall {
            prepareAndContinue(pendingInstall)
            return
        }
        if let install = automaticInstall {
            automaticInstall = nil
            idleTask?.cancel()
            prepareAndContinue(install)
            return
        }
        guard canCheckForUpdates else { return }
        errorMessage = nil
        NSApp.activate()
        Task {
            await refreshGate()
            controller?.checkForUpdates(nil)
        }
    }

    func retryInstallation() {
        guard let pendingInstall else { return }
        prepareAndContinue(pendingInstall)
    }

    private func prepareAndContinue(_ install: @escaping () -> Void) {
        guard !preparingInstallation else { return }
        pendingInstall = install
        preparingInstallation = true
        Task {
            defer { preparingInstallation = false }
            if await prepareServices() {
                pendingInstall = nil
                install()
            }
        }
    }

    /// Also used on Quit: Sparkle may install a staged update without relaunching,
    /// in which case its postpone-relaunch delegate is not guaranteed to run.
    func prepareServices() async -> Bool {
        if servicesPrepared { return true }
        if let preparationTask { return await preparationTask.value }
        let task = Task { await self.stopServices() }
        preparationTask = task
        let ready = await task.value
        preparationTask = nil
        return ready
    }

    private func stopServices() async -> Bool {
        guard let host else { return false }
        do {
            try await host.prepareForUpdate()
            guard !Task.isCancelled else {
                host.resumeAfterCancelledUpdate()
                return false
            }
            servicesPrepared = true
            errorMessage = nil
            return true
        } catch {
            errorMessage = "Update paused: \(error.localizedDescription)"
            host.resumeAfterCancelledUpdate()
            return false
        }
    }

    /// A release that needs a newer iPhone app than the App Store has (still in review) waits,
    /// so paired iPhones aren't asked for an update they can't get yet. Without an answer from
    /// the App Store, background checks wait and a check the person started goes ahead.
    func updater(_ updater: SPUUpdater, shouldProceedWithUpdate item: SUAppcastItem, updateCheck: SPUUpdateCheck) throws {
        waitingForApp = nil
        guard let required = item.propertiesDictionary["codync:minApp"] as? String,
              AppVersion.isBelow(host?.store?.hostVersion?.minApp ?? "0", required),
              iphonesPaired != false else { return }
        if let appStoreVersion, AppVersion.parse(appStoreVersion) != nil, !AppVersion.isBelow(appStoreVersion, required) {
            return
        }
        if appStoreVersion == nil && updateCheck == .updates { return }
        waitingForApp = required
        throw NSError(domain: "Codync", code: 1, userInfo: [NSLocalizedDescriptionKey:
            "Codync \(item.displayVersionString) needs the iPhone app \(required), which isn't in the App Store yet. "
            + "It installs once that version passes review."])
    }

    func updater(_ updater: SPUUpdater, shouldPostponeRelaunchForUpdate item: SUAppcastItem,
                 untilInvokingBlock installHandler: @escaping () -> Void) -> Bool {
        hasStagedUpdate = true
        prepareAndContinue(installHandler)
        return true
    }

    func updater(_ updater: SPUUpdater, willInstallUpdate item: SUAppcastItem) {
        hasStagedUpdate = true
    }

    func updater(_ updater: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem,
                 immediateInstallationBlock installHandler: @escaping () -> Void) -> Bool {
        hasStagedUpdate = true
        availableVersion = item.displayVersionString
        automaticInstall = installHandler
        idleTask?.cancel()
        idleTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard !Task.isCancelled, let self else { return }
                guard self.automaticallyDownloadsUpdates, !NSApp.isActive,
                      CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: .null) >= 600,
                      await self.host?.isIdleForUpdate() == true,
                      let install = self.automaticInstall else { continue }
                self.automaticInstall = nil
                self.prepareAndContinue(install)
                return
            }
        }
        return true
    }

    func updater(_ updater: SPUUpdater, didAbortWithError error: Error) {
        idleTask?.cancel()
        preparationTask?.cancel()
        pendingInstall = nil
        automaticInstall = nil
        hasStagedUpdate = false
        servicesPrepared = false
        availableVersion = nil
        // No-update and user cancellation are ordinary outcomes; Sparkle shows
        // actionable errors in its standard interface for a manual update check.
        let failure = error as NSError
        let ordinary = failure.domain == SUSparkleErrorDomain &&
            [Int(SUError.noUpdateError.rawValue), Int(SUError.installationCanceledError.rawValue)].contains(failure.code)
        errorMessage = ordinary ? nil : error.localizedDescription
        host?.resumeAfterCancelledUpdate()
        readSettings()
    }

    var supportsGentleScheduledUpdateReminders: Bool { true }

    func standardUserDriverShouldHandleShowingScheduledUpdate(_ update: SUAppcastItem,
                                                               andInImmediateFocus immediateFocus: Bool) -> Bool {
        NSApp.isActive && immediateFocus
    }

    func standardUserDriverWillHandleShowingUpdate(_ handleShowingUpdate: Bool,
                                                   forUpdate update: SUAppcastItem, state: SPUUserUpdateState) {
        availableVersion = update.displayVersionString
        if state.userInitiated { NSApp.activate() }
    }
}

/// Normal Quit leaves the daemon running. If Sparkle has staged an update,
/// termination waits until the background services have actually stopped.
@MainActor
final class CodyncAppDelegate: NSObject, NSApplicationDelegate {
    static let showInDockKey = "showInDock"

    weak var updates: UpdatesManager?
    var openChatWindow: (() -> Void)?

    func applicationWillFinishLaunching(_ notification: Notification) {
        let visible = UserDefaults.standard.bool(forKey: Self.showInDockKey)
        NSApp.setActivationPolicy(visible ? .regular : .accessory)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openChatWindow?()
        sender.activate()
        return false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let updates, updates.hasStagedUpdate else { return .terminateNow }
        Task {
            let ready = await updates.prepareServices()
            sender.reply(toApplicationShouldTerminate: ready)
        }
        return .terminateLater
    }
}
