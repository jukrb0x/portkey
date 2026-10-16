// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "USBFollowDisplay", platforms: [.macOS(.v13)], products: [.library(name: "FollowCore", targets: ["FollowCore"]), .executable(name: "USBFollowDisplay", targets: ["USBFollowDisplay"])], targets: [.target(name: "FollowCore"), .executableTarget(name: "USBFollowDisplay", dependencies: ["FollowCore"]), .executableTarget(name: "CoreTests", dependencies: ["FollowCore"], path: "Tests/FollowCoreTests")], swiftLanguageModes: [.v5])
