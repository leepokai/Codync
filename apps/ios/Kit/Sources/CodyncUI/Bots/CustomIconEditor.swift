import CodyncKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

struct CustomIconEditor: View {
    @Environment(BotStore.self) private var store
    @Environment(\.customIconAccount) private var account
    @State private var needsSignIn = false
    let target: String
    @State private var editor = IconEditor()
    @State private var choosing = false
    @State private var url = ""
    @State private var showURL = false
    @FocusState private var urlFocused: Bool

    private var icon: CustomIcon? { store.icons[target] }
    var body: some View {
        if store.hello?.icons != nil { content }
    }

    private var content: some View {
        VStack(alignment: .center, spacing: 12) {
            Text("Custom icon").font(.subheadline.weight(.semibold))
            HStack(spacing: 12) {
                if let icon { CustomIconView(icon: icon, size: 40) }
                Button("Upload image") { requireAccount { Motion.animate { choosing = true } } }.buttonStyle(.secondary)
            }
            Button("Paste URL") { requireAccount { Motion.animate { showURL.toggle() }; urlFocused = showURL } }.buttonStyle(.secondary)
            if icon != nil || editor.busy { HStack {
                if icon != nil { Button("Use default") { save(.init(target: target, remove: true)) }.buttonStyle(.secondary) }
                if editor.busy { ThinkingOrb(size: 18) }
            } }
            if showURL {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Image URL").font(.caption).foregroundStyle(Palette.secondary)
                    HStack {
                        TextField("https://…", text: $url).textInputAutocapitalization(.never).autocorrectionDisabled().fieldBox()
                            .keyboardType(.URL).focused($urlFocused).submitLabel(.go)
                            .accessibilityLabel("Image URL").onSubmit { saveURL() }
                        Button { saveURL() } label: { Image(systemName: "arrow.up") }
                            .buttonStyle(.secondary).accessibilityLabel("Use URL")
                            .disabled(url.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                }.transition(.opacity)
            }
            if let icon {
                Toggle("Tint icon", isOn: Binding(get: { icon.tinted }, set: { save(.init(target: target, tinted: $0)) }))
                    .toggleStyle(.codync)
            }
            VStack(spacing: 2) {
                Text("PNG, JPEG, WebP up to 5 MiB; SVG up to 256 KiB.")
                Text("Saved immediately. Custom images stay still.")
            }
            .font(.caption).foregroundStyle(Palette.secondary).multilineTextAlignment(.center)
            if let error = editor.error ?? account?.error { Text(error).font(.caption).foregroundStyle(Palette.danger) }
        }
        .frame(maxWidth: .infinity)
        .disabled(editor.busy || account?.loading == true || store.connection != .online)
        .codyncDialog("Sign in required", isPresented: $needsSignIn, message: "Sign in to use custom images.") {
            [DialogAction("Continue with Apple") { account?.signInApple() },
             DialogAction("Continue with Google") { account?.signInGoogle() }]
        }
        .fileImporter(isPresented: $choosing, allowedContentTypes: [.png, .jpeg, .webP, .svg]) { result in
            switch result {
            case let .success(file): requireAccount { Task { await editor.save(.init(target: target), store: store, file: file) } }
            case let .failure(error): editor.error = error.localizedDescription
            }
        }
    }

    private func requireAccount(_ action: () -> Void) {
        guard account?.signedIn == true else { Motion.animate { needsSignIn = true }; return }
        action()
    }

    private func saveURL() {
        let value = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty, !editor.busy, store.connection == .online else { return }
        requireAccount { save(.init(target: target, url: value)) }
    }

    private func save(_ change: IconEditor.Change) { Task { await editor.save(change, store: store) } }
}
