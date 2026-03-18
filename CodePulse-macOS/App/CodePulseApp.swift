import SwiftUI
import AppKit
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "App")

@main
struct CodePulseApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate

    var body: some Scene {
        Settings { EmptyView() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    var menuBarController: MenuBarController?
    let scanner = SessionScanner()
    var stateManager: SessionStateManager!
    var cloudKitSync: CloudKitSync!
    let hookServer = ClaudeHookServer()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        scanner.hookServer = hookServer
        stateManager = SessionStateManager(scanner: scanner)
        stateManager.hookServer = hookServer
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)

        hookServer.ensureHooksConfigured()
        // SessionStart/SessionEnd: immediate refresh (new/removed session)
        hookServer.onSessionEvent = { [weak self] in
            self?.stateManager.refreshFromHookState()
        }
        // All other hooks: immediate refresh with hook signal applied first
        hookServer.onHookSignal = { [weak self] sessionId, signal, toolName in
            self?.stateManager.transcriptWatcher.handleHookSignal(
                sessionId: sessionId, signal: signal, toolName: toolName
            )
            self?.stateManager.refreshFromHookState()
        }
        hookServer.start()

        scanner.start()
        logger.info("CodePulse launched — hook-driven status detection (7 hooks) + JSONL supplementary data")

        Task {
            do {
                let activeIds = Set(scanner.activeSessions.keys)
                try await CloudKitManager.shared.deleteOrphans(activeSessionIds: activeIds)
            } catch {
                logger.warning("Orphan cleanup failed: \(error.localizedDescription)")
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        scanner.stop()
        hookServer.stop()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
