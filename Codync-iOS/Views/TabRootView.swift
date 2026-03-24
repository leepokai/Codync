import SwiftUI
import CodyncShared

// TODO: Re-enable TabView with Cowork and Codex tabs once those features are implemented.
// enum AppTab: String, CaseIterable {
//     case claudeCode
//     case cowork
//     case codex
// }

struct TabRootView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager

    // private let iconSize: CGFloat = 22

    var body: some View {
        // TODO: Restore TabView when Cowork / Codex tabs are ready.
        // TabView(selection: $selectedTab) {
        //     Tab(value: AppTab.claudeCode.rawValue) { ... }
        //     Tab(value: AppTab.cowork.rawValue) {
        //         ComingSoonView(icon: "CoworkIcon", isSystemImage: false,
        //             title: "Cowork", description: "Monitor Claude Cowork sessions in real time")
        //     }
        //     Tab(value: AppTab.codex.rawValue) {
        //         ComingSoonView(icon: "CodexIcon", isSystemImage: false,
        //             title: "Codex", description: "Track OpenAI Codex jobs and costs")
        //     }
        // }
        NavigationStack {
            IOSSessionListView(
                sessions: sessions,
                liveActivityManager: liveActivityManager,
                primarySessionManager: primarySessionManager
            )
        }
    }
}
