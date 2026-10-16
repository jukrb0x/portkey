// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "PortKey", defaultLocalization: "en", platforms: [.macOS(.v13)], products: [.library(name: "FollowCore", targets: ["FollowCore"]), .executable(name: "PortKey", targets: ["PortKey"])], targets: [.target(name: "FollowCore", resources: [.process("Resources")]), .executableTarget(name: "PortKey", dependencies: ["FollowCore"]), .executableTarget(name: "CoreTests", dependencies: ["FollowCore"], path: "Tests/FollowCoreTests")], swiftLanguageModes: [.v5])
