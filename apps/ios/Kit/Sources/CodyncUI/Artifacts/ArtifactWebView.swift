import CodyncKit
import SwiftUI
import UIKit
import WebKit

/// An artifact's HTML in a web view of its own: its scripts run, nothing of the app is reachable from it, and what it
/// opens goes to the browser. Each artifact has an origin of its own, so what it keeps (a tracker's ticks) stays its own.
struct ArtifactWebView: UIViewRepresentable {
    let artifactId: String
    let html: ArtifactCache.HTML
    /// The chat card's preview: no touches, and the artifact opens nothing.
    var interactive = true

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.userContentController.addUserScript(Self.viewport)
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.uiDelegate = context.coordinator
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        let coordinator = context.coordinator
        coordinator.interactive = interactive
        view.isUserInteractionEnabled = interactive
        let origin = Self.origin(artifactId)
        let shown = "\(artifactId)@\(html.updatedAt)"
        guard coordinator.shown != shown else { return }
        coordinator.shown = shown
        coordinator.origin = origin?.host
        view.loadHTMLString(html.text, baseURL: origin)
    }

    /// Never resolves: relative links go nowhere, and the artifact gets an origin to keep its storage under.
    static func origin(_ artifactId: String) -> URL? {
        URL(string: "https://\(artifactId.lowercased()).artifact.codync.invalid/")
    }

    /// An artifact written without a viewport would lay out 980 points wide and show tiny on a phone.
    private static let viewport = WKUserScript(source: """
        if (!document.querySelector('meta[name="viewport"]')) {
          const meta = document.createElement('meta');
          meta.name = 'viewport';
          meta.content = 'width=device-width, initial-scale=1';
          document.head.appendChild(meta);
        }
        """, injectionTime: .atDocumentEnd, forMainFrameOnly: true)

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        var interactive = true
        var shown: String?
        var origin: String?

        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
            decisionHandler(policy(for: action))
        }

        /// The artifact's own document (its #anchors too) and the frames it embeds load here; a link the user follows
        /// goes to the browser instead, and the artifact sending itself elsewhere goes nowhere.
        private func policy(for action: WKNavigationAction) -> WKNavigationActionPolicy {
            guard let url = action.request.url else { return .cancel }
            if (origin != nil && url.host == origin) || url.scheme == "about" { return .allow }
            let followed = action.navigationType == .linkActivated
            if action.targetFrame?.isMainFrame == false, !followed { return .allow }
            if followed { leave(to: url) }
            return .cancel
        }

        /// `target="_blank"` and `window.open` (WebKit lets a script open one only from a tap).
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                     for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if let url = action.request.url { leave(to: url) }
            return nil
        }

        /// Web, mail and phone links only, as on Android: another app's scheme goes nowhere.
        private func leave(to url: URL) {
            guard interactive, let scheme = url.scheme?.lowercased(), ["http", "https", "mailto", "tel"].contains(scheme)
            else { return }
            UIApplication.shared.open(url)
        }
    }
}
