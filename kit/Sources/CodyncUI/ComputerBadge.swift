import CodyncKit
import SwiftUI

/// A computer's icon showing what it is (laptop, Mac mini, Linux…), in the color picked for it:
/// the profile button and the rows that switch computers.
public struct ComputerBadge: View {
    let computer: Pairing?
    let size: CGFloat

    public init(_ computer: Pairing?, size: CGFloat = 36) {
        self.computer = computer
        self.size = size
    }

    public var body: some View {
        icon
            .foregroundStyle(computer?.color.map(AvatarPalette.color) ?? Palette.text)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }

    @ViewBuilder private var icon: some View {
        switch computer?.device {
        // SF Symbols has no penguin.
        case "linux": Image("computer-linux", bundle: .module).resizable().scaledToFit().frame(width: size * 0.55)
        case let device: Image(systemName: Self.symbol(device)).font(.system(size: size * 0.5, weight: .medium))
        }
    }

    private static func symbol(_ device: String?) -> String {
        switch device {
        case "laptop": "laptopcomputer"
        case "macmini": "macmini"
        case "macstudio": "macstudio"
        case "imac": "desktopcomputer"
        case "macpro": "macpro.gen3"
        default: "desktopcomputer"
        }
    }
}
