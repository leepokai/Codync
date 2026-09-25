import CodyncKit
import CodyncUI
import SwiftUI
import VisionKit

/// First run: explain the model, then pair with a computer running codync-host.
struct PairingView: View {
    /// Set when adding another computer from the profile sheet; shows a close button.
    var onDone: (() -> Void)?
    @Environment(BotStore.self) private var model
    @State private var scanning = false
    @State private var pasted = ""
    @State private var error: String?
    @State private var tailscaleOn = Tailscale.isConnected
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                HStack(spacing: -10) {
                    CharacterAvatar(shape: "blob", color: "blue", size: 64, mood: .working)
                    CharacterAvatar(shape: "squircle", color: "orange", size: 64)
                    CharacterAvatar(shape: "teardrop", color: "violet", size: 64, mood: .working)
                }
                .padding(.top, 40)

                VStack(alignment: .leading, spacing: 10) {
                    Text("Your coding agents,\nas teammates.")
                        .font(.system(size: 34, weight: .semibold))
                        .tracking(-0.6)
                        .foregroundStyle(Palette.text)
                    Text("Give each agent a name, a job and a project. Then just message the right one — it works on your computer while your phone is in your pocket.")
                        .font(.body)
                        .foregroundStyle(Palette.secondary)
                }

                VStack(alignment: .leading, spacing: 14) {
                    Step(n: 1, title: "Install Codync on your computer", detail: "Mac — then open Codync in the menu bar and click Install host:", code: "brew install --cask leepokai/codync/codync")
                    Step(n: nil, title: "", detail: "Linux:", code: "brew install leepokai/codync/codync-host\ncodync-host install")
                    Step(n: 2, title: "Show the pairing code", detail: "On a Mac, click Codync in the menu bar → Pair iPhone. Or run:", code: "codync-host pair")
                    Step(n: 3, title: "Scan it", detail: "Use the button below, or point the Camera app at the code.", code: nil)
                    TailscaleStep(connected: tailscaleOn)
                }

                VStack(spacing: 12) {
                    Button {
                        scanning = true
                    } label: {
                        Label("Scan pairing code", systemImage: "qrcode.viewfinder")
                            .font(.headline)
                            .frame(maxWidth: .infinity, minHeight: 50)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Palette.accentFill)
                    .foregroundStyle(Palette.onAccent)
                    .disabled(!DataScannerViewController.isSupported)

                    HStack {
                        TextField("…or paste a codync://pair link", text: $pasted)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .font(.callout.monospaced())
                        Button("Pair", systemImage: "arrow.right.circle.fill") { pair(pasted) }
                            .labelStyle(.iconOnly)
                            .font(.title3)
                            .disabled(pasted.isEmpty)
                    }
                    .padding(12)
                    .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.border))

                    if let error {
                        Text(error).font(.footnote).foregroundStyle(Palette.danger)
                    }
                }

                Text("Your phone talks straight to your computer over Tailscale or local Wi-Fi. Codync has no account and no cloud copy of your code or chats.")
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
            }
            .padding(24)
        }
        .background(Palette.background)
        .overlay(alignment: .topLeading) {
            if let onDone {
                Button("Close", systemImage: "xmark", action: onDone)
                    .labelStyle(.iconOnly)
                    .font(.body.weight(.semibold))
                    .frame(width: 44, height: 44)
                    .background(Palette.bubbleAgent, in: Circle())
                    .foregroundStyle(Palette.text)
                    .padding(16)
            }
        }
        // Coming back from the Tailscale app: show whether it's connected now.
        .onChange(of: scenePhase) { _, phase in if phase == .active { tailscaleOn = Tailscale.isConnected } }
        .sheet(isPresented: $scanning) {
            QRScanner { code in
                scanning = false
                pair(code)
            }
            .ignoresSafeArea()
        }
    }

    private func pair(_ text: String) {
        guard let p = Pairing(string: text) else {
            error = "That isn't a Codync pairing code."
            return
        }
        error = nil
        model.pair(p)
        onDone?()
        Task { _ = await PushRegistrar.shared.requestAuthorization() }
    }
}

private struct Step: View {
    let n: Int?
    let title: String
    let detail: String
    let code: String?

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text(n.map { String(format: "%02d", $0) } ?? "")
                .font(.system(size: 12, weight: .medium, design: .monospaced))
                .frame(width: 24, alignment: .leading)
                .padding(.top, 2)
                .foregroundStyle(Palette.tertiary)
            VStack(alignment: .leading, spacing: 6) {
                if !title.isEmpty {
                    Text(title).font(.headline).foregroundStyle(Palette.text)
                }
                Text(detail).font(.subheadline).foregroundStyle(Palette.secondary)
                if let code {
                    Text(code)
                        .font(.footnote.monospaced())
                        .textSelection(.enabled)
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.border))
                }
            }
        }
    }
}

/// Optional step: Tailscale is what lets the phone reach the computer off the home Wi-Fi.
private struct TailscaleStep: View {
    let connected: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text("04")
                .font(.system(size: 12, weight: .medium, design: .monospaced))
                .frame(width: 24, alignment: .leading)
                .padding(.top, 2)
                .foregroundStyle(Palette.tertiary)
            VStack(alignment: .leading, spacing: 6) {
                Text("Away from home (optional)").font(.headline).foregroundStyle(Palette.text)
                Text("Without it, Codync works on the same Wi-Fi as your computer. Install Tailscale on this iPhone and on the computer, and sign in to both with the same account.")
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                if connected {
                    Label("Tailscale is on", systemImage: "checkmark.circle.fill")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(Palette.added)
                } else {
                    Link(destination: Tailscale.downloadURL) {
                        Label("Get Tailscale", systemImage: "arrow.down.circle")
                            .font(.subheadline.weight(.medium))
                    }
                }
            }
        }
    }
}

/// VisionKit live QR scanner.
struct QRScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let vc = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .fast,
            isHighlightingEnabled: true
        )
        vc.delegate = context.coordinator
        try? vc.startScanning()
        return vc
    }

    func updateUIViewController(_ vc: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onCode: onCode) }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onCode: (String) -> Void
        private var done = false

        init(onCode: @escaping (String) -> Void) { self.onCode = onCode }

        func dataScanner(_ scanner: DataScannerViewController, didAdd items: [RecognizedItem], allItems: [RecognizedItem]) {
            for case let .barcode(code) in items {
                if let value = code.payloadStringValue, value.hasPrefix("codync://"), !done {
                    done = true
                    scanner.stopScanning()
                    onCode(value)
                }
            }
        }
    }
}
