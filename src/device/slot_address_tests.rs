//! Coverage for the slot-table addressing scheme.
//!
//! Split from `mod.rs` for the file-length gate.

#[cfg(test)]
mod tests {
    use crate::device::{slot_table_addresses, DEVICE_LAYERS};

    /// Every slot exactly once: the walk that missed one confirmed 17 of 18
    /// and reported the last as "not seen", which reads like a failed write
    /// and was a truncated reader.
    #[test]
    fn the_addresses_cover_every_slot_once() {
        let addrs = slot_table_addresses(6, DEVICE_LAYERS);
        assert_eq!(addrs.len(), 18, "6 slots x 3 layers");
        let counters: Vec<u8> = addrs.iter().map(|(_, c)| *c).collect();
        assert_eq!(counters, (1..=18).collect::<Vec<u8>>());
        assert!(
            addrs.iter().all(|(g, _)| *g == 6),
            "the group parameter is the layer width, constant across the walk"
        );
    }

    /// A knob-only device is narrower, and a macropad wider; neither should
    /// need the caller to know anything but its own layout.
    #[test]
    fn the_walk_scales_with_the_layer_width() {
        assert_eq!(slot_table_addresses(3, 3).len(), 9);
        assert_eq!(slot_table_addresses(18, 3).len(), 54);
    }

    /// A layout wide enough to overflow the counter must not wrap around and
    /// silently read a handful of slots instead of refusing.
    #[test]
    fn an_unaddressable_width_saturates_rather_than_wrapping() {
        let addrs = slot_table_addresses(200, 3);
        assert_eq!(addrs.len(), usize::from(u8::MAX));
    }
}
