import WidgetKit
import SwiftUI

@main
struct CodyncLiveActivityBundle: WidgetBundle {
    var body: some Widget {
        CodyncLiveActivityWidget()
        OverallLiveActivityWidget()
    }
}
