import SwiftUI

struct SessionTagView: View {
    let tag: String
    @Environment(\.theme) private var theme

    var body: some View {
        Text(tag)
            .font(.system(size: 9, weight: .medium, design: .monospaced))
            .foregroundStyle(theme.tertiaryText)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(
                RoundedRectangle(cornerRadius: 3, style: .continuous)
                    .fill(theme.border)
            )
    }
}
