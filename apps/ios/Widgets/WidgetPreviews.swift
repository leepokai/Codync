#if DEBUG
import CodyncKit
import SwiftUI
import WidgetKit

// Native WidgetKit previews exercise the actual configurations, including their
// system margins and accessory families. All data here is explicitly synthetic.
#Preview(as: .systemSmall) {
    ProviderUsageWidget()
} timeline: {
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .claude)
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .codex)
    ProviderUsageEntry(date: .now, usage: Usage(), provider: .claude)
}

#Preview(as: .systemMedium) {
    ProviderUsageWidget()
} timeline: {
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .claude)
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .codex)
}

#Preview(as: .systemSmall) {
    BotsWidget()
} timeline: {
    BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true)
    BotsEntry(date: .now, bots: [], paired: true)
    BotsEntry(date: .now, bots: [], paired: false)
}

#Preview(as: .systemMedium) {
    BotsWidget()
} timeline: {
    BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true)
    BotsEntry(date: .now, bots: [], paired: true)
    BotsEntry(date: .now, bots: [], paired: false)
}

#Preview(as: .accessoryRectangular) {
    BotsWidget()
} timeline: {
    BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true)
    BotsEntry(date: .now, bots: [], paired: false)
}

#Preview(as: .accessoryCircular) {
    UsageWidget()
} timeline: {
    UsageEntry(date: .now, usage: .widgetPreview)
    UsageEntry(date: .now, usage: Usage())
}

#Preview(as: .accessoryInline) {
    UsageWidget()
} timeline: {
    UsageEntry(date: .now, usage: .widgetPreview)
    UsageEntry(date: .now, usage: Usage())
}
#endif
