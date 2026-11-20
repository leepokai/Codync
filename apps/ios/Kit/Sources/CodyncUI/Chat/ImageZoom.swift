import CoreGraphics

/// Pinch-to-zoom for a picture laid out at its fit size in a box: how big it is and how far it's moved, and the
/// moves that change that. Points and offsets are measured from the box's center. The same math as the desktop
/// viewer's `image-zoom.ts`.
struct ImageZoom: Equatable {
    var scale: CGFloat = 1
    /// How far the picture is moved from the box's center.
    var offset: CGSize = .zero

    static let fit = ImageZoom()
    static let maxScale: CGFloat = 8
    /// A double tap zooms in to this much.
    static let doubleTapScale: CGFloat = 2.5

    var isZoomed: Bool { scale > 1 }

    /// Never below fit size or past `maxScale`; a picture smaller than the box stays centered on that side, and a
    /// larger one never leaves a gap at an edge.
    func clamped(box: CGSize, picture: CGSize) -> ImageZoom {
        let scale = Self.limit(scale, 1, Self.maxScale)
        let reach = { (boxSide: CGFloat, pictureSide: CGFloat) in max(0, (pictureSide * scale - boxSide) / 2) }
        let (rx, ry) = (reach(box.width, picture.width), reach(box.height, picture.height))
        return ImageZoom(scale: scale, offset: CGSize(width: Self.limit(offset.width, -rx, rx), height: Self.limit(offset.height, -ry, ry)))
    }

    /// Zooms by `factor`, keeping the spot under `point` where it is.
    func zoomed(by factor: CGFloat, at point: CGPoint, box: CGSize, picture: CGSize) -> ImageZoom {
        let scale = Self.limit(scale * factor, 1, Self.maxScale)
        let ratio = scale / self.scale
        let offset = CGSize(width: point.x - (point.x - offset.width) * ratio, height: point.y - (point.y - offset.height) * ratio)
        return ImageZoom(scale: scale, offset: offset).clamped(box: box, picture: picture)
    }

    func panned(by delta: CGSize, box: CGSize, picture: CGSize) -> ImageZoom {
        ImageZoom(scale: scale, offset: CGSize(width: offset.width + delta.width, height: offset.height + delta.height))
            .clamped(box: box, picture: picture)
    }

    /// Where a double tap goes: back to fit when zoomed in, else in on the spot tapped.
    func toggled(at point: CGPoint, box: CGSize, picture: CGSize) -> ImageZoom {
        isZoomed ? .fit : zoomed(by: Self.doubleTapScale, at: point, box: box, picture: picture)
    }

    private static func limit(_ value: CGFloat, _ low: CGFloat, _ high: CGFloat) -> CGFloat { min(max(value, low), high) }
}
