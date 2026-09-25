import CodyncKit
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

extension View {
    /// The message box: Liquid Glass on iPhone; on the Mac a white box with a
    /// hairline border and a soft shadow (as in Grok Bot's desktop app).
    @ViewBuilder func composerSurface(in shape: some Shape) -> some View {
        #if os(macOS)
        background(Palette.bubbleUser, in: shape)
            .overlay(shape.stroke(Palette.border))
            .shadow(color: .black.opacity(0.06), radius: 10, y: 2)
        #else
        glass(in: shape)
        #endif
    }

    /// Mac: Return sends, Shift-Return adds a new line. iPhone keeps Return as a new line.
    @ViewBuilder func sendOnReturn(_ send: @escaping () -> Void) -> some View {
        #if os(macOS)
        onKeyPress(.return, phases: .down) { press in
            guard !press.modifiers.contains(.shift) else { return .ignored }
            send()
            return .handled
        }
        #else
        self
        #endif
    }
}
