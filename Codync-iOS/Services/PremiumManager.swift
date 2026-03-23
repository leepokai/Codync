import Foundation
import RevenueCat
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "Premium")

@MainActor
@Observable
final class PremiumManager {
    static let shared = PremiumManager()

    fileprivate(set) var isPro = false
    private(set) var isLoaded = false

    #if DEBUG
    /// When true, isPro is forced on and RevenueCat status is ignored
    fileprivate let debugForcesPro = true
    #else
    fileprivate let debugForcesPro = false
    #endif

    private init() {}

    func configure() {
        Purchases.logLevel = .warn
        Purchases.configure(withAPIKey: "appl_dMPFOjggBowEweUmCxskLNbwHkH")
        Purchases.shared.delegate = RCPurchasesDelegate.shared
        Task {
            await refreshStatus()
        }
    }

    func refreshStatus() async {
        if debugForcesPro {
            isPro = true
            isLoaded = true
            logger.info("Premium status: Pro (debug forced)")
            return
        }
        do {
            let info = try await Purchases.shared.customerInfo()
            isPro = info.entitlements["Codync Pro"]?.isActive == true
            isLoaded = true
            logger.info("Premium status: \(self.isPro ? "Pro" : "Free")")
        } catch {
            logger.warning("Failed to fetch customer info: \(error.localizedDescription)")
            isLoaded = true
        }
    }

    func restorePurchases() async throws {
        if debugForcesPro { return }
        let info = try await Purchases.shared.restorePurchases()
        isPro = info.entitlements["Codync Pro"]?.isActive == true
        logger.info("Restored purchases: \(self.isPro ? "Pro" : "Free")")
    }
}

// MARK: - Delegate

private final class RCPurchasesDelegate: NSObject, RevenueCat.PurchasesDelegate, Sendable {
    static let shared = RCPurchasesDelegate()

    nonisolated func purchases(_ purchases: Purchases, receivedUpdated customerInfo: CustomerInfo) {
        let isPro = customerInfo.entitlements["Codync Pro"]?.isActive == true
        Task { @MainActor in
            if !PremiumManager.shared.debugForcesPro {
                PremiumManager.shared.isPro = isPro
            }
            logger.info("Customer info updated: \(isPro ? "Pro" : "Free")")
        }
    }
}
