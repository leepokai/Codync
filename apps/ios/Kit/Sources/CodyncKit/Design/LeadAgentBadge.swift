import SwiftUI

public struct LeadAgentBadge: View {
    let size: CGFloat
    public init(size: CGFloat) { self.size = size }

    public var body: some View {
        Image(systemName: "star.fill")
            .font(.system(size: size * 0.58, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Palette.warning, in: Circle())
            .accessibilityLabel(L10n.tr("Lead agent"))
    }
}
