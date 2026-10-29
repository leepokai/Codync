// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodyncKit",
    platforms: [.iOS("26.0")],
    products: [
        .library(name: "CodyncKit", targets: ["CodyncKit"]),
        .library(name: "CodyncUI", targets: ["CodyncUI"]),
    ],
    dependencies: [
        // libwebrtc for the remote screen viewer (hardware H.264 over WebRTC) and realtime voice calls.
        .package(url: "https://github.com/stasel/WebRTC.git", exact: "153.0.0"),
        // Terminal emulator for agent install and sign-in. 1.19+ adds a build-tool
        // plugin every Xcode build would have to trust.
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", exact: "1.18.0"),
    ],
    targets: [
        .target(name: "CodyncKit", resources: [
            .copy("Resources/ThirdPartyNotices"),
            .copy("Resources/Languages/zh-Hans.json"),
            .copy("Resources/Languages/zh-Hant.json"),
            .copy("Resources/Languages/ja.json"),
            .copy("Resources/Languages/ko.json"),
            .copy("Resources/Languages/fr.json"),
            .copy("Resources/Languages/de.json"),
            .copy("Resources/Languages/es.json"),
            .process("Resources/ProviderIcons.xcassets"),
        ]),
        .target(
            name: "CodyncUI",
            dependencies: [
                "CodyncKit",
                .product(name: "WebRTC", package: "WebRTC"),
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            resources: [.process("Resources")]
        ),
        .testTarget(name: "CodyncKitTests", dependencies: ["CodyncKit"], resources: [.copy("Fixtures")]),
        .testTarget(name: "CodyncUITests", dependencies: ["CodyncUI", "CodyncKit"]),
    ]
)
