import CodyncKit
import SwiftUI

/// A discrete slider whose stops come from the harness rather than a fixed list of effort levels.
struct ReasoningSlider: View {
    let option: AgentModels.Option
    @Binding var selection: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var index: Int? { option.values.firstIndex { $0.id == (selection ?? option.default) } }
    private var name: String { index.map { option.values[$0].name } ?? "" }

    private func choose(_ index: Int) {
        guard option.values.indices.contains(index) else { return }
        withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { selection = option.values[index].id }
    }

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 8) {
                Text(option.name).font(.caption).foregroundStyle(Palette.secondary)
                Spacer(minLength: 0)
                Text(name).font(.subheadline.weight(.medium)).foregroundStyle(Palette.text)
                    .contentTransition(.opacity)
                    .animation(Motion.reduced(Motion.fade, reduceMotion), value: name)
                Spacer(minLength: 0)
                Button { selection = nil } label: {
                    Text(L10n.tr("Default"))
                    Image(systemName: selection == nil ? "checkmark" : "arrow.counterclockwise")
                }
                .font(.caption).foregroundStyle(Palette.secondary).buttonStyle(.plain)
            }
            GeometryReader { geometry in
                let travel = max(0, geometry.size.width - 30)
                let last = max(1, option.values.count - 1)
                let x = CGFloat(index ?? 0) / CGFloat(last) * travel
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.surface).frame(height: 30)
                    if index != nil {
                        Capsule().fill(Palette.text.opacity(0.14)).frame(width: x + 30, height: 30)
                    }
                    ForEach(option.values.indices, id: \.self) { stop in
                        Circle().fill(Palette.secondary).frame(width: 4, height: 4)
                            .offset(x: 13 + CGFloat(stop) / CGFloat(last) * travel)
                    }
                    if index != nil { Circle().fill(Palette.text).frame(width: 30, height: 30).offset(x: x) }
                }
                .frame(height: 36)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onChanged { gesture in
                    guard travel > 0 else { return }
                    let position = min(1, max(0, (gesture.location.x - 15) / travel))
                    choose(Int((position * CGFloat(last)).rounded()))
                })
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(option.name)
                .accessibilityValue(name.isEmpty ? L10n.tr("Default") : name)
                .accessibilityAdjustableAction { direction in
                    switch direction {
                    case .increment: choose(min(last, (index ?? -1) + 1))
                    case .decrement: choose(max(0, (index ?? 1) - 1))
                    @unknown default: break
                    }
                }
            }.frame(height: 36)
                .animation(Motion.reduced(Motion.hover, reduceMotion), value: index)
        }
    }
}
