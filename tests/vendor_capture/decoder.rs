//! A decoder for captured `0xFD` / `0xFE` / `0xFA` frames.
//!
//! `antiknob` can BUILD every frame it sends but could not READ one back, so
//! the capture log in `VENDOR_UI_MAP.md` had no code that interpreted it: the
//! frames sat in prose, and the claims made about them — that byte 6 is a
//! payload length and not an entry count, that the delay field is 16-bit
//! big-endian, that a media usage spans two entries — were assertions in a
//! document that no test read.
//!
//! This is that decoder, and it encodes the DOCUMENTED format, not a format
//! convenient to the writer:
//!
//! ```text
//! 03 FD <key> <layer> <kind> <b5> <len> <entry 1> ... <entry n>
//! entry = <delay hi> <delay lo> <value>      delay is 16-bit BIG-endian ms
//! ```
//!
//! `len` counts value BYTES, not entries, because one action can span several:
//! a keycode is 1 byte, a media usage 2 (low then high), a mouse action 4. An
//! entry is 3 bytes either way, so the byte count and the entry count differ
//! for every multi-byte action — which is precisely the distinction a decoder
//! that walked `len` entries would get wrong, and precisely what the captured
//! Calculator frame (`0x0092`, non-zero high byte) exists to settle.
//!
//! It is also the shared half of the two oracle test files — `vendor_capture_oracle.rs`
//! (provenance, and re-deriving each frame from this repo's producers) and
//! `vendor_capture_decode.rs` (the format claims themselves) — which include it
//! with `#[path]` rather than each carrying their own copy of the fixture
//! loader. One description of a capture, or the point is lost.

use antiknob::firmware::{FD_ENTRY_LEN, FD_HEADER_LEN, FD_KIND_KEYBOARD, REPORT_LEN};

/// One `(delay_ms, value)` pair as the wire carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub delay_ms: u16,
    pub value: u8,
}

/// What a captured frame says, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Byte 1. `0xFD` going out, `0xFA` coming back — the vendor reuses the
    /// byte, which is why the direction has to be read rather than assumed.
    pub tag: u8,
    pub key_id: u8,
    /// Byte 3. 1-based on the wire; the builders take it 0-based.
    pub layer: u8,
    /// Byte 4: 1 keyboard, 2 media, 3 mouse.
    pub kind: u8,
    /// Byte 5. Device-owned: it reads back whatever was sent here.
    pub b5: u8,
    /// Byte 6. The declared payload LENGTH IN BYTES, not an entry count.
    pub declared_bytes: u8,
    pub entries: Vec<Entry>,
}

impl Record {
    /// The number of entries the payload actually occupies.
    ///
    /// Byte 6 is a byte count; each entry is `FD_ENTRY_LEN` bytes, and a
    /// multi-byte action fills one entry per byte, so the two agree numerically
    /// and mean different things. Kept as separate accessors anyway, because
    /// conflating them is the bug this decoder exists to make impossible.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Whether the entries span exactly the declared byte count.
    ///
    /// A capture where this is false is a capture of something other than what
    /// it claims: either the firmware truncates (the media-chain limit) or the
    /// vendor app wrote a length it did not fill (the chaining failure).
    pub fn entries_match_declared_length(&self) -> bool {
        self.entries.len() == self.declared_bytes as usize
    }

    /// One action's bytes, reassembled from the entries it spans.
    ///
    /// A keycode is one entry. A media usage is two, low byte first — so
    /// `0x00B7` (Stop) reads `b7 00` and `0x0192` (Calculator) reads `92 01`,
    /// and a decoder that took a single entry would report both as their low
    /// byte alone.
    pub fn action_bytes(&self) -> Vec<u8> {
        self.entries.iter().map(|e| e.value).collect()
    }

    /// The media usage this record carries, if it is a media record.
    pub fn media_usage(&self) -> Option<u16> {
        if self.kind != 2 || self.entries.len() < 2 {
            return None;
        }
        Some(u16::from_le_bytes([
            self.entries[0].value,
            self.entries[1].value,
        ]))
    }

    /// The single keycode this record carries, if it is a one-key keyboard
    /// record. A chord is several entries and returns `None`.
    pub fn keycode(&self) -> Option<u8> {
        (self.kind == FD_KIND_KEYBOARD && self.entries.len() == 1).then(|| self.entries[0].value)
    }
}

