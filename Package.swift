// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "Portkey", defaultLocalization: "en", platforms: [.macOS(.v13)], products: [.library(name: "FollowCore", targets: ["FollowCore"]), .executable(name: "Portkey", targets: ["Portkey"])], targets: [.target(name: "FollowCore", resources: [.process("Resources")]), .executableTarget(name: "Portkey", dependencies: ["FollowCore"]), .executableTarget(name: "CoreTests", dependencies: ["FollowCore"], path: "Tests/FollowCoreTests")], swiftLanguageModes: [.v5])
