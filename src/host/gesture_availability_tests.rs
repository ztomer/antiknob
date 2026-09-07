//! Which knob gestures the firmware can actually produce.
//!
//! Split from `mod.rs` for the file-length gate.

#[cfg(test)]
mod tests {
    use crate::host::Gesture;

    /// Three gestures exist in the firmware and two do not. Captured from
    /// the device: a hold-and-twist emits the press binding then the rotate
    /// binding, and a probe that wrote distinct markers to the two spare
    /// slots saw neither of them fire.
    #[test]
    fn only_the_three_firmware_gestures_are_bindable() {
        assert!(Gesture::TwistL.is_bindable());
        assert!(Gesture::Press.is_bindable());
        assert!(Gesture::TwistR.is_bindable());
        assert!(!Gesture::HoldTwistL.is_bindable());
        assert!(!Gesture::HoldTwistR.is_bindable());
        assert_eq!(Gesture::ALL.iter().filter(|g| g.is_bindable()).count(), 3);
    }

    /// "Unavailable" without a reason reads as "broken". The reason is what
    /// stops someone re-flashing in the hope of fixing it.
    #[test]
    fn an_unavailable_gesture_explains_itself_and_a_bindable_one_says_nothing() {
        assert!(Gesture::TwistL.unavailable_reason().is_none());
        let why = Gesture::HoldTwistL
            .unavailable_reason()
            .expect("an unavailable gesture must say why");
        assert!(why.contains("hold+twist"), "{why}");
    }
}
