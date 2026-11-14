import CodyncKit
import SwiftUI

extension View {
    /// Records `event` each time this view appears, and again whenever it changes while shown
    /// (a tab or a step); nil records nothing.
    public func tracked(_ event: AnalyticsEvent?) -> some View {
        onAppear { if let event { Analytics.shared.capture(event) } }
            .onChange(of: event) { _, event in if let event { Analytics.shared.capture(event) } }
    }

    /// Records `link_opened` for a link opened inside this view (`Link`, a link in `Text`); the
    /// system still opens it.
    public func trackedLinks(_ link: AnalyticsEvent.Link) -> some View {
        environment(\.openURL, OpenURLAction { _ in
            Analytics.shared.capture(.linkOpened(link))
            return .systemAction
        })
    }
}
