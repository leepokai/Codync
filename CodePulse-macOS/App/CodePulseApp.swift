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
        stateManager.hookServer = hookServer // Connect hook state to state manager
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)

        // Hook server for real-time updates
        hookServer.ensureHooksConfigured()
        hookServer.onEvent = { [weak self] in
            self?.scanner.scan() // Immediate rescan on hook event
        }
        hookServer.start()

        scanner.start()
        logger.info("CodePulse launched — menu bar + hook server active")
    }

    func applicationWillTerminate(_ notification: Notification) {
        scanner.stop()
        hookServer.stop()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
