import SwiftUI

struct ComingSoonView: View {
    let icon: String
    let isSystemImage: Bool
    let title: String
    let description: String
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 16) {
            if isSystemImage {
                Image(systemName: icon)
                    .font(.system(size: 48))
                    .foregroundStyle(theme.tertiaryText)
            } else {
                Image(icon)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 48, height: 48)
                    .foregroundStyle(theme.tertiaryText)
            }
            Text(title)
                .font(.title2.bold())
                .foregroundStyle(theme.primaryText)
            Text(description)
                .font(.subheadline)
                .foregroundStyle(theme.secondaryText)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
            Text("Coming Soon")
                .font(.caption.bold())
                .foregroundStyle(theme.tertiaryText)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(theme.tertiaryText.opacity(0.1), in: Capsule())
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.background)
    }
}
