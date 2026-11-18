import CodyncKit
import SwiftUI
import WebKit

struct CallSearchView: View {
    let result: CallSearchResult

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Google Search")).font(.caption.weight(.semibold))
            if !result.text.isEmpty {
                ScrollView { Text(result.text).font(.footnote).frame(maxWidth: .infinity, alignment: .leading) }
                    .frame(maxHeight: 100)
            }
            SearchSuggestionsHTML(html: result.html).frame(height: 120).clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .padding(12)
        .background(Palette.background, in: RoundedRectangle(cornerRadius: 16))
    }
}

/// A nonpersistent document with scripts and background network access disabled. Links open in the browser.
private struct SearchSuggestionsHTML: UIViewRepresentable {
    let html: String
    private static let policy = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"

    func makeCoordinator() -> Coordinator { Coordinator() }
    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        view.isInspectable = false
        return view
    }
    func updateUIView(_ view: WKWebView, context: Context) {
        guard context.coordinator.html != html else { return }
        context.coordinator.html = html
        view.loadHTMLString(Self.policy + html, baseURL: nil)
    }
    static func dismantleUIView(_ view: WKWebView, coordinator: Coordinator) {
        view.stopLoading()
        view.navigationDelegate = nil
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var html: String?
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
            guard let url = action.request.url else { return .cancel }
            if action.navigationType == .linkActivated {
                if ["https", "http"].contains(url.scheme?.lowercased() ?? "") { await UIApplication.shared.open(url) }
                return .cancel
            }
            return url.absoluteString == "about:blank" ? .allow : .cancel
        }
    }
}
