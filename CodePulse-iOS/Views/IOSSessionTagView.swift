import SwiftUI

struct SessionTagView: View {
    let tag: String
    @Environment(\.theme) private var theme

    var body: some View {
        Text(tag)
            .font(.system(size: 10, weight: .medium, design: .monospaced))
            .foregroundStyle(theme.tertiaryText)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(theme.border)
            )
    }
}
