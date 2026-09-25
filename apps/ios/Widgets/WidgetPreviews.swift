#if DEBUG
import CodyncKit
import SwiftUI
import WidgetKit
import ActivityKit

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

#Preview(as: .systemLarge) {
    ProviderUsageWidget()
} timeline: {
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .claude)
    ProviderUsageEntry(date: .now, usage: .widgetPreview, provider: .codex)
}

#Preview(as: .systemLarge) {
    BotsWidget()
} timeline: {
    BotsEntry(date: .now, bots: Bot.widgetPreview, paired: true)
    BotsEntry(date: .now, bots: [], paired: true)
}

private extension BotActivityAttributes {
    static var preview: Self { .init(bot: Bot.widgetPreview[0], link: URL(string: "codync://computers")) }
}

#Preview("Activity · Lock Screen", as: .content, using: BotActivityAttributes.preview) {
    BotLiveActivity()
} contentStates: {
    BotActivityAttributes.ContentState(status: "working", activity: "Running the test suite.", startedAt: .now - 154)
    BotActivityAttributes.ContentState(status: "needsInput", activity: "Review the proposed changes.", startedAt: .now - 154)
    BotActivityAttributes.ContentState(status: "idle", activity: "", startedAt: nil)
    BotActivityAttributes.ContentState(status: "error", activity: "", startedAt: nil)
}

#Preview("Island · Compact", as: .dynamicIsland(.compact), using: BotActivityAttributes.preview) {
    BotLiveActivity()
} contentStates: {
    BotActivityAttributes.ContentState(status: "working", activity: "Running tests.", startedAt: .now - 154)
    BotActivityAttributes.ContentState(status: "needsInput", activity: "Review changes.", startedAt: nil)
    BotActivityAttributes.ContentState(status: "error", activity: "", startedAt: nil)
    BotActivityAttributes.ContentState(status: "idle", activity: "", startedAt: nil)
}

#Preview("Island · Minimal", as: .dynamicIsland(.minimal), using: BotActivityAttributes.preview) {
    BotLiveActivity()
} contentStates: {
    BotActivityAttributes.ContentState(status: "working", activity: "Running tests.", startedAt: .now - 154)
    BotActivityAttributes.ContentState(status: "needsInput", activity: "Review changes.", startedAt: nil)
    BotActivityAttributes.ContentState(status: "error", activity: "", startedAt: nil)
    BotActivityAttributes.ContentState(status: "idle", activity: "", startedAt: nil)
}

#Preview("Island · Expanded", as: .dynamicIsland(.expanded), using: BotActivityAttributes.preview) {
    BotLiveActivity()
} contentStates: {
    BotActivityAttributes.ContentState(status: "working", activity: "Running the test suite.", startedAt: .now - 154)
    BotActivityAttributes.ContentState(status: "needsInput", activity: "Review the proposed changes.", startedAt: nil)
    BotActivityAttributes.ContentState(status: "error", activity: "", startedAt: nil)
    BotActivityAttributes.ContentState(status: "idle", activity: "", startedAt: nil)
}
#endif
