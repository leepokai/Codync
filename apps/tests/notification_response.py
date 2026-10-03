#!/usr/bin/env python3
"""Check the iOS notification delegate's Objective-C completion on macOS.

Uses the production delegate method with isolated routing state and synthetic
notification objects. No app data, notification delivery, or network is used.
Pass --source-ref main to demonstrate the regression in the original method.
"""
import argparse
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = "apps/ios/App/CodyncApp.swift"

HEADER = r"""
#import <Foundation/Foundation.h>
NS_ASSUME_NONNULL_BEGIN
void InvokeNotification(id delegate, NSDictionary<NSString *, NSString *> *info,
                        NSString * _Nullable malformedField,
                        void (^completion)(void));
NS_ASSUME_NONNULL_END
"""

OBJC = r"""
#import "NotificationFixture.h"
#import <UserNotifications/UserNotifications.h>

// Stand-ins for system-created objects that have no public initializer.
// Their getters match the Objective-C notification response API; the request
// and content are real UserNotifications objects with synthetic payloads.
@interface FixtureNotification : NSObject
@property(nonatomic, strong) UNNotificationRequest *request;
@end
@implementation FixtureNotification
@end
@interface FixtureResponse : NSObject
@property(nonatomic, strong) FixtureNotification *notification;
@end
@implementation FixtureResponse
@end

void InvokeNotification(id delegate, NSDictionary<NSString *, NSString *> *info,
                        NSString *malformedField,
                        void (^completion)(void)) {
    UNMutableNotificationContent *content = [UNMutableNotificationContent new];
    NSMutableDictionary *payload = [info mutableCopy];
    if (malformedField) { payload[malformedField] = @42; }
    content.userInfo = payload;
    FixtureNotification *notification = [FixtureNotification new];
    notification.request = [UNNotificationRequest requestWithIdentifier:@"fixture"
                                                                content:content trigger:nil];
    FixtureResponse *response = [FixtureResponse new];
    response.notification = notification;
    // The delegate never uses the center. Invoke the system's selector directly
    // to exercise Swift's generated Objective-C thunk, rather than await Swift.
    [delegate userNotificationCenter:nil didReceiveNotificationResponse:(id)response
               withCompletionHandler:completion];
}
"""

