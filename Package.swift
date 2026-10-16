// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "USBFollowDisplay", platforms: [.macOS(.v13)], products: [.library(name: "FollowCore", targets: ["FollowCore"])], targets: [.target(name: "FollowCore"), .executableTarget(name: "CoreTests", dependencies: ["FollowCore"], path: "Tests/FollowCoreTests")], swiftLanguageModes: [.v5])
