import Foundation
import SwiftUI
import Testing
@testable import CodyncKit

/// Renders the Live Activity, Dynamic Island and widget cards to PNG contact sheets for design
/// review (the simulator's screenshots don't show Live Activities). Runs only when asked:
/// `TEST_RUNNER_CODYNC_RENDER_DIR=<dir> xcodebuild test … -only-testing:CodyncKitTests/renderSurfaces`
@MainActor @Test func renderSurfaces() throws {
    guard let dir = ProcessInfo.processInfo.environment["CODYNC_RENDER_DIR"] else { return }
    let bot = Bot.widgetPreview[0]
    let started = Date.now - 154
    let phases: [(String, BotActivityPresentation)] = [
        ("Sending", .init(status: "sending", activity: "")),
        ("Waiting for computer", .init(status: "queued", activity: "")),
        ("Working", .init(status: "working", activity: "Running the test suite.")),
        ("Needs you", .init(status: "needsInput", activity: "Review the proposed changes.")),
        ("Done", .init(status: "idle", activity: "")),
        ("Failed", .init(status: "error", activity: "")),
        ("Update delayed", .init(status: "working", activity: "", isStale: true)),
    ]

    func label(_ text: String) -> some View {
        Text(text).font(.system(size: 11, weight: .medium)).foregroundStyle(Palette.secondary)
    }

    for scheme in [ColorScheme.light, .dark] {
        let activities = VStack(alignment: .leading, spacing: 28) {
            ForEach(phases, id: \.0) { name, state in
                VStack(alignment: .leading, spacing: 10) {
                    label(name)
                    HStack(alignment: .top, spacing: 20) {
                        BotActivityPreview(bot: bot, state: state, form: .lockScreen, startedAt: started).frame(width: 360)
                        VStack(spacing: 10) {
                            BotActivityPreview(bot: bot, state: state, form: .compact, startedAt: started)
                            BotActivityPreview(bot: bot, state: state, form: .minimal, startedAt: started)
                        }
                        BotActivityPreview(bot: bot, state: state, form: .expanded, startedAt: started).frame(width: 370)
                    }
                }
            }
        }
        try save(activities, scheme: scheme, to: "\(dir)/activity-\(scheme == .dark ? "dark" : "light").png")

        let widgets = HStack(alignment: .top, spacing: 20) {
            VStack(alignment: .leading, spacing: 8) {
                label("Team")
                BotsTeamCard(bots: Bot.widgetPreview).padding(14).frame(width: 170, height: 170)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
            }
            VStack(alignment: .leading, spacing: 8) {
                label("Bots · Small")
                BotsWidgetCard(bots: Bot.widgetPreview, wide: false).padding(14).frame(width: 170, height: 170)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
            }
            VStack(alignment: .leading, spacing: 8) {
                label("Bots · Medium")
                BotsWidgetCard(bots: Bot.widgetPreview, wide: true).padding(14).frame(width: 364, height: 170)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 22))
            }
        }
        try save(widgets, scheme: scheme, to: "\(dir)/widgets-\(scheme == .dark ? "dark" : "light").png")
    }
}

@MainActor private func save(_ view: some View, scheme: ColorScheme, to path: String) throws {
    let renderer = ImageRenderer(content: view.padding(28).background(Palette.background).environment(\.colorScheme, scheme))
    renderer.scale = 2
    let data = try #require(renderer.uiImage?.pngData())
    try data.write(to: URL(fileURLWithPath: path))
}

/// Writes the Dynamic Island's working loops (the bot in every shape and the thinking orb,
/// 10 frames each, in points of a 20 pt face) for tools/timer-fonts.py:
/// `TEST_RUNNER_CODYNC_FRAMES_FILE=<file.json> xcodebuild test … -only-testing:'CodyncKitTests/exportBotFrames()'`.
/// Each font is one layer: `Ink` every dot of the bot, `Tint` its lit dots (in its color),
/// `Ghost` the orb's resting grain, `Dot` its inked dots (a font has no per-dot opacity).
@MainActor @Test func exportBotFrames() throws {
    guard let file = ProcessInfo.processInfo.environment["CODYNC_FRAMES_FILE"] else { return }
    func dots(_ list: [(x: Double, y: Double, r: Double)]) -> [[String: Double]] {
        list.map { ["x": $0.x, "y": $0.y, "r": $0.r] }
    }
    var fonts: [String: [[[String: Double]]]] = [:]
    for shape in ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"] {
        let loop = DottedBody.workingLoop(shape: shape, size: 20)
        fonts["CodyncBot-\(shape)-Ink"] = loop.map { f in dots(f.filter { !$0.eye }.map { ($0.dot.center.x, $0.dot.center.y, $0.radius) }) }
        fonts["CodyncBot-\(shape)-Tint"] = loop.map { f in
            dots(f.filter { !$0.eye && $0.tint > 0.35 }.map { ($0.dot.center.x, $0.dot.center.y, $0.radius) })
        }
    }
    let orb = ThinkingOrbGeometry.workingLoop(size: 20)
    fonts["CodyncOrb-Ghost"] = orb.map { f in dots(f.filter { $0.alpha < 0.6 }.map { ($0.x, $0.y, $0.radius) }) }
    fonts["CodyncOrb-Dot"] = orb.map { f in dots(f.filter { $0.alpha >= 0.6 }.map { ($0.x, $0.y, $0.radius) }) }
    let data = try JSONSerialization.data(withJSONObject: ["size": 20, "fonts": fonts], options: [.sortedKeys])
    try data.write(to: URL(fileURLWithPath: file))
}

/// The loop is ten frames (one per second of a timer digit) and actually moves.
@MainActor @Test func workingLoopHasTenDistinctFrames() {
    let frames = DottedBody.workingLoop(shape: "blob", size: 20)
    #expect(frames.count == 10)
    #expect(frames[0].map(\.radius) != frames[3].map(\.radius))
}
