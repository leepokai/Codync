import SwiftUI

struct PanelHeaderView: View {
    @ObservedObject var panelState: CodyncPanelState
    @AppStorage("codync_darkMode") private var isDarkMode = true
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 8) {
            Image("CodyncIcon")
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: 16, height: 16)
                .clipShape(RoundedRectangle(cornerRadius: 3, style: .continuous))

            Spacer()

            Button(action: { isDarkMode.toggle() }) {
                Image(systemName: isDarkMode ? "sun.max" : "moon")
                    .font(.system(size: 11))
                    .foregroundStyle(theme.secondaryText)
            }
            .buttonStyle(.plain)

            Button(action: { panelState.isPinned.toggle() }) {
                Image(systemName: panelState.isPinned ? "pin.fill" : "pin")
                    .font(.system(size: 11))
                    .foregroundStyle(panelState.isPinned ? theme.primaryText : theme.secondaryText)
                    .rotationEffect(.degrees(panelState.isPinned ? 0 : 45))
            }
            .buttonStyle(.plain)

            Button(action: { NSApp.terminate(nil) }) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(theme.secondaryText)
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }
}
