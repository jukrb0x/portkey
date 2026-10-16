import Foundation
var failures = 0
var checks = 0
func XCTAssertEqual<T: Equatable>(_ actual: T, _ expected: T, file: StaticString = #filePath, line: UInt = #line) { checks += 1; if actual != expected { failures += 1; print("FAIL \(file):\(line): \(actual) != \(expected)") } }
func XCTAssertNotEqual<T: Equatable>(_ actual: T, _ expected: T, file: StaticString = #filePath, line: UInt = #line) { checks += 1; if actual == expected { failures += 1; print("FAIL \(file):\(line): unexpected equality \(actual)") } }
func XCTAssertNil<T>(_ actual: T?, file: StaticString = #filePath, line: UInt = #line) { checks += 1; if actual != nil { failures += 1; print("FAIL \(file):\(line): expected nil, got \(actual!)") } }
func XCTAssertThrowsError<T>(_ body: @autoclosure () throws -> T, file: StaticString = #filePath, line: UInt = #line) { checks += 1; do { _ = try body(); failures += 1; print("FAIL \(file):\(line): expected error") } catch {} }
