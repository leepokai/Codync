import SwiftUI
import CodePulseShared

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

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        stateManager = SessionStateManager(scanner: scanner)
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)
        scanner.start()
    }
}
