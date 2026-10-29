import CodyncKit
import CodyncUI
import SwiftUI
import UIKit

struct AppearanceSettings: View {
    @AppStorage(Appearance.storageKey) private var appearance = Appearance.system

    var body: some View {
        CardSection("Appearance", footer: "Only changes appearance on this device.") {
            ValueRow("Theme") {
                ChoicePicker(selection: $appearance, options: Appearance.allCases.map { (id: $0, label: $0.label) })
            }
        }
    }
}

/// Apply the device preference to the window so already-presented sheets update too.
struct AppearanceWindow: UIViewRepresentable {
    let appearance: Appearance

    func makeUIView(context: Context) -> AppearanceView { AppearanceView() }

    func updateUIView(_ view: AppearanceView, context: Context) {
        view.style = switch appearance {
        case .system: .unspecified
        case .light: .light
        case .dark: .dark
        }
    }

    final class AppearanceView: UIView {
        var style: UIUserInterfaceStyle = .unspecified {
            didSet { window?.overrideUserInterfaceStyle = style }
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            window?.overrideUserInterfaceStyle = style
        }
    }
}
