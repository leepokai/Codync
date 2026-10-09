import AppKit
import ApplicationServices
import CoreGraphics

/// System prompts are explicit onboarding actions, never a side effect of launching the helper.
@MainActor
enum ScreenPermissions {
    static func request(_ permission: String) throws {
        switch permission {
        case "capture":
            if !CGPreflightScreenCaptureAccess() { _ = CGRequestScreenCaptureAccess() }
        case "input":
            if !AXIsProcessTrusted() {
                // The C global for this option is not available under Swift 6 concurrency.
                _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
            }
        default:
            throw HelperError("Unknown computer access permission.")
        }
    }
}
