import SwiftUI

// Codync's page and card layout: shared by the app (CodyncUI) and the share extension.

// MARK: - Forms

/// A scrolling page of cards: the replacement for `Form` / grouped `List`.
public struct CardForm<Content: View>: View {
    let content: Content

    public init(@ViewBuilder content: () -> Content) { self.content = content() }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                content
            }
            .font(.body)
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
    }
}

/// A titled group of rows: the replacement for `Section`. Modeled on ChatGPT's desktop settings:
/// a small bold heading over a filled, rounded group whose rows are split by inset hairlines
/// (references in `docs/design/reference/`).
public struct CardSection<Content: View, Accessory: View>: View {
    let title: String?
    let footer: String?
    let content: Content
    /// Trailing controls on the heading's line (add, refresh, clear).
    let accessory: Accessory
    @Environment(\.cardStyle) private var cardStyle

    public init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content,
                @ViewBuilder accessory: () -> Accessory) {
        self.title = title
        self.footer = footer
        self.content = content()
        self.accessory = accessory()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let title {
                HStack(spacing: 8) {
                    Text(L10n.tr(title))
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(Palette.text)
                        .accessibilityAddTraits(.isHeader)
                    Spacer(minLength: 0)
                    accessory
                }
                .padding(.leading, 2)
            }
            // No rows (`{}`): a footer-only note, without an empty card.
            if Content.self != EmptyView.self {
                VStack(alignment: .leading, spacing: 0) {
                    _VariadicView.Tree(HairlineRows()) { content }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(cardStyle == .filled ? Palette.surface : .clear, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay {
                    if cardStyle == .outlined {
                        RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.border)
                    }
                }
            }
            if let footer {
                Text(L10n.tr(footer))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

public extension CardSection where Accessory == EmptyView {
    init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title, footer: footer, content: content) { EmptyView() }
    }
}

/// A 1-pixel separator line in the border color.
public struct Hairline: View {
    @Environment(\.displayScale) private var scale

    public init() {}

    public var body: some View {
        Rectangle().fill(Palette.border).frame(height: 1 / scale)
    }
}

/// Lays a section's rows out one under another with an inset hairline between each pair.
private struct HairlineRows: _VariadicView_MultiViewRoot {
    func body(children: _VariadicView.Children) -> some View {
        let inset: CGFloat = 16
        ForEach(children) { child in
            child
                .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                .padding(.horizontal, inset)
                .padding(.vertical, 14)
            if child.id != children.last?.id { Hairline().padding(.horizontal, inset) }
        }
    }
}

/// How `CardSection` draws its group: filled (Settings style), or outlined on the page like Grok
/// Bot's routine cards (the bot details sheet).
public enum CardStyle: Sendable {
    case filled, outlined
}

public extension EnvironmentValues {
    @Entry var cardStyle: CardStyle = .filled
}
