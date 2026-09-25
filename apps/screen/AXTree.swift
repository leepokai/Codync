import AppKit
import ApplicationServices

/// The frontmost app's accessibility tree, compact enough to hand to an agent.
/// Frames are `[x, y, w, h]` in points relative to the display's top-left corner.
@MainActor
enum AXTree {
    private static let maxNodes = 600
    private static let maxDepth = 30
    /// Wrappers with nothing to say and a single child are skipped.
    private static let passThrough: Set<String> = ["AXGroup", "AXSplitGroup", "AXScrollArea", "AXLayoutArea", "AXUnknown"]

    static func frontmost(display: CGDirectDisplayID) throws -> [String: Any] {
        guard AXIsProcessTrusted() else {
            throw HelperError("Codync Screen isn't allowed to read the screen's controls yet. Allow it under Privacy & Security → Accessibility.")
        }
        guard let app = NSWorkspace.shared.frontmostApplication else { throw HelperError("No app is in front.") }
        let root = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetMessagingTimeout(root, 1)
        var budget = maxNodes
        let origin = CGDisplayBounds(display).origin
        let windows: [AXUIElement] = attribute(root, kAXWindowsAttribute) ?? []
        let tree: [String: Any] = [
            "role": "AXApplication",
            "title": app.localizedName ?? "",
            "children": windows.compactMap { node($0, depth: 0, budget: &budget, origin: origin) },
        ]
        return ["app": app.localizedName ?? "", "tree": tree, "truncated": budget <= 0]
    }

    private static func node(_ el: AXUIElement, depth: Int, budget: inout Int, origin: CGPoint) -> [String: Any]? {
        guard budget > 0, depth < maxDepth else { return nil }
        budget -= 1
        var n: [String: Any] = [:]
        let role: String = attribute(el, kAXRoleAttribute) ?? "AXUnknown"
        n["role"] = role
        for (key, attr) in [("title", kAXTitleAttribute), ("description", kAXDescriptionAttribute), ("help", kAXHelpAttribute), ("id", kAXIdentifierAttribute)] {
            if let s: String = attribute(el, attr), !s.isEmpty { n[key] = String(s.prefix(200)) }
        }
        if let v: Any = attribute(el, kAXValueAttribute) {
            if let s = v as? String, !s.isEmpty { n["value"] = String(s.prefix(200)) } else if let num = v as? NSNumber { n["value"] = num }
        }
        if let enabled: Bool = attribute(el, kAXEnabledAttribute), !enabled { n["enabled"] = false }
        if let focused: Bool = attribute(el, kAXFocusedAttribute), focused { n["focused"] = true }
        if let frame = frame(el) {
            n["frame"] = [frame.minX - origin.x, frame.minY - origin.y, frame.width, frame.height].map { ($0 * 10).rounded() / 10 }
        }
        let kids: [AXUIElement] = attribute(el, kAXChildrenAttribute) ?? []
        let children = kids.compactMap { node($0, depth: depth + 1, budget: &budget, origin: origin) }
        if passThrough.contains(role), children.count == 1, n["title"] == nil, n["description"] == nil, n["value"] == nil {
            return children[0]
        }
        if !children.isEmpty { n["children"] = children }
        return n
    }

    private static func frame(_ el: AXUIElement) -> CGRect? {
        guard let pos: AXValue = attribute(el, kAXPositionAttribute), let size: AXValue = attribute(el, kAXSizeAttribute) else { return nil }
        var p = CGPoint.zero, s = CGSize.zero
        guard AXValueGetValue(pos, .cgPoint, &p), AXValueGetValue(size, .cgSize, &s) else { return nil }
        return CGRect(origin: p, size: s)
    }

    private static func attribute<T>(_ el: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success else { return nil }
        return value as? T
    }
}
