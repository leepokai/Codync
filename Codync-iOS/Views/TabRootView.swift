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
                    .renderingMode(.template)
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
                    .renderingMode(.template)
            }
        }
    }
}
