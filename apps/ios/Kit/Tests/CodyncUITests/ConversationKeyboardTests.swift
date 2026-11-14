import Testing
import UIKit
@testable import CodyncUI

@MainActor @Test func tappingConversationEndsEditingInSiblingComposer() {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 640))
    let controller = UIViewController()
    window.rootViewController = controller
    let conversation = ConversationCollectionView(frame: .zero, collectionViewLayout: UICollectionViewFlowLayout())
    let composer = UITextView(frame: CGRect(x: 0, y: 500, width: 320, height: 80))
    controller.view.addSubview(conversation)
    controller.view.addSubview(composer)
    window.makeKeyAndVisible()
    defer { window.isHidden = true }
    composer.becomeFirstResponder()
    #expect(composer.isFirstResponder)

    conversation.dismissComposerKeyboard()

    #expect(!composer.isFirstResponder)
}

@MainActor @Test func keyboardTapFilteringDoesNotOverrideScrollGestureDelegate() {
    let conversation = ConversationCollectionView(frame: .zero, collectionViewLayout: UICollectionViewFlowLayout())
    #expect(conversation.keyboardDismissTapDelegate !== conversation.panGestureRecognizer.delegate)
}
