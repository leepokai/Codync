// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodyncKit",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodyncKit", targets: ["CodyncKit"]),
        .library(name: "CodyncUI", targets: ["CodyncUI"]),
    ],
    dependencies: [
        // libwebrtc for the remote screen viewer (hardware H.264 over WebRTC).
        .package(url: "https://github.com/stasel/WebRTC.git", exact: "153.0.0"),
        // Terminal emulator for agent install and sign-in. 1.19+ adds a build-tool
        // plugin every Xcode build would have to trust.
        .package(url: "https://github.com/migueldeicaza/SwiftTerm.git", exact: "1.18.0"),
    ],
    targets: [
        .target(name: "CodyncKit"),
        .target(
            name: "CodyncUI",
            dependencies: [
                "CodyncKit",
                .product(name: "WebRTC", package: "WebRTC", condition: .when(platforms: [.iOS])),
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            resources: [.process("Resources")]
        ),
        .testTarget(name: "CodyncKitTests", dependencies: ["CodyncKit"], resources: [.copy("Fixtures")]),
        .testTarget(name: "CodyncUITests", dependencies: ["CodyncUI", "CodyncKit"]),
    ]
)
