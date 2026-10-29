import CodyncKit
import CodyncUI
import SwiftUI
import VisionKit

/// Pair with a computer running codync-host, one thing per page: install it, then scan its code.
struct PairingView<Leading: View>: View {
    /// Adding another computer from the computers sheet: has a close button; closes once paired.
    var inModal = false
    /// Finish onboarding without pairing; modal pairing simply closes when skipped.
    var onSkip: (() -> Void)? = nil
    /// Top-left on the first page (back to the welcome, the account switcher).
    @ViewBuilder var leading: Leading

    private enum Step { case install, scan }
    @State private var step = Step.install
    @State private var os = ComputerOS.mac
    @Environment(\.dismissModal) private var dismissModal
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                if step == .scan { BackButton { go(.install) } } else { leading }
            } title: {
                HStack(spacing: 6) {
                    Capsule().fill(Palette.text).frame(width: 18, height: 4)
                    Capsule().fill(step == .scan ? Palette.text : Palette.accentDim).frame(width: 18, height: 4)
                }
                .accessibilityElement()
                .accessibilityLabel(step == .install ? "Step 1 of 2" : "Step 2 of 2")
            } trailing: {
                if inModal { IconButton("Close", systemImage: "xmark") { dismissModal() } }
            }

            ZStack {
                switch step {
                case .install:
                    InstallPage(os: $os) { go(.scan) }
                        .transition(.move(edge: .leading).combined(with: .opacity))
                case .scan:
                    ScanPage(os: os) { if inModal { dismissModal() } }
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            }
            .frame(maxHeight: .infinity)
            .clipped()

            if step == .scan {
                Button {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        if inModal { dismissModal() } else { onSkip?() }
                    }
                } label: {
                    Text(L10n.tr("Skip")).font(.headline).frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.secondary)
                .accessibilityHint("Pair a computer later from Manage computers.")
                .padding(.horizontal, 24)
                .padding(.bottom, 12)
                .transition(.opacity)
            }
        }
        .background(Palette.background)
        // First run, each page is an onboarding step; afterwards, the whole sheet is one screen.
        .tracked(inModal ? .screenViewed(.pairing) : .onboardingStepViewed(step == .install ? .install : .scan))
    }

    private func go(_ next: Step) {
        withAnimation(Motion.reduced(.spring(duration: 0.45, bounce: 0), reduceMotion)) { step = next }
    }
}

extension PairingView where Leading == EmptyView {
    init(inModal: Bool) { self.init(inModal: inModal) { EmptyView() } }
}

private enum ComputerOS: Hashable { case mac, linux }

// MARK: - Install

private struct InstallPage: View {
    @Binding var os: ComputerOS
    let next: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // A bot on the computer's screen: that's where it lives.
                    ZStack {
                        Image(systemName: "laptopcomputer")
                            .font(.system(size: 150, weight: .ultraLight))
                            .foregroundStyle(Palette.tertiary)
                        CharacterAvatar(shape: "blob", color: "blue", size: 52, mood: .working)
                            .offset(y: -12)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 8)

                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.tr("Install Codync\non your computer"))
                            .font(.system(size: 30, weight: .semibold))
                            .tracking(-0.5)
                            .foregroundStyle(Palette.text)
                        Text(L10n.tr("Your bots run there. This iPhone is how you talk to them."))
                            .font(.body)
                            .foregroundStyle(Palette.secondary)
                    }

                    SegmentedChoice(selection: $os, options: [(.mac, "Mac"), (.linux, "Linux")])

                    VStack(alignment: .leading, spacing: 10) {
                        // Copied here, pasted on the computer (Universal Clipboard or a message to yourself).
                        if os == .mac {
                            Hint("Install with Homebrew")
                            CommandBlock("brew install --cask leepokai/codync/codync", records: .installCommand)
                            Hint("or download the Mac app")
                            CommandBlock("https://github.com/leepokai/Codync/releases/latest/download/codync-macos.dmg", records: .installCommand)
                        } else {
                            Hint("Run the installer")
                            CommandBlock("curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh\ncodync-host install", records: .installCommand)
                            Hint("or install with Homebrew")
                            CommandBlock("brew install leepokai/codync/codync-host\ncodync-host install", records: .installCommand)
                        }
                        Text(L10n.tr(os == .mac
                             ? "Open Codync and it sets up the host on its own."
                             : "codync-host install keeps it running in the background."))
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .contentTransition(.opacity)
                        Hint("Every install option")
                        CommandBlock("https://www.codync.dev/#install", records: .installCommand)
                    }
                    .animation(Motion.layout, value: os)

                    DemoButton()
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 16)
            }

            Button(action: next) {
                Text(L10n.tr("Continue")).font(.headline).frame(maxWidth: .infinity, minHeight: 50)
            }
            .buttonStyle(.primary)
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
        }
    }
}

/// A small label above a `CommandBlock`.
private struct Hint: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(L10n.tr(text)).font(.footnote).foregroundStyle(Palette.secondary).transition(.opacity)
    }
}

/// A terminal command with a copy button; copying it counts as opening `records`.
private struct CommandBlock: View {
    let command: String
    let records: AnalyticsEvent.Link?
    @State private var copied = false

