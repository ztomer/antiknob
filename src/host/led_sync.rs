//! Making the knob's light follow the ACTIVE HOST LAYER.
//!
//! The firmware stores one backlight mode per DEVICE layer and has never
//! heard of host layers. So a per-layer colour is not something the hardware
//! does on its own -- it is something the daemon has to write on every
//! switch, to whichever device layer carries the slot chords.
//!
//! Until this existed, `EngineEvent::LayerChanged` was printed and nothing
//! else, and the settings app's lighting controls addressed the firmware's
//! three fixed layers. Those are a different axis from the host layers: only
//! ONE device layer is host-translated, so while the daemon is driving, the
//! knob sits on that one and shows its one colour whatever host layer is
//! active. Offering a colour per host layer without this would have been a
//! control that changes a value nothing reads.
//!
//! The decision is pure and the effect is one function, so what a switch
//! IMPLIES can be tested without a knob attached.

use super::HostConfig;
use crate::firmware::{LED_INTER_LAYER_SETTLE_MS, LED_JOB_INTERVAL_MS};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::time::{Duration, Instant};

/// Minimum gap between the START of two LED jobs anywhere in this process.
///
/// The firmware's renderer wedges when mode changes arrive in a burst (seen
/// on a `514c:8850`: three writes in seconds, then the stored mode and the
/// rendered light disagreed until a replug -- kriomant/ch57x-keyboard-tool#175
/// documents the same freeze striking after any mode change). One change
/// still carries some risk no software can remove; bursts multiply it, and
/// bursts are the part this process controls: a layer switch fans out to
/// every bound device layer, and a lighting click arrives as three RPCs.
///
/// The budget lives in the firmware map; the pacer that enforces it lives
/// here.
const MIN_LED_JOB_INTERVAL: Duration = Duration::from_millis(LED_JOB_INTERVAL_MS);

/// Settle between consecutive layer writes inside one job. The probe's
/// 150ms is the only measured number in the tree; shorter has not been
/// shown to render reliably.
const INTER_LAYER_SETTLE: Duration = Duration::from_millis(LED_INTER_LAYER_SETTLE_MS);

/// Every `sync_led` call takes the next ticket; the pacer remembers the
/// newest one. A worker that waited out its pacing and finds a newer ticket
/// drops its write: the newer call carries the newer intent, so writing the
/// older modes afterwards would leave the light showing a layer nobody is
/// on. Last-wins, by construction rather than by comment.
static NEXT_TICKET: AtomicU64 = AtomicU64::new(0);

struct LedPacer {
    last_start: Option<Instant>,
    latest_ticket: u64,
}

static PACER: OnceLock<Mutex<LedPacer>> = OnceLock::new();

fn pacer() -> &'static Mutex<LedPacer> {
    PACER.get_or_init(|| {
        Mutex::new(LedPacer {
            last_start: None,
            latest_ticket: 0,
        })
    })
}

/// How long a job starting `now` must wait after `last_start`, purely.
///
/// Consecutive job STARTS stay `MIN_LED_JOB_INTERVAL` apart: a job booked
/// to start at T pushes the next one to at least T plus the interval, so a
/// burst queues instead of stacking. Separate from the clock so the
/// arithmetic is testable without sleeping.
fn wait_before(last_start: Option<Instant>, now: Instant) -> Duration {
    match last_start {
        Some(t) => t
            .checked_add(MIN_LED_JOB_INTERVAL)
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(Duration::ZERO),
        None => Duration::ZERO,
    }
}

