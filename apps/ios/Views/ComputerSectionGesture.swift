import SwiftUI
import UIKit

/// A shared hit area keeps short taps, scrolling and long-press reordering distinct.
struct ComputerSectionGesture: ViewModifier {
    let tap: () -> Void
    let drag: (UIGestureRecognizer.State, CGFloat) -> Void
    @GestureState private var holding = false
    @State private var dragging = false

    func body(content: Content) -> some View {
        content
            .contentShape(Rectangle())
            .highPriorityGesture(hold.simultaneously(with: TapGesture().onEnded {
                if !dragging { tap() }
            }))
            .onChange(of: holding) { _, held in
                if !held && dragging {
                    dragging = false
                    drag(.cancelled, 0)
                }
            }
    }

    private var hold: some Gesture {
        LongPressGesture(minimumDuration: 0.35)
            .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .global))
            .updating($holding) { value, held, _ in
                if case .second(true, _) = value { held = true }
            }
            .onChanged { value in
                guard case let .second(true, movement?) = value else { return }
                if !dragging {
                    dragging = true
                    drag(.began, movement.startLocation.y)
                }
                drag(.changed, movement.location.y)
            }
            .onEnded { value in
                dragging = false
                if case let .second(true, movement?) = value {
                    drag(.ended, movement.location.y)
                } else { drag(.cancelled, 0) }
            }
    }
}
