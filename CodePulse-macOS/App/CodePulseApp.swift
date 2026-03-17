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

        stateManager = SessionStateManager(scanner: scanner)
        stateManager.hookServer = hookServer
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)

        // Hook events: Notification, PermissionRequest, PreCompact, SessionStart, SessionEnd
        hookServer.ensureHooksConfigured()
        hookServer.onPermissionEvent = { [weak self] in
            self?.scanner.scan()
        }
        hookServer.onSessionEvent = { [weak self] in
            self?.scanner.scan()
        }
        hookServer.onHookSignal = { [weak self] sessionId, signalType, toolName in
            self?.stateManager.transcriptWatcher.handleHookSignal(
                sessionId: sessionId, signalType: signalType, toolName: toolName
            )
            self?.scanner.scan()
        }
        hookServer.start()

        scanner.start()
        logger.info("CodePulse launched — JSONL watcher + Notification/SessionStart/SessionEnd hooks active")

        // Clean up orphan CloudKit records from prior crashes or stale sessions
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
