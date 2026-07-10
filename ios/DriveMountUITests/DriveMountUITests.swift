import XCTest

final class DriveMountUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testCanOpenConnectionList() {
        let app = XCUIApplication()
        app.launchArguments = ["--uitesting"]
        app.launch()

        XCTAssertTrue(app.navigationBars["Drive Mount"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["add-connection-button"].exists)
    }

    func testSeededB2ConnectionAppearsWhenEnvironmentIsPresent() throws {
        let bucket = Self.environmentValue("DRIVEMOUNT_TEST_B2_BUCKET")
        let keyID = Self.environmentValue("DRIVEMOUNT_TEST_B2_KEY_ID")
        let applicationKey = Self.environmentValue("DRIVEMOUNT_TEST_B2_APPLICATION_KEY")
        try XCTSkipUnless(bucket != nil && keyID != nil && applicationKey != nil, "B2 credentials not supplied to UI test environment.")

        let app = XCUIApplication()
        app.launchArguments = ["--uitesting", "--seed-b2-from-environment"]
        app.launchEnvironment["DRIVEMOUNT_TEST_B2_BUCKET"] = bucket
        app.launchEnvironment["DRIVEMOUNT_TEST_B2_KEY_ID"] = keyID
        app.launchEnvironment["DRIVEMOUNT_TEST_B2_APPLICATION_KEY"] = applicationKey
        app.launch()

        XCTAssertTrue(app.staticTexts[bucket!].waitForExistence(timeout: 8))
    }

    private static func environmentValue(_ key: String) -> String? {
        let env = ProcessInfo.processInfo.environment
        return env[key] ?? env["TEST_RUNNER_\(key)"]
    }
}