/// Wait out this process's LED pacing and reserve the next slot.
///
/// Entry points that write LED modes directly (the `set_led` command, the
/// CLI) call this first, so a lighting click's three RPCs pace themselves
/// instead of bursting. Jobs already inside `sync_led`'s worker go through
/// the same reservation; sharing one clock is what makes the interval hold
/// across entry points rather than per caller.
pub(crate) fn pace_led() {
    // Reserve the slot before sleeping, so concurrent jobs queue instead
    // of all waking together.
    let wait = {
        let mut pacer = pacer().lock().unwrap();
        let now = Instant::now();
        let wait = wait_before(pacer.last_start, now);
        pacer.last_start = Some(now + wait);
        wait
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
}

/// Drop the writes the device already holds, so a sync that changes nothing
/// sends nothing.
///
/// Each skipped write is one fewer mode change the firmware can freeze on.
/// The common case is real: switching back to a layer whose modes are
/// already stored, or syncing right after a keymap flash that wrote them.
/// Unknown on either side writes rather than skips -- skipping on
/// uncertainty would leave a light unchanged with success reported.
///
/// The skip needs the mode number AND the canonical spec word: two specs
/// can name one mode with different palettes ("red" vs "red blue"), and
/// the number alone cannot tell them apart. An alias ("static" for red)
/// always writes -- a redundant write is merely a wasted change, while a
/// wrongly skipped palette would be a light the config no longer describes.
/// On this VK01 the palette is ignored anyway; the strictness is for the
/// 16-key variant sharing the product id, which honours per-key RGB.
pub fn pending_writes(writes: Vec<(u8, String)>, stored: &[(u8, u8)]) -> Vec<(u8, String)> {
    writes
        .into_iter()
        .filter(|(layer, mode)| {
            let normalized = mode.trim().to_ascii_lowercase();
            match (
                crate::led::led_mode_number(mode),
                stored.iter().find(|(l, _)| l == layer),
            ) {
                (Some(want), Some((_, have))) => {
                    want != *have
                        || crate::firmware::LED_MODE_NAMES
                            .get(want as usize)
                            .is_none_or(|canonical| *canonical != normalized)
                }
                _ => true,
            }
        })
        .collect()
}

/// The LED write a switch to `layer_idx` implies: `(device layer, mode)`.
///
/// `None` in three real cases, none of which is an error:
///   * nothing is bound, so no device layer is host-translated and there is
///     no light the daemon is entitled to drive;
///   * the layer names no mode, which means "leave it alone";
///   * the index is out of range, which a caller can produce during a
///     config reload and must not be turned into a write to layer 0.
pub fn led_writes_for(cfg: &HostConfig, layer_idx: usize) -> Vec<(u8, String)> {
    let Some(mode) = cfg.layers.get(layer_idx).and_then(|l| l.led.as_deref()) else {
        return Vec::new();
    };
    let mode = mode.trim();
    if mode.is_empty() {
        return Vec::new();
    }
    // EVERY bound layer, not one. `bind-slots` with no `--layer` binds all
    // three, and nothing on this firmware reports which one the knob is
    // currently on -- a calibrated 512-query sweep found no such query. So
    // the only way to be sure the active layer shows the right colour is to
    // set them all. With one layer bound this is the single write it always
    // was.
    cfg.bound_device_layers
        .iter()
        .map(|l| (*l, mode.to_string()))
        .collect()
}

/// Apply that write, OFF the calling thread.
///
/// Detached deliberately. Both callers are latency-critical: one is the
/// CGEventTap callback, and macOS disables a tap whose callback runs long.
/// An LED write is two HID reports with a 20 ms settle between them, plus
/// however long the HID thread's queue already is -- easily 100 ms, on the
/// thread that is supposed to be forwarding the user's keystrokes.
///
/// Three guarantees live here now, all about the firmware freeze above:
///
/// * pacing: a job waits out `MIN_LED_JOB_INTERVAL` since the previous
///   job's start, so layer switches and lighting clicks cannot stack
///   writes back-to-back;
/// * last-wins: a job superseded while pacing drops its write instead of
///   painting a stale layer afterwards -- and the check repeats after the
///   read and between layers, because a sync arriving mid-job would
///   otherwise interleave older modes over the newer job's;
/// * read-before-write: layers already holding the target mode are
///   skipped, so a no-op sync sends nothing at all.
///
/// Failures are dropped on purpose: the knob may be unplugged, and a switch
/// whose light did not follow is still a switch that happened. The layer
/// change must not fail because the light did not.
pub fn sync_led(cfg: &HostConfig, layer_idx: usize) {
    let writes = led_writes_for(cfg, layer_idx);
    if writes.is_empty() {
        return;
    }
    let ticket = NEXT_TICKET.fetch_add(1, Ordering::SeqCst);
    {
        let mut pacer = pacer().lock().unwrap();
        pacer.latest_ticket = ticket;
    }
    std::thread::spawn(move || {
        pace_led();
        // A newer sync arrived while pacing: it carries the newer intent.
        // The check repeats after the read and between layers below -- a
        // sync arriving mid-job must not interleave older modes over the
        // newer job's, one check at the top cannot see it.
        if superseded(ticket) {
            return;
        }
        // Read at write time, not at spawn time: an earlier job may have
        // landed while pacing, and re-writing its modes would be a no-op
        // change the firmware can still freeze on. A failed read falls
        // back to the full list -- writing on uncertainty, never skipping
        // on it.
        let layers: Vec<u8> = writes.iter().map(|(l, _)| *l).collect();
        let stored: Vec<(u8, u8)> = crate::device::with_device(move |dev| {
            layers
                .iter()
                .map(|l| crate::device::read_led_mode(dev, *l).map(|m| (*l, m)))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
        if superseded(ticket) {
            return;
        }
        let pending = if stored.len() == writes.len() {
            pending_writes(writes, &stored)
        } else {
            writes
        };
        if pending.is_empty() {
            return;
        }
        for (i, (device_layer, mode)) in pending.into_iter().enumerate() {
            if i > 0 {
                std::thread::sleep(INTER_LAYER_SETTLE);
            }
            if superseded(ticket) {
                return;
            }
            let Ok(packet) = crate::protocol::build_led_packet(device_layer, &mode) else {
                continue;
            };
            let _ = crate::device::with_device(move |dev| crate::device::send_led(dev, &packet));
        }
    });
}

/// True when a newer `sync_led` has arrived since `ticket` was taken.
/// Small enough to call at every yield point; the pacer mutex is never
/// held across HID work, so this cannot deadlock the writer it guards.
fn superseded(ticket: u64) -> bool {
    pacer().lock().unwrap().latest_ticket != ticket
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostConfig, HostLayer};

    fn layer(name: &str, led: Option<&str>) -> HostLayer {
        HostLayer {
            name: name.to_string(),
            led: led.map(str::to_string),
            ..HostLayer::empty(name)
        }
    }

    fn cfg(bound: Vec<u8>, layers: Vec<HostLayer>) -> HostConfig {
        HostConfig {
            layers,
            bound_device_layers: bound,
            ..HostConfig::default_config()
        }
    }

    #[test]
    fn a_layer_with_a_mode_writes_it_to_the_bound_device_layer() {
        let c = cfg(
            vec![1],
            vec![layer("Media", Some("red")), layer("Nav", Some("green"))],
        );
        assert_eq!(led_writes_for(&c, 0), vec![(1, "red".to_string())]);
        assert_eq!(led_writes_for(&c, 1), vec![(1, "green".to_string())]);
    }

    /// THE CASE THAT SHIPPED BROKEN. `bind-slots` with no `--layer` binds
    /// every device layer, and nothing on this firmware reports which one
    /// the knob is currently on -- so the only way the active layer shows
    /// the right colour is to write all of them.
    ///
    /// Before the recorded value became a SET this could not even be
    /// expressed: binding all three recorded `None`, the same value as
    /// binding nothing, so the backlight silently never fired after the most
    /// common flash there is.
    #[test]
    fn every_bound_layer_is_written_when_all_of_them_are_bound() {
        let c = cfg(vec![0, 1, 2], vec![layer("Media", Some("green"))]);
        assert_eq!(
            led_writes_for(&c, 0),
            vec![
                (0, "green".to_string()),
                (1, "green".to_string()),
                (2, "green".to_string())
            ],
            "a fully bound knob must have every layer set, or the colour \
             depends on which layer it happens to be on"
        );
    }

    /// Nothing bound means no device layer is host-translated, so there is
    /// no light this daemon is entitled to drive.
    #[test]
    fn nothing_bound_writes_nothing() {
        let c = cfg(Vec::new(), vec![layer("Media", Some("red"))]);
        assert!(led_writes_for(&c, 0).is_empty());
    }

    /// A layer that names no mode leaves the light as it is. This is the
    /// state every layer written before the field existed loads in.
    #[test]
    fn a_layer_without_a_mode_leaves_the_light_alone() {
        let c = cfg(
            vec![0],
            vec![layer("Media", None), layer("Blank", Some("   "))],
        );
        assert!(led_writes_for(&c, 0).is_empty());
        assert!(led_writes_for(&c, 1).is_empty(), "whitespace is not a mode");
    }

    /// An index past the end is producible during a config reload. Falling
    /// back to layer 0's mode would light the knob for a layer that is not
    /// active.
    #[test]
    fn an_out_of_range_layer_writes_nothing() {
        let c = cfg(vec![0], vec![layer("Media", Some("red"))]);
        assert!(led_writes_for(&c, 7).is_empty());
    }

    /// Whatever the mode string is, it must be one `build_led_packet`
    /// accepts -- otherwise `sync_led` silently drops every switch and the
    /// colours never change, with nothing anywhere reporting why.
    #[test]
    fn every_mode_a_layer_can_carry_builds_a_packet() {
        for name in crate::firmware::LED_MODE_NAMES {
            let c = cfg(vec![0], vec![layer("L", Some(name))]);
            let writes = led_writes_for(&c, 0);
            assert_eq!(writes.len(), 1, "{name}");
            assert!(
                crate::protocol::build_led_packet(writes[0].0, &writes[0].1).is_ok(),
                "{name} is offered but cannot be sent"
            );
        }
    }

    /// The pacer math, without sleeping: idle means go, a recent job means
    /// wait out the remainder, and a clock that runs backwards (or a stale
    /// timestamp) means go rather than a negative sleep.
    #[test]
    fn wait_zero_when_idle() {
        let now = Instant::now();
        assert_eq!(wait_before(None, now), Duration::ZERO);
    }

    #[test]
    fn wait_covers_the_remainder() {
        let now = Instant::now();
        let started_400ms_ago = now - Duration::from_millis(400);
        let wait = wait_before(Some(started_400ms_ago), now);
        assert!(
            wait >= Duration::from_millis(90) && wait <= Duration::from_millis(110),
            "waits out the remainder, got {wait:?}"
        );
    }

    #[test]
    fn wait_never_goes_negative() {
        let now = Instant::now();
        // Long past the interval: no wait.
        let old = now - MIN_LED_JOB_INTERVAL - Duration::from_secs(1);
        assert_eq!(wait_before(Some(old), now), Duration::ZERO);
        // A reservation ahead of now (another job's booked start): the next
        // job starts one interval after it, which is the case the pacer
        // produces on every burst.
        let reserved = now + Duration::from_millis(300);
        let wait = wait_before(Some(reserved), now);
        assert!(
            wait >= Duration::from_millis(790) && wait <= Duration::from_millis(810),
            "starts one interval after the booked start, got {wait:?}"
        );
    }

    /// A sync that changes nothing sends nothing: every skipped write is
    /// one fewer mode change the firmware can freeze on. The cases that
    /// matter are the no-op sync (everything already stored) and the
    /// partial one (only the changed layer goes out).
    #[test]
    fn already_stored_modes_are_not_rewritten() {
        let writes = vec![(0u8, "red".to_string()), (1u8, "green".to_string())];
        assert!(pending_writes(writes, &[(0, 1), (1, 2)]).is_empty());
    }

    #[test]
    fn only_the_changed_layer_goes_out() {
        let writes = vec![(0u8, "red".to_string()), (1u8, "green".to_string())];
        // Layer 1 holds red (1), not green (2): only it is pending.
        let pending = pending_writes(writes, &[(0, 1), (1, 1)]);
        assert_eq!(pending, vec![(1u8, "green".to_string())]);
    }

    /// Uncertainty writes rather than skips: an unmappable spec or a layer
    /// with no read-back must still go out, or a light change would be
    /// silently dropped with success reported.
    #[test]
    fn unknown_modes_and_missing_reads_still_write() {
        let writes = vec![(0u8, "red".to_string())];
        // No read-back for layer 0 at all.
        assert_eq!(pending_writes(writes.clone(), &[]).len(), 1);
        // A spec no table knows (cannot normally happen; the writer drops
        // it later, but the filter must not be the thing that does).
        let odd = vec![(0u8, "chartreuse".to_string())];
        assert_eq!(pending_writes(odd, &[(0, 1)]).len(), 1);
    }

    /// The skip is exact-canonical or nothing: an alias names a stored mode
    /// the filter cannot prove identical (palettes ride on the spec, and
    /// the stored side carries only the number), so it goes out. A
    /// redundant write is a wasted change; a wrong skip is a light the
    /// config no longer describes.
    #[test]
    fn aliases_and_palettes_are_not_skipped_as_canonical() {
        // "static" IS mode 1, but the stored palette is unknown: write.
        let alias = vec![(0u8, "static".to_string())];
        assert_eq!(pending_writes(alias, &[(0, 1)]).len(), 1);
        // Same mode number, extra palette words: write.
        let paletted = vec![(0u8, "red white".to_string())];
        assert_eq!(pending_writes(paletted, &[(0, 1)]).len(), 1);
        // Bare canonical, case-insensitive: skip.
        let canonical = vec![(0u8, "  RED  ".to_string())];
        assert!(pending_writes(canonical, &[(0, 1)]).is_empty());
    }
}
