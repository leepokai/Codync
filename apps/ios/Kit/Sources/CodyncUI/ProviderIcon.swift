import CodyncKit
import SwiftUI
import UIKit

/// A Codync Cloud provider's logo by the cloud's id for it, in its brand color or tinted like text; a cloud otherwise.
public struct ProviderIcon: View {
    let provider: String
    let size: CGFloat

    public init(_ provider: String, size: CGFloat = 20) {
        self.provider = provider
        self.size = size
    }

    public var body: some View {
        Group {
            if UIImage(named: asset, in: .module, with: nil) != nil {
                Image(asset, bundle: .module).resizable().scaledToFit()
            } else {
                Image(systemName: "cloud").resizable().scaledToFit()
            }
        }
        .frame(width: size, height: size)
        .foregroundStyle(tint)
        .accessibilityHidden(true)
    }

    private var asset: String { "cloud-\(provider)" }

    /// Logos are one-color; Boat's is its brand blue in both themes, like the agents' brand tints.
    private var tint: Color {
        provider == "boat" ? Color(red: 0x26 / 255, green: 0x3F / 255, blue: 0xA9 / 255) : Palette.text
    }
}
