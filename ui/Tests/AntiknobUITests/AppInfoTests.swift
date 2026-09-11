// AppInfoTests.swift — tests for AppConstants.AppInfo version metadata.

import Testing

@testable import AntiknobUI

@Suite("AppInfo metadata")
struct AppInfoTests {
    @Test("version string is present and non-empty")
    func versionIsNonEmpty() {
        #expect(!AppConstants.AppInfo.version.isEmpty)
    }

    @Test("version display begins with v and contains version")
    func versionDisplayFormatted() {
        let display = AppConstants.AppInfo.versionDisplay
        #expect(display.hasPrefix("v"))
        #expect(display.contains(AppConstants.AppInfo.version))
    }
}