/// Zero-pad a capture out to a full report.
///
/// A HID report is always `REPORT_LEN` bytes and the capture log writes down
/// only the bytes that were not zero — `03 fd 02 01 02 00 02 00 00 b7` is ten
/// bytes of a 64-byte report whose remaining 54 were `00`. Decoding the ten
/// bytes as-is would read ONE entry where byte 6 declares two payload bytes,
/// and a media usage that spans two entries would come back as its low byte
/// alone. So a capture is treated as a PREFIX and padded, which is what it is.
///
/// The consequence is stated rather than hidden: a capture cannot show that a
/// trailing byte was zero, only that the log did not record it. Every frame
/// here was recorded by an interposer that logs non-zero runs, and that is the
/// assumption this rests on.
fn pad_to_report(frame: &[u8]) -> Vec<u8> {
    let mut out = frame.to_vec();
    out.resize(REPORT_LEN, 0);
    out
}

/// Decode a captured `0xFD` record — or an `0xFA` read-back of one.
///
/// Bytes past the declared length are not read: the device stores a whole
/// 64-byte report and only executes what byte 6 declares, so the remainder of
/// a read-back is residue. Reading it anyway is how a stale tail gets mistaken
/// for a binding.
pub fn decode_record(frame: &[u8]) -> Result<Record, String> {
    if frame.len() < FD_HEADER_LEN {
        return Err(format!(
            "a record needs at least {FD_HEADER_LEN} bytes (report id, tag, key, layer, \
             kind, b5, len); got {}",
            frame.len()
        ));
    }
    if frame[0] != antiknob::firmware::REPORT_ID {
        return Err(format!(
            "report id is 0x{:02X}, not 0x{:02X}; this is not a report from this device",
            frame[0],
            antiknob::firmware::REPORT_ID
        ));
    }
    let frame = pad_to_report(frame);
    let declared = frame[6] as usize;
    let entries = declared;
    let mut out = Vec::with_capacity(entries);
    for i in 0..entries {
        let at = FD_HEADER_LEN + i * FD_ENTRY_LEN;
        out.push(Entry {
            // Big-endian, and this is the claim the 0xC418 = 50200 capture
            // exists to prove: a one-byte field could not hold that value.
            delay_ms: u16::from_be_bytes([frame[at], frame[at + 1]]),
            value: frame[at + 2],
        });
    }
    Ok(Record {
        tag: frame[1],
        key_id: frame[2],
        layer: frame[3],
        kind: frame[4],
        b5: frame[5],
        declared_bytes: frame[6],
        entries: out,
    })
}

/// Decode a captured `0xFA B0 <layer>` backlight reply.
///
/// Framed `03 FA <mode> <48 bytes of palette>`, so the mode is byte 2. Byte 2
/// of the LED INIT's echo is `00`, which is the whole reason a read-back has to
/// match the tag rather than trust the first report it is handed.
pub fn decode_led_reply(frame: &[u8]) -> Result<(u8, Vec<u8>), String> {
    if frame.len() < 3 {
        return Err(format!(
            "an LED reply needs at least 3 bytes; got {}",
            frame.len()
        ));
    }
    if frame[1] != antiknob::firmware::LED_REPLY_TAG {
        return Err(format!(
            "byte 1 is 0x{:02X}, not the 0xFA reply tag; this is the wrong report \
             (an LED init echo answers 0xFB, and its byte 2 reads as mode 0)",
            frame[1]
        ));
    }
    Ok((frame[2], frame[3..].to_vec()))
}

/// Decode a captured run of ENTRIES — bytes 7 onward of a record, with no
/// header in front of them.
///
/// The delay-box captures are exactly this: the interposer logged bytes 7..15
/// of a record while someone typed into the vendor app's delay spin boxes, and
/// the whole 16-bit big-endian claim rests on those nine bytes. They are not
/// reports and must not be decoded as reports — they carry no report id, and
/// scoring a fragment against a frame's layout is how a real frame and a real
/// fragment end up agreeing about nothing.
pub fn decode_entry_group(bytes: &[u8]) -> Vec<Entry> {
    bytes
        .as_chunks::<FD_ENTRY_LEN>()
        .0
        .iter()
        .map(|c| Entry {
            delay_ms: u16::from_be_bytes([c[0], c[1]]),
            value: c[2],
        })
        .collect()
}

/// Parse a hex string from the fixture into bytes.
///
/// The fixture stores frames as hex with no separators, so the test never
/// re-parses a hand-spaced literal and cannot disagree with the file about
/// which byte is which.
pub fn hex(text: &str) -> Vec<u8> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '|')
        .collect();
    assert!(
        clean.len().is_multiple_of(2),
        "fixture frame {text:?} has an odd number of hex digits"
    );
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).expect("fixture holds hex digits"))
        .collect()
}

/// The longest common prefix length of two frames.
pub fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Whether a frame is at most `REPORT_LEN` bytes, as a 64-byte report is.
pub fn report_sized(frame: &[u8]) -> bool {
    frame.len() <= REPORT_LEN
}