SWIFT = r"""
import Foundation
import UserNotifications

struct BotReference: Equatable, Sendable {
    let accountId: String?
    let computerId: String
    let botId: String
}
@MainActor final class Accounts {
    struct Storage { let id = "active" }
    let storage = Storage()
    let accountId: String? = "account"
    var selection: BotReference?
    func store(for id: String) -> Bool? { id == "known" ? true : nil }
}
@MainActor final class AppStore {
    enum Tab { case bots, state }
    static let shared = AppStore()
    let accounts = Accounts()
    var tab = Tab.state
}
@MainActor final class AppDelegate: NSObject, UNUserNotificationCenterDelegate {
__METHOD__
}
struct Completion: Equatable, Sendable {
    let main: Bool
    let routed: Bool
}
actor CompletionRecord {
    private var completions: [Completion] = []
    func record(_ completion: Completion) { completions.append(completion) }
    func snapshot() -> [Completion] { completions }
}
@MainActor enum Runner {
    static var finished = false
    static var failures = 0

    static func check() async {
        let valid = ["ctx": "active", "computerId": "known", "botId": "bot"]
        let cases: [(String, [String: String], String?, Bool)] = [
            ("valid", valid, nil, true),
            ("empty payload", [:], nil, false),
            ("missing account", ["computerId": "known", "botId": "bot"], nil, false),
            ("missing computer", ["ctx": "active", "botId": "bot"], nil, false),
            ("missing bot", ["ctx": "active", "computerId": "known"], nil, false),
            ("different account", ["ctx": "other", "computerId": "known", "botId": "bot"], nil, false),
            ("removed computer", ["ctx": "active", "computerId": "removed", "botId": "bot"], nil, false),
            ("wrong account type", valid, "ctx", false),
            ("wrong computer type", valid, "computerId", false),
            ("wrong bot type", valid, "botId", false),
        ]
        for background in [false, true] {
            for (name, info, malformedField, opens) in cases {
                let app = AppStore.shared
                app.tab = .state
                let previous = BotReference(accountId: "account", computerId: "known", botId: "previous")
                app.accounts.selection = previous
                let expected = opens ? BotReference(accountId: "account", computerId: "known", botId: "bot") : previous
                let delegate = AppDelegate()
                let record = CompletionRecord()
                let completion: @Sendable () -> Void = {
                    let main = Thread.isMainThread
                    // Inspect the route inside the callback, before UIKit could
                    // archive it, rather than after other work has run.
                    let routed = main && MainActor.assumeIsolated {
                        let app = AppStore.shared
                        return app.accounts.selection == expected && app.tab == (opens ? .bots : .state)
                    }
                    Task { await record.record(Completion(main: main, routed: routed)) }
                }
                if background {
                    let invocation = Task.detached { InvokeNotification(delegate, info, malformedField, completion) }
                    await invocation.value
                } else {
                    InvokeNotification(delegate, info, malformedField, completion)
                }
                var completions: [Completion] = []
                for _ in 0..<200 {
                    completions = await record.snapshot()
                    if !completions.isEmpty { break }
                    try? await Task.sleep(for: .milliseconds(10))
                }
                try? await Task.sleep(for: .milliseconds(20))
                completions = await record.snapshot()
                let routeCorrect = app.accounts.selection == expected && app.tab == (opens ? .bots : .state)
                let passed = completions == [Completion(main: true, routed: true)] && routeCorrect
                if !passed { failures += 1 }
                print("\(passed ? "PASS" : "FAIL") \(background ? "background" : "main") entry, \(name): completions=\(completions.count), main=\(completions.map(\.main)), routedBeforeCompletion=\(completions.map(\.routed)), routing=\(routeCorrect)")
            }
        }
        finished = true
    }
}
@main struct NotificationRegression {
    @MainActor static func main() {
        Task { await Runner.check() }
        let deadline = Date().addingTimeInterval(40)
        while !Runner.finished && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
        guard Runner.finished else { fatalError("Delegate completion timed out") }
        print("Failures: \(Runner.failures)")
        exit(Runner.failures == 0 ? 0 : 1)
    }
}
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-ref", help="Read the delegate from a Git revision")
    args = parser.parse_args()
    source = (subprocess.check_output(["git", "show", f"{args.source_ref}:{SOURCE}"], cwd=ROOT, text=True)
              if args.source_ref else (ROOT / SOURCE).read_text())
    start = source.index("    nonisolated func userNotificationCenter(")
    end = source.index("    nonisolated func userNotificationCenter(", start + 1)
    method = source[start:end].rstrip()
    with tempfile.TemporaryDirectory(prefix="codync-notification-") as temp:
        path = Path(temp)
        header = path / "NotificationFixture.h"
        objc = path / "NotificationFixture.m"
        swift = path / "NotificationRegression.swift"
        obj = path / "NotificationFixture.o"
        executable = path / "notification-regression"
        header.write_text(HEADER)
        objc.write_text(OBJC)
        swift.write_text(SWIFT.replace("__METHOD__", method))
        subprocess.run(["xcrun", "clang", "-fobjc-arc", "-Wno-nonnull", "-c", str(objc), "-o", str(obj)], check=True)
        subprocess.run(["xcrun", "swiftc", "-swift-version", "6", "-strict-concurrency=complete",
                        "-parse-as-library", "-import-objc-header", str(header), str(swift), str(obj),
                        "-framework", "Foundation", "-framework", "UserNotifications", "-o", str(executable)], check=True)
        result = subprocess.run([str(executable)], timeout=45)
        raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
