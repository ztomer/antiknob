// Tests for the shared control metrics.
//
// `Layout.dropdown` is a hardcoded point width, which is exactly the kind of
// constant that silently stops being wide enough: the Template pop-up was
// capped at 240 and truncated "Meeting Controller (Zoom)" to "Media
// Controller..." for as long as it existed, because nothing measured it.
//
// So measure it. These render every string that can appear in a dropdown in
// the real system font and fail, naming the offender and the width it needs,
// if one would not fit. Adding a longer option is then a red test rather than
// an ellipsis nobody notices.

import AppKit
import Testing

@testable import AntiknobUI

/// Measured in the real system body font. Fetched per call rather than held
/// in a global: `NSFont` is not `Sendable`, and this package builds in the
/// Swift 6 language mode.
private func width(_ s: String) -> CGFloat {
    let font = NSFont.preferredFont(forTextStyle: .body)
    return (s as NSString).size(withAttributes: [.font: font]).width
}

/// Every label that can be shown *in* a dropdown, from the enums that own
/// them wherever one exists.
private var everyDropdownLabel: [String] {
    var labels: [String] = []
    labels += KeymapTemplate.allCases.map(\.rawValue)
    labels += SlotTarget.allCases.map(\.label)
    labels += SlotMemoryGroup.allCases.map(\.label)
    labels += auxTitles.values
    labels += MouseButton.allCases.map { "Click \($0.rawValue)" }
    // Mirrors the literal arms of LayerDetail.currentTitle. Kept in sync by
    // hand; the enum-backed lists above are the ones that can grow silently.
    labels += [
        "None", "Keystroke", "Scroll Up", "Scroll Down", "Sequence",
        "Media", "Open App", "Website", "Open File", "Quit App",
        "Hotkey Switch"
    ]
    return labels
}

@Suite("Control metrics")
struct LayoutTests {
    @Test("every dropdown option fits the dropdown width")
    func optionsFitTheDropdown() {
        let budget = Layout.dropdown - Layout.dropdownChrome
        for label in everyDropdownLabel {
            let needed = width(label)
            #expect(
                needed <= budget,
                """
                "\(label)" needs \(Int(needed.rounded()))pt of text but the \
                dropdown allows \(Int(budget.rounded()))pt. Raise \
                Layout.dropdown to at least \
                \(Int((needed + Layout.dropdownChrome).rounded()))pt, or shorten it.
                """
            )
        }
    }

    @Test("every pop-up's own label fits the label column")
    func controlLabelsFitTheirColumn() {
        for label in ["Target Layer", "Template", "Memory Group"] {
            let needed = width(label)
            #expect(
                needed <= Layout.controlLabel,
                """
                "\(label)" needs \(Int(needed.rounded()))pt but \
                Layout.controlLabel is \(Int(Layout.controlLabel.rounded()))pt.
                """
            )
        }
    }

    /// Calibration: the budget must be capable of rejecting something. A
    /// chrome allowance larger than the width would make every case pass by
    /// arithmetic rather than by fitting.
    @Test("the fit check can actually fail")
    func theBudgetIsMeaningful() {
        let budget = Layout.dropdown - Layout.dropdownChrome
        #expect(budget > 0, "chrome allowance swallows the whole width")
        #expect(
            width("An Option Label Far Longer Than Any Real One") > budget,
            "the budget is so wide nothing could ever fail it"
        )
        #expect(!everyDropdownLabel.isEmpty, "no labels were collected")
        #expect(everyDropdownLabel.count > 25, "the label set looks truncated")
    }
}
