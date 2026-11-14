import CodyncKit
import CodyncUI
import SwiftUI

/// "Add a widget", acted out on a drawn iPhone: hold the Home Screen, Edit, Add Widget, find Codync,
/// add it, Done. Every frame is a pure function of one clock, so it loops, pauses and scrubs.
struct WidgetSetupDemo: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.stateSurfaceActive) private var active
    @State private var playing = true
    /// The clock reads `base` at `since`, then runs on while playing.
    @State private var base: Double = 0
    @State private var since = Date.now
    @State private var scrubbing = false

    static let duration: Double = 9.5

    var body: some View {
        let running = playing && !scrubbing && active
        VStack(spacing: 14) {
            TimelineView(.animation(paused: !running)) { context in
                let t = time(at: context.date)
                VStack(spacing: 14) {
                    DemoPhone(t: t)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(L10n.tr("How to add the Codync widget"))
                        .accessibilityValue(L10n.tr(Self.step(at: t).text))
                    Text(L10n.tr(Self.step(at: t).text))
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(Palette.text)
                        .contentTransition(.opacity)
                        .animation(Motion.fade, value: Self.step(at: t).index)
                        .frame(maxWidth: .infinity)
                        .accessibilityHidden(true)
                    controls(t)
                }
            }
        }
        .onAppear { if reduceMotion { pause(at: 7.8) } }
        // Hidden tabs keep their place: the clock stops with them and picks up where it was.
        .onChange(of: active) { _, on in
            if on {
                since = .now
            } else if playing, !scrubbing {
                base = (base + Date.now.timeIntervalSince(since)).truncatingRemainder(dividingBy: Self.duration)
            }
        }
    }

    // MARK: clock

    private func time(at date: Date) -> Double {
        guard playing, !scrubbing, active else { return base }
        return (base + date.timeIntervalSince(since)).truncatingRemainder(dividingBy: Self.duration)
    }

    private func pause(at t: Double) {
        base = t
        playing = false
    }

    private func controls(_ t: Double) -> some View {
        HStack(spacing: 10) {
            IconButton(playing ? "Pause" : "Play", systemImage: playing ? "pause.fill" : "play.fill") {
                if playing {
                    pause(at: t)
                } else {
                    base = t
                    since = .now
                    playing = true
                }
            }
            GeometryReader { geo in
                let x = geo.size.width * t / Self.duration
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.accentDim).frame(height: 4)
                    Capsule().fill(Palette.accent).frame(width: x, height: 4)
                    Circle().fill(Palette.accent)
                        .frame(width: scrubbing ? 18 : 14, height: scrubbing ? 18 : 14)
                        .offset(x: x - (scrubbing ? 9 : 7))
                        .animation(Motion.press, value: scrubbing)
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            scrubbing = true
                            base = min(max(value.location.x / geo.size.width, 0), 0.999) * Self.duration
                        }
                        .onEnded { _ in
                            since = .now
                            scrubbing = false
                        }
                )
            }
            .frame(height: 32)
            .accessibilityElement()
            .accessibilityLabel(L10n.tr("Step"))
            .accessibilityValue(L10n.tr(Self.step(at: t).text))
            .accessibilityAdjustableAction { direction in
                let i = Self.step(at: t).index + (direction == .increment ? 1 : -1)
                guard Self.steps.indices.contains(i) else { return }
                pause(at: Self.steps[i].start + 0.05)
            }
        }
    }

    // MARK: script

    struct Step {
        let index: Int
        let start: Double
        let text: String
    }

    static let steps: [Step] = [
        Step(index: 0, start: 0, text: "Touch and hold an empty spot on the Home Screen"),
        Step(index: 1, start: 1.8, text: "Tap Edit, then Add Widget"),
        Step(index: 2, start: 3.0, text: "Find Codync in the list"),
        Step(index: 3, start: 4.5, text: "Tap Add Widget"),
        Step(index: 4, start: 6.0, text: "Tap Done"),
    ]

    static func step(at t: Double) -> Step { steps.last { t >= $0.start } ?? steps[0] }
}

extension EnvironmentValues {
    /// False while the State tab or its Widget page is hidden, so the demo stops drawing.
    @Entry var stateSurfaceActive = true
}

// MARK: - The phone

/// 0 before `a`, 1 after `b`, smooth in between.
private func ramp(_ t: Double, _ a: Double, _ b: Double) -> Double {
    let x = min(max((t - a) / (b - a), 0), 1)
    return x * x * (3 - 2 * x)
}

