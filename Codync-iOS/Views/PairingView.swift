import CodyncKit
import CodyncUI
import SwiftUI
import VisionKit

/// First run: explain the model, then pair with a computer running codync-host.
struct PairingView: View {
    @Environment(BotStore.self) private var model
    @State private var scanning = false
    @State private var pasted = ""
    @State private var error: String?

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
                        .font(.system(size: 34, weight: .bold))
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
            Text(n.map(String.init) ?? "")
                .font(.footnote.bold())
                .frame(width: 24, height: 24)
                .background(n == nil ? .clear : Palette.accentDim, in: Circle())
                .foregroundStyle(Palette.text)
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
