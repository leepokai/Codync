import CodyncKit
import SwiftUI

/// Every avatar shape in the chosen color, then every color, as wrapping, centered grids
/// (the New bot dialog and the bot's Settings).
struct AvatarPicker: View {
    @Binding var shape: String
    @Binding var color: String

    var body: some View {
        VStack(spacing: 16) {
            WrapLayout(spacing: 6) {
                ForEach(AvatarPalette.shapes, id: \.self) { s in
                    choice("\(s) shape", chosen: s == shape) { shape = s } label: {
                        CharacterAvatar(shape: s, color: color, size: 40)
                    }
                }
            }
            WrapLayout(spacing: 6) {
                ForEach(AvatarPalette.colors) { c in
                    choice(c.label, chosen: c.id == color) { color = c.id } label: {
                        Circle().fill(c.color).frame(width: 28, height: 28)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func choice(_ title: String, chosen: Bool, pick: @escaping () -> Void, @ViewBuilder label: () -> some View) -> some View {
        Button { withAnimation(Motion.hover) { pick() } } label: { label().choiceRing(chosen) }
            .buttonStyle(.plain)
            .accessibilityLabel(title)
            .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

extension View {
    /// The accent ring around a chosen shape, color or group member.
    func choiceRing(_ chosen: Bool) -> some View {
        padding(4).background(Circle().strokeBorder(chosen ? Palette.accent : .clear, lineWidth: 2))
    }
}

/// Rows that wrap, each centered, split evenly (8 shapes are two rows of 4, not 7 and 1).
// ponytail: even split assumes same-width items (avatars, swatches, member tiles); mixed widths could overflow a row.
struct WrapLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = rows(subviews, width: proposal.width ?? .infinity)
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in rows(subviews, width: bounds.width) {
            var x = bounds.midX - row.width / 2
            for i in row.items {
                subviews[i].place(at: CGPoint(x: x, y: y), proposal: .unspecified)
                x += subviews[i].sizeThatFits(.unspecified).width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row {
        var items: Range<Int>
        var width: CGFloat
        var height: CGFloat
    }

    private func rows(_ subviews: Subviews, width: CGFloat) -> [Row] {
        let sizes = subviews.map { $0.sizeThatFits(.unspecified) }
        return Self.split(sizes.map(\.width), spacing: spacing, width: width).map { items in
            Row(items: items,
                width: sizes[items].map(\.width).reduce(0, +) + spacing * CGFloat(items.count - 1),
                height: sizes[items].map(\.height).max() ?? 0)
        }
    }

    /// Each row's items: as many rows as a greedy fill needs, the items shared out evenly between them.
    static func split(_ widths: [CGFloat], spacing: CGFloat, width: CGFloat) -> [Range<Int>] {
        var count = 0, used: CGFloat = 0
        for w in widths {
            if count == 0 || used + spacing + w > width {
                count += 1
                used = w
            } else {
                used += spacing + w
            }
        }
        guard count > 0 else { return [] }
        let perRow = (widths.count + count - 1) / count
        return stride(from: 0, to: widths.count, by: perRow).map { $0..<min($0 + perRow, widths.count) }
    }
}
