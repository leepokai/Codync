import SwiftUI
import AppKit
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "App")

@main
struct CodePulseApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate

    var body: some Scene {
        // Menu bar only app — no windows
        Settings { EmptyView() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    var menuBarController: MenuBarController?
    let scanner = SessionScanner()
    var stateManager: SessionStateManager!
    var cloudKitSync: CloudKitSync!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Hide from Dock — must be called early
        NSApp.setActivationPolicy(.accessory)

        stateManager = SessionStateManager(scanner: scanner)
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)
        scanner.start()

        logger.info("CodePulse launched, menu bar active")
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // Don't quit when popover closes
        false
    }
}
