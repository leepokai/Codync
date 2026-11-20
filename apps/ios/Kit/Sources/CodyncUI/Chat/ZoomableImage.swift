import CodyncKit
import SwiftUI

/// A picture that zooms (`ImageZoom`): pinch about the pinch point, double tap in on the spot and back to fit, and
/// drag to move it once zoomed in. `content` is laid out at its fit size; zooming only transforms it.
struct ZoomableImage<Content: View>: View {
    @Binding var zoom: ImageZoom
    @ViewBuilder let content: Content
    @State private var box = CGSize.zero
    @State private var picture = CGSize.zero
    /// The zoom when the pinch began: each step zooms from it by the pinch's whole magnification.
    @State private var pinchStart: ImageZoom?
    /// How far the fingers have dragged during the pinch: each step applies it on top of the pinch's zoom.
    @State private var pinchDrag = CGSize.zero
    /// How far the drag had gone at its last step: each step moves the picture by the difference.
    @State private var dragged: CGSize?
    // True while each gesture lasts; their end also clears the state above when the system cancels them.
    @GestureState private var pinching = false
    @GestureState private var dragging = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        content
            .onGeometryChange(for: CGSize.self) { $0.size } action: { picture = $0 }
            .scaleEffect(zoom.scale)
            .offset(zoom.offset)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .clipped()
            .onGeometryChange(for: CGSize.self) { $0.size } action: { size in
                box = size
                zoom = zoom.clamped(box: box, picture: picture)
            }
            .gesture(doubleTap)
            .simultaneousGesture(pinch)
            // At fit size a drag belongs to the sheet (swipe down closes it).
            .simultaneousGesture(pan, isEnabled: zoom.isZoomed)
            .onChange(of: pinching) { if !pinching { pinchStart = nil; pinchDrag = .zero } }
            .onChange(of: dragging) { if !dragging { dragged = nil } }
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isImage)
            .accessibilityValue(Text(zoom.scale, format: .percent.precision(.fractionLength(0))))
            .accessibilityZoomAction { action in
                zoom = zoom.zoomed(by: action.direction == .zoomIn ? 2 : 0.5, at: .zero, box: box, picture: picture)
            }
    }

    private var doubleTap: some Gesture {
        SpatialTapGesture(count: 2).onEnded { tap in
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                zoom = zoom.toggled(at: fromCenter(tap.location), box: box, picture: picture)
            }
        }
    }

    private var pinch: some Gesture {
        MagnifyGesture()
            .updating($pinching) { _, active, _ in active = true }
            .onChanged { value in
                let start = pinchStart ?? zoom
                pinchStart = start
                zoom = start.zoomed(by: value.magnification, at: fromCenter(value.startLocation), box: box, picture: picture)
                    .panned(by: pinchDrag, box: box, picture: picture)
            }
    }

    private var pan: some Gesture {
        DragGesture()
            .updating($dragging) { _, active, _ in active = true }
            .onChanged { value in
                let last = dragged ?? .zero
                dragged = value.translation
                let delta = CGSize(width: value.translation.width - last.width, height: value.translation.height - last.height)
                // Two fingers pinching also drag: the pinch's next steps keep that movement.
                if pinchStart != nil { pinchDrag = CGSize(width: pinchDrag.width + delta.width, height: pinchDrag.height + delta.height) }
                zoom = zoom.panned(by: delta, box: box, picture: picture)
            }
    }

    private func fromCenter(_ point: CGPoint) -> CGPoint {
        CGPoint(x: point.x - box.width / 2, y: point.y - box.height / 2)
    }
}