    init(_ command: String, records: AnalyticsEvent.Link? = nil) {
        self.command = command
        self.records = records
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(command)
                .font(.footnote.monospaced())
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 6)
                .contentTransition(.opacity)
            IconButton(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                UIPasteboard.general.string = command
                if let records { Analytics.shared.capture(.linkOpened(records)) }
                withAnimation(Motion.morph) { copied = true }
                Task {
                    try? await Task.sleep(for: .seconds(1.5))
                    withAnimation(Motion.morph) { copied = false }
                }
            }
            .contentTransition(.symbolEffect(.replace))
        }
        .padding(.leading, 14)
        .padding(.trailing, 4)
        .padding(.vertical, 4)
        .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 12))
        .animation(Motion.fade, value: command)
    }
}

// MARK: - Scan

private struct ScanPage: View {
    let os: ComputerOS
    let paired: () -> Void
    @Environment(AppStore.self) private var app
    @Environment(AccountSession.self) private var account
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.openURL) private var openURL
    @State private var pairing = false
    @State private var pasted = ""
    @State private var error: String?
    /// Bumped after a failed pair so the scanner starts looking again.
    @State private var attempt = 0
    @State private var tailscaleOn = Tailscale.isConnected

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(L10n.tr("Scan the pairing code"))
                        .font(.system(size: 30, weight: .semibold))
                        .tracking(-0.5)
                        .foregroundStyle(Palette.text)
                    Text(os == .mac
                         ? L10n.tr("On your Mac, click Codync in the menu bar, then Pair iPhone.")
                         : L10n.tr("On your computer, run this in a terminal:"))
                        .font(.body)
                        .foregroundStyle(Palette.secondary)
                    if os == .linux { CommandBlock("codync-host pair") }
                }

                viewfinder

                HStack {
                    TextField(L10n.tr("Or paste a {scheme}://pair link").replacingOccurrences(of: "{scheme}", with: AppEnvironment.current.urlScheme), text: $pasted)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.callout.monospaced())
                        .onSubmit { pair(pasted) }
                    IconButton("Pair", systemImage: "arrow.right") { pair(pasted) }
                        .disabled(pasted.isEmpty || pairing)
                }
                .padding(.leading, 14)
                .padding(.trailing, 4)
                .padding(.vertical, 4)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))

                if let error {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(error).font(.footnote).foregroundStyle(Palette.danger)
                        // The usual cause: the phone has no road to the computer yet.
                        Label(L10n.tr("Check that this iPhone and the computer are on the same Wi-Fi or both on Tailscale, or that “Reach from anywhere” is on in Codync on the computer."),
                              systemImage: "wifi.exclamationmark")
                            .font(.footnote)
                            .foregroundStyle(Palette.secondary)
                    }
                    .transition(.opacity)
                }

                // Signed in, account computers show up in Computers, where a tap asks one for access.
                if account.isConfigured && !account.isSignedIn { SignInButtons() }

                // How it connects is never a choice here: every paired computer gets every route.
                VStack(alignment: .leading, spacing: 8) {
                    Label(L10n.tr(RelayCopy.pairing(pasted: pasted)), systemImage: "lock.fill")
                    if tailscaleOn {
                        Label(L10n.tr("Tailscale is on: Codync connects over it directly."), systemImage: "checkmark.circle.fill")
                    } else {
                        Button {
                            Analytics.shared.capture(.linkOpened(.tailscale))
                            openURL(Tailscale.downloadURL)
                        } label: {
                            Label(L10n.tr("Use Tailscale? Direct and faster away from home."), systemImage: "arrow.up.right")
                        }
                        .buttonStyle(PressScale())
                    }
                }
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 24)
        }
        .scrollDismissesKeyboard(.interactively)
        // Coming back from the Tailscale app: show whether it's connected now.
        .onChange(of: scenePhase) { _, phase in if phase == .active { tailscaleOn = Tailscale.isConnected } }
    }

    /// The camera, right on the page: no extra sheet between the instruction and the scan.
    private var viewfinder: some View {
        ZStack {
            if DataScannerViewController.isSupported {
                QRScanner { code in pair(code) }
                    .id(attempt)
                // VisionKit's own hint follows the app's localizations, which stay English.
                if !pairing {
                    Text(L10n.tr("Find nearby barcodes."))
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .shadow(color: .black.opacity(0.55), radius: 8)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 20)
                        .allowsHitTesting(false)
                }
            } else {
                VStack(spacing: 10) {
                    Image(systemName: "qrcode.viewfinder").font(.system(size: 44, weight: .light))
                    Text(L10n.tr("Camera not available. Paste the link below.")).font(.footnote)
                }
                .foregroundStyle(Palette.tertiary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.surface)
            }
            if pairing {
                Palette.background.opacity(0.7)
                Spinner(size: 28)
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
        .animation(Motion.fade, value: pairing)
    }

    private func pair(_ text: String) {
        let p: Pairing
        do {
            p = try app.pairing(from: text.trimmingCharacters(in: .whitespacesAndNewlines))
        } catch {
            show(error)
            return
        }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { error = nil }
        pairing = true
        Task {
            defer { pairing = false }
            do {
                _ = try await app.pair(p, via: .qr)
                pasted = ""
                paired()
            } catch {
                show(error)
            }
        }
    }

    private func show(_ error: Error) {
        attempt += 1
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { self.error = error.localizedDescription }
    }
}

/// VisionKit live QR scanner.
struct QRScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let vc = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .fast,
            isGuidanceEnabled: false,
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
                if let value = code.payloadStringValue, value.hasPrefix(AppEnvironment.current.urlScheme + "://"), !done {
                    done = true
                    scanner.stopScanning()
                    onCode(value)
                }
            }
        }
    }
}