/// Ease-out for things that arrive (sheets, pages).
private func arrive(_ t: Double, _ a: Double, _ b: Double) -> Double {
    let x = min(max((t - a) / (b - a), 0), 1)
    return 1 - pow(1 - x, 3)
}

private struct DemoPhone: View {
    let t: Double
    @Environment(\.colorScheme) private var scheme

    private static let size = CGSize(width: 184, height: 384)
    private static let widgetHeight: CGFloat = 84

    // Where the finger goes and when it taps (screen points).
    private static let path: [(t: Double, p: CGPoint)] = [
        (0.2, CGPoint(x: 150, y: 300)), (0.9, CGPoint(x: 128, y: 262)), (1.8, CGPoint(x: 128, y: 262)),
        (2.2, CGPoint(x: 28, y: 40)), (2.45, CGPoint(x: 28, y: 40)), (2.8, CGPoint(x: 52, y: 66)),
        (3.1, CGPoint(x: 52, y: 66)), (3.9, CGPoint(x: 92, y: 177)), (4.5, CGPoint(x: 92, y: 177)),
        (5.1, CGPoint(x: 92, y: 334)), (6.2, CGPoint(x: 92, y: 334)), (6.8, CGPoint(x: 156, y: 40)),
        (7.6, CGPoint(x: 156, y: 40)),
    ]
    private static let taps: [Double] = [2.35, 2.95, 4.3, 5.75, 7.2]

    private var jiggle: Double { ramp(t, 1.45, 1.65) * (1 - ramp(t, 7.2, 7.35)) }
    private var menu: Double { ramp(t, 2.4, 2.55) * (1 - ramp(t, 2.95, 3.1)) }
    private var sheet: Double { arrive(t, 3.0, 3.5) * (1 - ramp(t, 5.8, 6.2)) }
    private var detail: Double { arrive(t, 4.4, 4.85) }
    private var placed: Double { arrive(t, 5.95, 6.4) }

    var body: some View {
        ZStack(alignment: .topLeading) {
            wallpaper
            homeScreen
            editButtons
            editMenu
            gallery
            finger
            statusBar
        }
        .frame(width: Self.size.width, height: Self.size.height)
        .clipShape(RoundedRectangle(cornerRadius: 30, style: .continuous))
        .padding(5)
        .background(Color.black, in: RoundedRectangle(cornerRadius: 35, style: .continuous))
        .environment(\.colorScheme, .dark)
        .frame(maxWidth: .infinity)
    }

    private var wallpaper: some View {
        LinearGradient(colors: [Color(red: 0.16, green: 0.33, blue: 0.43), Color(red: 0.29, green: 0.53, blue: 0.47)],
                       startPoint: .top, endPoint: .bottom)
            // The Home Screen dims behind the widget gallery.
            .overlay(Color.black.opacity(0.35 * sheet))
    }

    private var statusBar: some View {
        ZStack {
            Capsule().fill(.black).frame(width: 54, height: 16)
            HStack {
                Text(L10n.tr("9:41")).font(.system(size: 9, weight: .semibold))
                Spacer()
                Image(systemName: "battery.75percent").font(.system(size: 9))
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 20)
        }
        .frame(width: Self.size.width, height: 30)
    }

    // MARK: home

    private static let apps: [(String, Color)] = [
        ("message.fill", .green), ("calendar", .red), ("photo.fill", .orange), ("camera.fill", .gray),
        ("map.fill", .teal), ("clock.fill", .black), ("cloud.sun.fill", .blue), ("note.text", .yellow),
        ("music.note", .pink), ("book.fill", .orange), ("heart.fill", .red), ("gearshape.fill", .gray),
    ]
    private static let dock: [(String, Color)] = [("phone.fill", .green), ("safari.fill", .blue), ("envelope.fill", .blue), ("music.note", .red)]

