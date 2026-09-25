import CoreGraphics

/// Which part of a remote display the viewer shows, and the mapping between
/// view points and display points. Zoom 1 fits the whole display; beyond that
/// the visible region takes the view's aspect and is kept on the display.
public struct ScreenViewport: Equatable, Sendable {
    /// Display size in points.
    public var display: CGSize
    /// Viewer size in points.
    public var view: CGSize
    public private(set) var zoom: CGFloat = 1
    /// The display point at the middle of the view.
    public private(set) var center: CGPoint
    /// Closest zoom: one display point spans this many view points.
    private static let maxPointScale: CGFloat = 4

    public init(display: CGSize, view: CGSize) {
        self.display = display
        self.view = view
        center = CGPoint(x: display.width / 2, y: display.height / 2)
    }

    private var fitScale: CGFloat {
        // Before layout the view is 0×0: any positive scale keeps the math finite.
        guard display.width > 0, display.height > 0, view.width > 0, view.height > 0 else { return 1 }
        return min(view.width / display.width, view.height / display.height)
    }

    public var maxZoom: CGFloat { max(1, Self.maxPointScale / max(fitScale, 0.0001)) }

    /// View points per display point.
    public var scale: CGFloat { fitScale * zoom }

    /// The display region the view covers (may extend past the display when letterboxed).
    public var visible: CGRect {
        let size = CGSize(width: view.width / scale, height: view.height / scale)
        return CGRect(x: center.x - size.width / 2, y: center.y - size.height / 2, width: size.width, height: size.height)
    }

    public var displayBounds: CGRect { CGRect(origin: .zero, size: display) }

    /// The region to stream: `nil` while the whole display is in view.
    public var crop: CGRect? {
        zoom <= 1.001 ? nil : visible.intersection(displayBounds)
    }

    public func toDisplay(_ p: CGPoint) -> CGPoint {
        let v = visible
        return CGPoint(x: v.minX + p.x / scale, y: v.minY + p.y / scale)
    }

    public func toView(_ p: CGPoint) -> CGPoint {
        let v = visible
        return CGPoint(x: (p.x - v.minX) * scale, y: (p.y - v.minY) * scale)
    }

    /// Where a streamed region (display points) sits in the view.
    public func frame(of region: CGRect) -> CGRect {
        let o = toView(region.origin)
        return CGRect(x: o.x, y: o.y, width: region.width * scale, height: region.height * scale)
    }

    /// Pinch: zoom by `factor` keeping the display point under `anchor` (view point) in place.
    public mutating func zoom(by factor: CGFloat, at anchor: CGPoint) {
        let fixed = toDisplay(anchor)
        zoom = min(max(zoom * factor, 1), maxZoom)
        center = CGPoint(x: fixed.x - (anchor.x - view.width / 2) / scale, y: fixed.y - (anchor.y - view.height / 2) / scale)
        clamp()
    }

    /// Moves the view by a drag of `delta` view points.
    public mutating func pan(by delta: CGPoint) {
        center = CGPoint(x: center.x - delta.x / scale, y: center.y - delta.y / scale)
        clamp()
    }

    /// Scrolls just enough to keep `p` (display point) inside the inner 80% of the view.
    public mutating func follow(_ p: CGPoint) {
        let v = visible
        let mx = v.width * 0.1, my = v.height * 0.1
        var c = center
        if p.x < v.minX + mx { c.x -= v.minX + mx - p.x } else if p.x > v.maxX - mx { c.x += p.x - (v.maxX - mx) }
        if p.y < v.minY + my { c.y -= v.minY + my - p.y } else if p.y > v.maxY - my { c.y += p.y - (v.maxY - my) }
        center = c
        clamp()
    }

    public mutating func reset() {
        zoom = 1
        center = CGPoint(x: display.width / 2, y: display.height / 2)
    }

    private mutating func clamp() {
        let v = visible.size
        center.x = v.width >= display.width ? display.width / 2 : min(max(center.x, v.width / 2), display.width - v.width / 2)
        center.y = v.height >= display.height ? display.height / 2 : min(max(center.y, v.height / 2), display.height - v.height / 2)
    }
}
