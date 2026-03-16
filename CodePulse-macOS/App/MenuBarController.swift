import SwiftUI
import Combine
import CodePulseShared

@MainActor
final class MenuBarController: NSObject {
    private var statusItem: NSStatusItem!
    private var popover: NSPopover!
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        super.init()
        setupStatusItem()
        setupPopover()
        observeSessionCount()
    }

    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "waveform.path", accessibilityDescription: "CodePulse")
            button.action = #selector(togglePopover)
            button.target = self
        }
    }

    private func setupPopover() {
        popover = NSPopover()
        popover.contentSize = NSSize(width: 340, height: 400)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(
            rootView: SessionListView(stateManager: stateManager)
        )
    }

    private func observeSessionCount() {
        stateManager.$sessions
            .receive(on: RunLoop.main)
            .sink { [weak self] sessions in
                let activeCount = sessions.filter { $0.status.isActive }.count
                self?.updateBadge(count: activeCount)
            }
            .store(in: &cancellables)
    }

    private func updateBadge(count: Int) {
        guard let button = statusItem.button else { return }
        if count > 0 {
            button.title = " \(count)"
        } else {
            button.title = ""
        }
    }

    @objc private func togglePopover() {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }
}