    private var homeScreen: some View {
        VStack(spacing: 0) {
            BotsWidgetCard(bots: Bot.widgetPreview, wide: true)
                .padding(12)
                .frame(width: 164 / 0.62, height: Self.widgetHeight / 0.62)
                .background(Color(white: 0.11), in: RoundedRectangle(cornerRadius: 22 / 0.62, style: .continuous))
                .scaleEffect(0.62)
                .frame(width: 164, height: Self.widgetHeight)
                .scaleEffect(0.9 + 0.1 * placed, anchor: .top)
                .rotationEffect(.degrees(jiggle * 1.2 * sin(t * 30 + 5)))
                .opacity(placed)
                // The icons below slide down as the widget takes its row.
                .frame(height: (Self.widgetHeight + 14) * placed, alignment: .top)
            LazyVGrid(columns: Array(repeating: GridItem(.fixed(34), spacing: 10), count: 4), spacing: 12) {
                ForEach(Self.apps.indices, id: \.self) { i in
                    icon(Self.apps[i], index: i)
                }
            }
            Spacer(minLength: 0)
            HStack(spacing: 10) {
                ForEach(Self.dock.indices, id: \.self) { i in icon(Self.dock[i], index: i + 20, label: false) }
            }
            .padding(9)
            .background(.white.opacity(0.18), in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .padding(.bottom, 8)
        }
        .padding(.top, 58)
        .frame(width: Self.size.width, height: Self.size.height)
        .scaleEffect(1 - 0.04 * sheet)
    }

    private func icon(_ app: (String, Color), index: Int, label: Bool = true) -> some View {
        VStack(spacing: 3) {
            Image(systemName: app.0)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(.white)
                .frame(width: 34, height: 34)
                .background(app.1.gradient, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            if label { Capsule().fill(.white.opacity(0.55)).frame(width: 20, height: 3) }
        }
        .rotationEffect(.degrees(jiggle * 2.4 * sin(t * 32 + Double(index) * 1.7)))
    }

    // MARK: edit mode

    private var editButtons: some View {
        HStack {
            pill("Edit", highlighted: pressed(near: 2.35))
            Spacer()
            pill("Done", highlighted: pressed(near: 7.2))
        }
        .padding(.horizontal, 12)
        .padding(.top, 30)
        .frame(width: Self.size.width)
        .opacity(jiggle)
    }

    private func pill(_ title: String, highlighted: Bool) -> some View {
        Text(L10n.tr(title))
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 9)
            .frame(height: 20)
            .background(.white.opacity(highlighted ? 0.45 : 0.25), in: Capsule())
    }

    private var editMenu: some View {
        VStack(alignment: .leading, spacing: 0) {
            menuRow("Add Widget", "plus.rectangle.on.rectangle", highlighted: pressed(near: 2.95))
            Rectangle().fill(.white.opacity(0.12)).frame(height: 0.5)
            menuRow("Customize", "paintbrush", highlighted: false)
        }
        .frame(width: 104)
        .background(Color(white: 0.16), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .scaleEffect(0.6 + 0.4 * menu, anchor: .topLeading)
        .opacity(menu)
        .offset(x: 12, y: 54)
    }

    private func menuRow(_ title: String, _ symbol: String, highlighted: Bool) -> some View {
        HStack(spacing: 6) {
            Text(L10n.tr(title)).font(.system(size: 9, weight: .medium))
            Spacer(minLength: 0)
            Image(systemName: symbol).font(.system(size: 9))
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 10)
        .frame(height: 24)
        .background(.white.opacity(highlighted ? 0.12 : 0))
    }

    // MARK: widget gallery

    private var gallery: some View {
        ZStack(alignment: .topLeading) {
            galleryList
                .offset(x: -40 * detail)
                .opacity(1 - detail)
            galleryDetail
                .offset(x: Self.size.width * (1 - detail))
        }
        .frame(width: Self.size.width, height: Self.size.height - 60, alignment: .top)
        .background(Color(white: 0.12), in: UnevenRoundedRectangle(topLeadingRadius: 20, topTrailingRadius: 20, style: .continuous))
        .clipShape(UnevenRoundedRectangle(topLeadingRadius: 20, topTrailingRadius: 20, style: .continuous))
        .offset(y: 60 + (Self.size.height - 60) * (1 - sheet))
    }

    private var galleryList: some View {
        VStack(spacing: 0) {
            Capsule().fill(.white.opacity(0.3)).frame(width: 28, height: 3).padding(.vertical, 6)
            HStack(spacing: 5) {
                Image(systemName: "magnifyingglass")
                Text(L10n.tr("Search Widgets"))
                Spacer()
            }
            .font(.system(size: 9))
            .foregroundStyle(.white.opacity(0.5))
            .padding(.horizontal, 9)
            .frame(height: 22)
            .background(.white.opacity(0.1), in: Capsule())
            .padding(.horizontal, 12)
            .padding(.bottom, 10)
            listRow("Batteries", Image(systemName: "battery.100percent"), tint: .green)
            listRow("Calendar", Image(systemName: "calendar"), tint: .red)
            listRow("Codync", nil, tint: .clear, highlighted: pressed(near: 4.3))
            listRow("Fitness", Image(systemName: "figure.run"), tint: .orange)
            listRow("Weather", Image(systemName: "cloud.sun.fill"), tint: .blue)
        }
    }

    private func listRow(_ title: String, _ symbol: Image?, tint: Color, highlighted: Bool = false) -> some View {
        HStack(spacing: 8) {
            Group {
                if let symbol {
                    symbol.font(.system(size: 10)).foregroundStyle(.white)
                        .frame(width: 20, height: 20)
                        .background(tint.gradient, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                } else {
                    CharacterAvatar(shape: "hex", color: "gray", size: 20)
                }
            }
            Text(L10n.tr(title)).font(.system(size: 10, weight: .medium)).foregroundStyle(.white)
            Spacer()
            Image(systemName: "chevron.right").font(.system(size: 7, weight: .semibold)).foregroundStyle(.white.opacity(0.35))
        }
        .padding(.horizontal, 12)
        .frame(height: 28)
        .background(.white.opacity(highlighted ? 0.1 : 0))
    }

    private var galleryDetail: some View {
        VStack(spacing: 12) {
            Capsule().fill(.white.opacity(0.3)).frame(width: 28, height: 3).padding(.top, 6)
            HStack(spacing: 6) {
                CharacterAvatar(shape: "hex", color: "gray", size: 18)
                Text(L10n.tr("Codync")).font(.system(size: 11, weight: .semibold)).foregroundStyle(.white)
            }
            Text(L10n.tr("See who needs you and who's still working."))
                .font(.system(size: 8)).foregroundStyle(.white.opacity(0.6))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 20)
            BotsWidgetCard(bots: Bot.widgetPreview, wide: true)
                .padding(12)
                .frame(width: 164 / 0.62, height: Self.widgetHeight / 0.62)
                .background(Color(white: 0.2), in: RoundedRectangle(cornerRadius: 22 / 0.62, style: .continuous))
                .scaleEffect(0.62)
                .frame(width: 164, height: Self.widgetHeight)
                .padding(.top, 8)
            HStack(spacing: 4) {
                ForEach(0..<3) { i in Circle().fill(.white.opacity(i == 1 ? 0.9 : 0.3)).frame(width: 4, height: 4) }
            }
            Spacer()
            Label(L10n.tr("Add Widget"), systemImage: "plus")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.black)
                .frame(width: 140, height: 28)
                .background(.white.opacity(pressed(near: 5.75) ? 0.75 : 1), in: Capsule())
                .scaleEffect(pressed(near: 5.75) ? 0.96 : 1)
                .padding(.bottom, 26)
        }
        .frame(width: Self.size.width)
    }

    // MARK: finger

    private func pressed(near tap: Double) -> Bool { abs(t - tap) < 0.12 }

    private var fingerPoint: CGPoint {
        let path = Self.path
        guard let next = path.firstIndex(where: { $0.t > t }) else { return path.last!.p }
        guard next > 0 else { return path[0].p }
        let a = path[next - 1], b = path[next]
        let k = ramp(t, a.t, b.t)
        return CGPoint(x: a.p.x + (b.p.x - a.p.x) * k, y: a.p.y + (b.p.y - a.p.y) * k)
    }

    private var finger: some View {
        let visible = ramp(t, 0.15, 0.4) * (1 - ramp(t, 7.5, 7.8))
        let tapDepth = Self.taps.map { max(0, 1 - abs(t - $0) / 0.14) }.max() ?? 0
        let holding = ramp(t, 0.95, 1.05) * (1 - ramp(t, 1.65, 1.75))
        let hold = ramp(t, 1.0, 1.6)
        let p = fingerPoint
        return ZStack {
            // The long press fills a ring before edit mode starts.
            Circle().trim(from: 0, to: hold * holding)
                .stroke(.white.opacity(0.8), style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .frame(width: 34, height: 34)
            Circle().fill(.white.opacity(0.35 + 0.25 * max(tapDepth, holding)))
                .overlay(Circle().stroke(.white.opacity(0.85), lineWidth: 1.5))
                .frame(width: 24, height: 24)
                .scaleEffect(1 - 0.2 * max(tapDepth, holding))
        }
        .opacity(visible)
        .position(p)
        .frame(width: Self.size.width, height: Self.size.height, alignment: .topLeading)
    }
}
