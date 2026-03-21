import SwiftUI
import CodyncShared

enum AppTab: String, CaseIterable {
    case claudeCode
    case cowork
    case codex
}

struct TabRootView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @AppStorage("codync_selectedTab") private var selectedTab: String = AppTab.claudeCode.rawValue

    private let iconSize: CGFloat = 22

    var body: some View {
        TabView(selection: $selectedTab) {
            Tab(value: AppTab.claudeCode.rawValue) {
                NavigationStack {
                    IOSSessionListView(
                        sessions: sessions,
                        liveActivityManager: liveActivityManager,
                        primarySessionManager: primarySessionManager
                    )
                }
            } label: {
                Image("ClaudeIcon")
                    .resizable()
                    .renderingMode(.template)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: iconSize, height: iconSize)
            }

            Tab(value: AppTab.cowork.rawValue) {
                ComingSoonView(
                    icon: "person.2.fill",
                    isSystemImage: true,
                    title: "Cowork",
                    description: "Monitor Claude Cowork sessions in real time"
                )
            } label: {
                Image(systemName: "person.2.fill")
                    .font(.system(size: iconSize))
            }

            Tab(value: AppTab.codex.rawValue) {
                ComingSoonView(
                    icon: "CodexColorIcon",
                    isSystemImage: false,
                    title: "Codex",
                    description: "Track OpenAI Codex jobs and costs"
                )
            } label: {
                Image("CodexIcon")
                    .resizable()
                    .renderingMode(.template)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: iconSize, height: iconSize)
            }
        }
    }
}
