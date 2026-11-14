import SwiftUI
import UIKit

/// The share sheet's Codync (`NSExtensionPrincipalClass`): shows `ShareSheet`, then ends the request.
final class ShareViewController: UIViewController, UIAdaptivePresentationControllerDelegate {
    private var share: ShareModel?

    override func viewDidLoad() {
        super.viewDidLoad()
        let items = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
        let share = ShareModel(items: items) { [weak self] sent in self?.end(sent: sent) }
        self.share = share
        let host = UIHostingController(rootView: ShareSheet(share: share))
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        presentationController?.delegate = self
    }

    /// A swipe down closes it like the close button, but never mid-send.
    func presentationControllerShouldDismiss(_ presentationController: UIPresentationController) -> Bool {
        share?.isSending != true
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        share?.cancel()
    }

    private func end(sent: Bool) {
        if sent { extensionContext?.completeRequest(returningItems: nil) }
        else { extensionContext?.cancelRequest(withError: CocoaError(.userCancelled)) }
    }
}
