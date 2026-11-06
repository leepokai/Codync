import CodyncKit
import SwiftUI

extension View {
    /// Shared presentations and editing suspend the continuous result-reading timer.
    func reviewInterruption(active: Bool) -> some View {
        modifier(ReviewInterruption(active: active))
    }
}

private struct ReviewInterruption: ViewModifier {
    let active: Bool
    @State private var token = UUID()

    func body(content: Content) -> some View {
        content
            .onAppear { ReviewInvitation.shared.setInterrupted(token, active: active) }
            .onChange(of: active) { _, active in ReviewInvitation.shared.setInterrupted(token, active: active) }
            .onDisappear { ReviewInvitation.shared.setInterrupted(token, active: false) }
    }
}
