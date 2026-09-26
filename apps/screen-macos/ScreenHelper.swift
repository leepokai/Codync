import AppKit
import ApplicationServices
import CoreGraphics

/// Serves the host's screen requests (see `host/src/screen.rs` for the protocol).
@MainActor
final class ScreenHelper {
    private let link = HostLink()
    let input = InputInjector()
    private var sessions: [String: Session] = [:]
    private let badge = ViewingBadge()
    private var lastStatus: [String: AnyHashable] = [:]

    init() {
        link.handler = { [unowned self] method, params in try await self.handle(method, params) }
        link.onConnect = { [unowned self] in
            // The host lost every session when we dropped.
            closeAll()
            lastStatus = [:]
            sendStatus()
        }
        badge.onDisconnect = { [unowned self] in closeAll() }
    }

    func start() {
        let captureAtLaunch = CGPreflightScreenCaptureAccess()
        if !captureAtLaunch { CGRequestScreenCaptureAccess() }
        // `kAXTrustedCheckOptionPrompt` is a C global Swift 6 won't read; its value is this string.
        _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        link.start()
        NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.sendStatus() }
        }
        // Permission grants don't notify: poll, cheaply, and report changes.
        Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                self?.sendStatus()
                // A process keeps the screen-recording answer it saw at launch, so a grant made
                // while we run never shows up here. Ask a fresh process; once it's granted, quit
                // and let launchd (KeepAlive) start us again with capture working.
                if !captureAtLaunch, await Self.captureGrantedNow() {
                    self?.closeAll()
                    exit(0)
                }
            }
        }
    }

    private func handle(_ method: String, _ p: [String: Any]) async throws -> Any {
        switch method {
        case "answer":
            guard let id = p["session"] as? String, let sdp = p["sdp"] as? String else { throw HelperError("session and sdp are required") }
            if let s = sessions[id] { return ["sdp": try await s.answer(offer: sdp)] }
            let s = Session(id: id, display: display(p["display"]), helper: self)
            sessions[id] = s
            do {
                let answer = try await s.answer(offer: sdp)
                badge.update(viewers: sessions.count)
                return ["sdp": answer]
            } catch {
                sessions[id] = nil
                s.close()
                throw error
            }
        case "close":
            if let id = p["session"] as? String, let s = sessions.removeValue(forKey: id) {
                s.close()
                badge.update(viewers: sessions.count)
            }
            return [String: Any]()
        case "closeAll":
            closeAll()
            return [String: Any]()
        case "screenshot":
            guard let w = InputInjector.number(p["width"]), let h = InputInjector.number(p["height"]) else { throw HelperError("width and height are required") }
            return ["data": try await Capture.screenshot(display: display(p["display"]), width: Int(w), height: Int(h))]
        case "input":
            guard let event = p["event"] as? [String: Any] else { throw HelperError("event is required") }
            try await input.perform(event, display: display(p["display"]))
            return [String: Any]()
        case "uiTree":
            return try AXTree.frontmost(display: display(p["display"]))
        case "openApp":
            guard let name = p["name"] as? String, !name.isEmpty else { throw HelperError("name is required") }
            try await openApp(name)
            return [String: Any]()
        default:
            throw HelperError("unknown method \(method)")
        }
    }

    func sessionEnded(_ id: String) {
        guard let s = sessions.removeValue(forKey: id) else { return }
        s.close()
        badge.update(viewers: sessions.count)
        link.notify("session", ["session": id, "state": "closed"])
    }

    private func closeAll() {
        for (id, s) in sessions {
            s.close()
            link.notify("session", ["session": id, "state": "closed"])
        }
        sessions = [:]
        badge.update(viewers: 0)
    }

    /// Runs this binary with `--probe-capture`: a new process reads the current grant.
    private static func captureGrantedNow() async -> Bool {
        guard let exe = Bundle.main.executableURL else { return false }
        let p = Process()
        p.executableURL = exe
        p.arguments = ["--probe-capture"]
        return await withCheckedContinuation { c in
            p.terminationHandler = { c.resume(returning: $0.terminationStatus == 0) }
            do { try p.run() } catch { c.resume(returning: false) }
        }
    }

    private func display(_ v: Any?) -> CGDirectDisplayID {
        (v as? NSNumber)?.uint32Value ?? CGMainDisplayID()
    }

    private func openApp(_ name: String) async throws {
        let p = Process()
        p.executableURL = URL(filePath: "/usr/bin/open")
        p.arguments = ["-a", name]
        let err = Pipe()
        p.standardError = err
        try p.run()
        await withCheckedContinuation { c in p.terminationHandler = { _ in c.resume() } }
        guard p.terminationStatus == 0 else { throw HelperError("Couldn't find an app named \(name).") }
    }

    // MARK: status

    private func sendStatus() {
        var ids = [CGDirectDisplayID](repeating: 0, count: 16)
        var count: UInt32 = 0
        CGGetActiveDisplayList(UInt32(ids.count), &ids, &count)
        let main = CGMainDisplayID()
        let displays: [[String: AnyHashable]] = ids.prefix(Int(count)).map { id in
            let b = CGDisplayBounds(id)
            let name = NSScreen.screens.first { ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == id }?.localizedName
            return ["id": Int(id), "name": name ?? "Display", "width": b.width, "height": b.height, "main": id == main]
        }
        let status: [String: AnyHashable] = [
            "platform": "macos",
            "version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "",
            "displays": displays,
            "capture": CGPreflightScreenCaptureAccess(),
            "input": AXIsProcessTrusted(),
        ]
        guard status != lastStatus, link.isConnected else { return }
        lastStatus = status
        link.notify("status", status)
    }
}

@main
enum ScreenHelperMain {
    @MainActor static var helper: ScreenHelper?

    @MainActor
    static func main() {
        if CommandLine.arguments.contains("--probe-capture") { exit(CGPreflightScreenCaptureAccess() ? 0 : 1) }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let helper = ScreenHelper()
        Self.helper = helper
        helper.start()
        app.run()
    }
}
