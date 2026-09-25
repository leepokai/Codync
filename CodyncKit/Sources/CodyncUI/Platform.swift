import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

enum Pasteboard {
    static func copy(_ text: String?) {
        guard let text else { return }
        #if canImport(UIKit)
        UIPasteboard.general.string = text
        #else
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}

extension View {
    func inlineNavigationTitle() -> some View {
        #if os(iOS)
        navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }

    func plainTextInput() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never).autocorrectionDisabled()
        #else
        autocorrectionDisabled()
        #endif
    }

    @ViewBuilder func groupedList() -> some View {
        #if os(iOS)
        listStyle(.insetGrouped)
        #else
        listStyle(.inset)
        #endif
    }
}

extension View {
    /// Liquid Glass where the OS has it (content scrolls visibly underneath), a material before that.
    @ViewBuilder func glass(in shape: some Shape) -> some View {
        if #available(iOS 26, macOS 26, *) {
            glassEffect(.regular, in: shape)
        } else {
            background(.ultraThinMaterial, in: shape)
        }
    }
}
