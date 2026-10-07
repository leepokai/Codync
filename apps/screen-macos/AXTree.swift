import AppKit
import ApplicationServices

/// What `type_login` needs to know about the field it would type into.
@MainActor
enum AXTree {
    /// The focused control of app `pid` (the frontmost app by default): whether it's a password
    /// field, and the URL of the web page it sits in (if any), so the host types a login only
    /// where it belongs. An app keeps its focused control while in the background.
    static func focusedField(pid: pid_t?) throws -> [String: Any] {
        guard AXIsProcessTrusted() else {
            throw HelperError("Codync Screen isn't allowed to read the screen's controls yet. Allow it under Privacy & Security → Accessibility.")
        }
        let app = pid.flatMap { NSRunningApplication(processIdentifier: $0) } ?? NSWorkspace.shared.frontmostApplication
        guard let app else { throw HelperError(pid == nil ? "No app is in front." : "No app is running with pid \(pid ?? 0).") }
        let root = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetMessagingTimeout(root, 1)
        // Chromium only exposes web content to assistive apps that ask for it.
        AXUIElementSetAttributeValue(root, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        var out: [String: Any] = ["app": app.localizedName ?? "", "pid": Int(app.processIdentifier), "secure": false]
        guard let focused: AXUIElement = attribute(root, kAXFocusedUIElementAttribute) else { return out }
        let role: String? = attribute(focused, kAXRoleAttribute)
        let subrole: String? = attribute(focused, kAXSubroleAttribute)
        out["secure"] = role == "AXSecureTextField" || subrole == kAXSecureTextFieldSubrole
        var el = focused
        for _ in 0..<60 {
            if (attribute(el, kAXRoleAttribute) as String?) == "AXWebArea" {
                if let url: URL = attribute(el, "AXURL") { out["url"] = url.absoluteString }
                break
            }
            guard let parent: AXUIElement = attribute(el, kAXParentAttribute) else { break }
            el = parent
        }
        return out
    }

    private static func attribute<T>(_ el: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success else { return nil }
        return value as? T
    }
}
