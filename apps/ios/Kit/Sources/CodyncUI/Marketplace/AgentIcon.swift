import CodyncKit
import SwiftUI
import UIKit

/// A coding agent's logo, by ACP registry id or by Codync's id for agents outside the registry,
/// tinted like text; a terminal glyph otherwise.
public struct AgentIcon: View {
    @Environment(BotStore.self) private var store
    let registry: String?
    let size: CGFloat

    public init(registry: String?, size: CGFloat = 20) {
        self.registry = registry
        self.size = size
    }

    public var body: some View {
        Group {
            if let custom = customIcon, custom.image != nil {
                CustomIconView(icon: custom, size: size)
            } else if let name = registry.map({ "agent-\($0)" }), Self.exists(name) {
                Image(name, bundle: .module).resizable().scaledToFit()
            } else {
                Image(systemName: "terminal").resizable().scaledToFit().padding(size * 0.1)
            }
        }
        .frame(width: size, height: size)
        .foregroundStyle(tint)
    }

    private var customIcon: CustomIcon? {
        guard let registry else { return nil }
        let backend = store.hello?.backends.first { $0.id == registry || $0.registry == registry }
        return store.icons["agent:\(backend?.id ?? registry)"]
    }

    /// Registry logos are one-color; paint the ones with a known brand color in it.
    private var tint: AnyShapeStyle {
        switch registry {
        case "claude-acp": AnyShapeStyle(Color(red: 0.85, green: 0.47, blue: 0.34))
        case "gemini": AnyShapeStyle(LinearGradient(colors: [Color(red: 0.28, green: 0.59, blue: 0.89), Color(red: 0.57, green: 0.47, blue: 0.78), Color(red: 0.79, green: 0.40, blue: 0.45)], startPoint: .bottomLeading, endPoint: .topTrailing))
        case "antigravity-acp": AnyShapeStyle(Color(red: 0.26, green: 0.52, blue: 0.96))
        case "mistral-vibe": AnyShapeStyle(Color(red: 0.98, green: 0.32, blue: 0.06))
        case "qwen-code": AnyShapeStyle(Color(red: 0.38, green: 0.36, blue: 0.93))
        case "amp-acp": AnyShapeStyle(Color(red: 0.95, green: 0.31, blue: 0.25))
        case "kiro": AnyShapeStyle(Color(red: 0.56, green: 0.27, blue: 1.0))
        case "dsh": AnyShapeStyle(Color(red: 0.30, green: 0.42, blue: 1.0))
        case "cortex-code": AnyShapeStyle(Color(red: 0.16, green: 0.71, blue: 0.91))
        case "omp": AnyShapeStyle(LinearGradient(colors: [Color(red: 0.93, green: 0.29, blue: 0.75), Color(red: 0.61, green: 0.30, blue: 1.0), Color(red: 0.35, green: 0.85, blue: 0.90)], startPoint: .topLeading, endPoint: .bottomTrailing))
        default: AnyShapeStyle(Palette.text)
        }
    }

    /// The logo alone, for places that take an `Image` (the system menu).
    public static func image(registry: String?) -> Image {
        if let name = registry.map({ "agent-\($0)" }), exists(name) { Image(name, bundle: .module) } else { Image(systemName: "terminal") }
    }

    private static func exists(_ name: String) -> Bool {
        UIImage(named: name, in: .module, with: nil) != nil
    }
}
