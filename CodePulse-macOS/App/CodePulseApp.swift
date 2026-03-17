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

        // Notification-only hook for instant permission detection
        hookServer.ensureHooksConfigured()
        hookServer.onPermissionEvent = { [weak self] in
            self?.scanner.scan() // Trigger immediate rescan → state manager picks up permission
        }
        hookServer.start()

        scanner.start()
        logger.info("CodePulse launched — JSONL transcript watcher + notification hook active")
    }

    func applicationWillTerminate(_ notification: Notification) {
        scanner.stop()
        hookServer.stop()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
