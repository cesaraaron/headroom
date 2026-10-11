//! MIDI input — drive pedals/knobs and follow a MIDI clock from a controller.
//!
//! A `midir` input connection reads MIDI on its own thread. The callback runs
//! **off the audio thread** and only performs atomic stores into [`Params`], so
//! there is no allocation, lock, or wait anywhere near the callback — the same
//! discipline as the UI/control thread.
//!
//! Control Changes are bound to targets in `~/.config/rusty-riff/midi.conf`. The
//! legacy single binding (`cc = 11`) drives the wah treadle; add more as
//! `<cc> = <target>`, and opt into MIDI-clock sync with `clock = true`:
//!
//! ```text
//! enabled = true
//! clock = true            # follow MIDI clock → delay TIME
//! cc = 11                 # wah treadle (legacy shorthand)
//! 20 = delay_mix
//! 21 = reverb_mix
//! ```
//!
//! A bound CC that drives `wah_position` also switches the wah to manual mode, so
//! an expression pedal overrides a preset's auto-wah as soon as it moves. When
//! `clock` is on, incoming 24-ppqn clock sets the delay `TIME` to the beat and
//! the UI footer shows the tempo; a clock `Stop` (or the signal going idle) hands
//! control back to the knob and the `;` tap. A learn/bind screen is not built yet
//! — bindings are edited in the config file.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

use midir::{Ignore, MidiInput};

use crate::dsp::Params;

/// MIDI standard "Expression" controller, used when no config overrides it.
const DEFAULT_CC: u8 = 11;

/// Keeps the MIDI connection alive for the process lifetime; dropping it closes
/// the port.
pub struct MidiHandle {
    _conn: midir::MidiInputConnection<()>,
    /// The input port we connected to (for logging/UI).
    pub port: String,
}

/// A knob/parameter a CC can drive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MidiTarget {
    WahPosition,
    WahMode,
    DelayTime,
    DelayFeedback,
    DelayMix,
    ReverbMix,
    BoostGain,
    CompSustain,
    TsDrive,
    DsDrive,
    ChorusMix,
    FlangerMix,
    PhaserMix,
    MasterWidth,
    MasterOutput,
}

impl MidiTarget {
    /// Every target, in bind-screen order.
    pub const ALL: [Self; 15] = [
        Self::WahPosition,
        Self::WahMode,
        Self::DelayTime,
        Self::DelayFeedback,
        Self::DelayMix,
        Self::ReverbMix,
        Self::BoostGain,
        Self::CompSustain,
        Self::TsDrive,
        Self::DsDrive,
        Self::ChorusMix,
        Self::FlangerMix,
        Self::PhaserMix,
        Self::MasterWidth,
        Self::MasterOutput,
    ];

    /// The config token for this target — the inverse of [`Self::parse`].
    pub fn name(self) -> &'static str {
        match self {
            Self::WahPosition => "wah_position",
            Self::WahMode => "wah_mode",
            Self::DelayTime => "delay_time",
            Self::DelayFeedback => "delay_feedback",
            Self::DelayMix => "delay_mix",
            Self::ReverbMix => "reverb_mix",
            Self::BoostGain => "boost_gain",
            Self::CompSustain => "comp_sustain",
            Self::TsDrive => "ts_drive",
            Self::DsDrive => "ds_drive",
            Self::ChorusMix => "chorus_mix",
            Self::FlangerMix => "flanger_mix",
            Self::PhaserMix => "phaser_mix",
            Self::MasterWidth => "master_width",
            Self::MasterOutput => "master_output",
        }
    }

    /// A human label for the bind screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::WahPosition => "Wah position",
            Self::WahMode => "Wah mode (manual)",
            Self::DelayTime => "Delay time",
            Self::DelayFeedback => "Delay feedback",
            Self::DelayMix => "Delay mix",
            Self::ReverbMix => "Reverb mix",
            Self::BoostGain => "Clean boost",
            Self::CompSustain => "Comp sustain",
            Self::TsDrive => "TS-808 drive",
            Self::DsDrive => "DS-1 drive",
            Self::ChorusMix => "Chorus mix",
            Self::FlangerMix => "Flanger mix",
            Self::PhaserMix => "Phaser mix",
            Self::MasterWidth => "Master width",
            Self::MasterOutput => "Master output",
        }
    }

    /// Parse a config target name. Derived from [`Self::name`] so the two can
    /// never drift apart.
    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == s)
    }

    /// Store the mapped value (0–1) into the target parameter.
    fn write(self, p: &Params, value: f32) {
        let value = value.clamp(0.0, 1.0);
        let target = match self {
            Self::WahPosition => &p.wah_position,
            Self::WahMode => &p.wah_mode,
            Self::DelayTime => &p.delay_time,
            Self::DelayFeedback => &p.delay_feedback,
            Self::DelayMix => &p.delay_mix,
            Self::ReverbMix => &p.rev_mix,
            Self::BoostGain => &p.boost_gain,
            Self::CompSustain => &p.cmp_sustain,
            Self::TsDrive => &p.ts_drive,
            Self::DsDrive => &p.ds_drive,
            Self::ChorusMix => &p.ch_mix,
            Self::FlangerMix => &p.fl_mix,
            Self::PhaserMix => &p.ph_mix,
            Self::MasterWidth => &p.master_width,
            Self::MasterOutput => &p.master_output,
        };
        target.store(value, Relaxed);
        // An expression pedal overrides a preset's auto-wah.
        if self == Self::WahPosition {
            p.wah_mode.store(1.0, Relaxed);
        }
    }
}

/// Live MIDI binding state, shared between the `midir` callback and the UI.
///
/// The binding table sits behind a `Mutex` because the bind screen changes it at
/// runtime; the callback only `try_lock`s it (never blocking the MIDI thread) and
/// skips a CC if the UI happens to hold the lock — a dropped CC during a bind is
/// inaudible.
pub struct MidiState {
    bindings: Mutex<Vec<(u8, MidiTarget)>>,
    enabled: bool,
    clock: bool,
    /// Count of CC events seen, and the packed last one (`cc << 8 | value`) — the
    /// learn channel. Written by the MIDI thread, polled by the UI.
    cc_seq: AtomicU64,
    last_cc: AtomicU32,
}

impl MidiState {
    /// Parse a config body. A missing file's default is applied by
    /// [`default_midi_state`], not here, so a file the user has emptied stays
    /// empty.
    pub fn from_conf(text: &str) -> Self {
        let (enabled, bindings, clock) = parse_midi_conf(text);
        Self {
            bindings: Mutex::new(bindings),
            enabled,
            clock,
            cc_seq: AtomicU64::new(0),
            last_cc: AtomicU32::new(0),
        }
    }

    /// Publish a CC event to the learn channel: store the value *before* bumping
    /// the sequence, so a reader that sees a new sequence reads a matching value.
    /// The MIDI thread is the producer; tests use it to simulate a controller.
    pub(crate) fn record_cc(&self, cc: u8, value: u8) {
        self.last_cc
            .store((u32::from(cc) << 8) | u32::from(value), Relaxed);
        self.cc_seq.fetch_add(1, Relaxed);
    }

    /// A token for the current CC sequence — read it when arming Learn.
    pub fn cc_seq(&self) -> u64 {
        self.cc_seq.load(Relaxed)
    }

    /// The first CC seen after `seq`: `(cc, value, new_seq)`, or `None` when
    /// nothing has arrived since. Polled once per frame while learning.
    pub fn cc_since(&self, seq: u64) -> Option<(u8, u8, u64)> {
        let now = self.cc_seq.load(Relaxed);
        if now == seq {
            return None;
        }
        let packed = self.last_cc.load(Relaxed);
        Some(((packed >> 8) as u8, (packed & 0xFF) as u8, now))
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn clock(&self) -> bool {
        self.clock
    }

    /// A snapshot of the table for the bind screen.
    pub fn bindings(&self) -> Vec<(u8, MidiTarget)> {
        self.lock().clone()
    }

    /// Bind `cc` to `target`: one CC per target and one target per CC, so any
    /// previous owner on either side is dropped first.
    pub fn bind(&self, cc: u8, target: MidiTarget) {
        let mut b = self.lock();
        b.retain(|&(c, t)| c != cc && t != target);
        b.push((cc, target));
        b.sort_by_key(|&(c, _)| c);
    }

    pub fn unbind(&self, target: MidiTarget) {
        self.lock().retain(|&(_, t)| t != target);
    }

    /// Rewrite `~/.config/rusty-riff/midi.conf` from the current table, via a
    /// temp file + rename so a crash mid-write cannot truncate it.
    pub fn write_conf(&self) -> std::io::Result<()> {
        let Some(path) = midi_config_path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, render_conf(self.enabled, self.clock, &self.lock()))?;
        std::fs::rename(&tmp, &path)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(u8, MidiTarget)>> {
        self.bindings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The default when `midi.conf` is missing: enabled, wah treadle on CC 11.
fn default_midi_state() -> MidiState {
    MidiState {
        bindings: Mutex::new(vec![(DEFAULT_CC, MidiTarget::WahPosition)]),
        enabled: true,
        clock: false,
        cc_seq: AtomicU64::new(0),
        last_cc: AtomicU32::new(0),
    }
}

/// Serialise `midi.conf`: `enabled`, `clock`, then one `<cc> = <target>` line per
/// binding (sorted by CC for a stable file).
fn render_conf(enabled: bool, clock: bool, bindings: &[(u8, MidiTarget)]) -> String {
    let mut out = format!("enabled = {enabled}\nclock = {clock}\n");
    for &(cc, target) in bindings {
        out.push_str(&format!("{cc} = {}\n", target.name()));
    }
    out
}

/// `~/.config/rusty-riff/midi.conf`.
fn midi_config_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("rusty-riff").join("midi.conf"))
}

/// A MIDI **system real-time** message we act on. Real-time bytes are single-byte
/// messages and may arrive at any time (even between a note-on's data bytes).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MidiRealtime {
    /// `0xF8` — 24 per quarter note.
    Clock,
    /// `0xFA` — start from the beginning.
    Start,
    /// `0xFB` — continue from the current position.
    Continue,
    /// `0xFC` — stop.
    Stop,
}

/// Parse a system real-time message.
pub fn parse_realtime(msg: &[u8]) -> Option<MidiRealtime> {
    if msg.len() != 1 {
        return None;
    }
    match msg[0] {
        0xF8 => Some(MidiRealtime::Clock),
        0xFA => Some(MidiRealtime::Start),
        0xFB => Some(MidiRealtime::Continue),
        0xFC => Some(MidiRealtime::Stop),
        _ => None,
    }
}

/// Pulses per quarter note in the MIDI clock spec.
const PPQN: f64 = 24.0;
/// Reject a per-pulse gap outside this range (a burst, jitter spike, or a paused
/// clock): the implied tempo would be ~12.5–625 BPM. A rejected gap re-seeds the
/// estimate instead of skewing it.
const MIN_GAP_US: u64 = 4_000;
const MAX_GAP_US: u64 = 200_000;
/// Smoothing factor for the interval estimate (0 = frozen, 1 = no smoothing).
const ALPHA: f64 = 0.2;
/// Valid intervals needed before the estimate is trusted.
const MIN_SAMPLES: u32 = 2;

/// Turns MIDI clock pulses into a BPM estimate. Pure (the caller passes the
/// timestamp), so it is unit-testable without a MIDI port — the same shape as
/// [`crate::tap_tempo::TapTempo`].
#[derive(Default)]
pub struct MidiClock {
    last_us: Option<u64>,
    /// Smoothed gap between consecutive clock pulses, in microseconds.
    interval_us: Option<f64>,
    samples: u32,
    running: bool,
}

impl MidiClock {
    pub const fn new() -> Self {
        Self {
            last_us: None,
            interval_us: None,
            samples: 0,
            running: false,
        }
    }

    /// Whether the transport is running (a `Start`/`Continue`/`Clock` since the
    /// last `Stop`).
    pub fn running(&self) -> bool {
        self.running
    }

    /// Feed one real-time message stamped at `ts_us` (microseconds). Returns the
    /// current BPM when the clock is running with a stable estimate.
    pub fn feed(&mut self, ev: MidiRealtime, ts_us: u64) -> Option<f32> {
        match ev {
            MidiRealtime::Clock => {
                self.running = true;
                if let Some(last) = self.last_us {
                    let gap = ts_us.saturating_sub(last);
                    if (MIN_GAP_US..=MAX_GAP_US).contains(&gap) {
                        let g = gap as f64;
                        self.interval_us = Some(match self.interval_us {
                            Some(prev) => prev + ALPHA * (g - prev),
                            None => g,
                        });
                        self.samples += 1;
                    } else {
                        // A gap is a restart/jitter: re-seed rather than average in.
                        self.interval_us = None;
                        self.samples = 0;
                    }
                }
                self.last_us = Some(ts_us);
                self.bpm()
            }
            MidiRealtime::Start => {
                self.running = true;
                self.last_us = Some(ts_us);
                self.interval_us = None;
                self.samples = 0;
                None
            }
            MidiRealtime::Continue => {
                self.running = true;
                self.last_us = Some(ts_us);
                self.bpm()
            }
            MidiRealtime::Stop => {
                self.running = false;
                self.last_us = None;
                self.interval_us = None;
                self.samples = 0;
                None
            }
        }
    }

    fn bpm(&self) -> Option<f32> {
        if !self.running || self.samples < MIN_SAMPLES {
            return None;
        }
        let interval = self.interval_us?;
        Some((60_000_000.0 / (interval * PPQN)) as f32)
    }
}

/// Extract `(controller, value)` from a 3-byte Control Change message.
pub fn parse_cc(msg: &[u8]) -> Option<(u8, u8)> {
    if msg.len() == 3 && msg[0] & 0xF0 == 0xB0 {
        Some((msg[1] & 0x7F, msg[2] & 0x7F))
    } else {
        None
    }
}

/// Map a 7-bit CC value to 0–1.
pub fn cc_to_unit(value: u8) -> f32 {
    (value & 0x7F) as f32 / 127.0
}

/// Parse `midi.conf` → `(enabled, bindings, clock)`. Unknown lines/targets are
/// ignored; a missing or empty file yields the default (enabled, CC 11 → wah
/// treadle, clock off).
fn parse_midi_conf(text: &str) -> (bool, Vec<(u8, MidiTarget)>, bool) {
    let mut enabled = true;
    let mut clock = false;
    let mut bindings: Vec<(u8, MidiTarget)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "enabled" => enabled = !matches!(value, "false" | "0" | "no" | "off"),
            "clock" => clock = !matches!(value, "false" | "0" | "no" | "off"),
            // Legacy shorthand: `cc = <number>` binds the wah treadle.
            "cc" => {
                if let Ok(cc) = value.parse::<u8>()
                    && cc <= 127
                {
                    bindings.push((cc, MidiTarget::WahPosition));
                }
            }
            _ => {
                if let Ok(cc) = key.parse::<u8>()
                    && cc <= 127
                    && let Some(target) = MidiTarget::parse(value)
                {
                    bindings.push((cc, target));
                }
            }
        }
    }
    (enabled, bindings, clock)
}

/// Load `midi.conf`. A **missing** file gets the legacy default (enabled, wah on
/// CC 11); a file that exists but lists nothing stays empty, so a table the bind
/// screen has emptied is not silently re-seeded.
fn midi_config() -> MidiState {
    match midi_config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => MidiState::from_conf(&text),
        None => default_midi_state(),
    }
}

/// Build the shared state from `midi.conf` and connect if possible. The state is
/// **always** returned (even with no controller, or MIDI disabled) so the bind
/// screen can list and edit bindings.
pub fn start(params: Arc<Params>) -> (Arc<MidiState>, Option<MidiHandle>) {
    let state = Arc::new(midi_config());
    let handle = connect(&state, params);
    (state, handle)
}

/// Connect to the first available MIDI input and drive whatever the (live)
/// binding table says. Returns `None` (silently) if MIDI is disabled, no input
/// exists, or the connection fails — the app must run fine without a controller.
/// Also the manual reconnect path for the bind screen.
pub fn connect(state: &Arc<MidiState>, params: Arc<Params>) -> Option<MidiHandle> {
    if !state.enabled() {
        return None;
    }
    let clock = state.clock();

    let mut input = MidiInput::new("rusty-riff").ok()?;
    input.ignore(Ignore::None);
    let port = input.ports().into_iter().next()?;
    let name = input
        .port_name(&port)
        .unwrap_or_else(|_| "unknown".to_string());

    let p = params.clone();
    let bindings = state.bindings();
    let summary: Vec<String> = bindings.iter().map(|(cc, _)| format!("CC{cc}")).collect();
    let st = state.clone();
    let mut tempo = MidiClock::new();
    let conn = input
        .connect(
            &port,
            "rusty-riff-midi",
            move |ts, msg, _| {
                if clock && let Some(ev) = parse_realtime(msg) {
                    match tempo.feed(ev, ts) {
                        // A beat's worth of delay (the tapped-tempo equivalent).
                        Some(bpm) => {
                            let secs = 60.0 / bpm;
                            p.delay_time.store((secs / 0.5).clamp(0.0, 1.0), Relaxed);
                            p.midi_clock_bpm.store(bpm, Relaxed);
                        }
                        // Stopped: hand control back to the knob / tap.
                        None if !tempo.running() => p.midi_clock_bpm.store(0.0, Relaxed),
                        None => {}
                    }
                    return;
                }
                if let Some((controller, value)) = parse_cc(msg) {
                    // Publish to the learn channel first, so a screen that is
                    // arming sees the event even if the lock below is contended.
                    st.record_cc(controller, value);
                    let unit = cc_to_unit(value);
                    // `try_lock` only: the MIDI thread must never block. A CC
                    // dropped while the UI is mid-bind is inaudible.
                    if let Ok(bindings) = st.bindings.try_lock() {
                        for &(cc, target) in bindings.iter() {
                            if cc == controller {
                                target.write(&p, unit);
                            }
                        }
                    }
                }
            },
            (),
        )
        .ok()?;

    crate::audio::log_line(&format!(
        "MIDI input '{name}' bound to {}{}",
        if summary.is_empty() {
            "(nothing yet)".to_string()
        } else {
            summary.join(", ")
        },
        if clock { " + clock sync" } else { "" }
    ));
    Some(MidiHandle {
        _conn: conn,
        port: name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_control_change_only() {
        assert_eq!(parse_cc(&[0xB0, 11, 64]), Some((11, 64))); // CC on channel 0
        assert_eq!(parse_cc(&[0xB3, 11, 127]), Some((11, 127))); // any channel
        assert_eq!(parse_cc(&[0x90, 60, 100]), None); // note-on
        assert_eq!(parse_cc(&[0xB0, 11]), None); // too short
        assert_eq!(parse_cc(&[0xB0, 11, 64, 0]), None); // too long
    }

    #[test]
    fn cc_value_maps_to_unit_range() {
        assert_eq!(cc_to_unit(0), 0.0);
        assert_eq!(cc_to_unit(127), 1.0);
        assert!((cc_to_unit(64) - 64.0 / 127.0).abs() < 1e-6);
    }

    #[test]
    fn parses_real_time_bytes_only() {
        assert_eq!(parse_realtime(&[0xF8]), Some(MidiRealtime::Clock));
        assert_eq!(parse_realtime(&[0xFA]), Some(MidiRealtime::Start));
        assert_eq!(parse_realtime(&[0xFB]), Some(MidiRealtime::Continue));
        assert_eq!(parse_realtime(&[0xFC]), Some(MidiRealtime::Stop));
        assert_eq!(parse_realtime(&[0xF0]), None); // sysex start
        assert_eq!(parse_realtime(&[0xB0, 11, 64]), None); // not real-time
        assert_eq!(parse_realtime(&[]), None);
    }

    /// A steady 24-ppqn clock at a known tempo reads back that BPM.
    #[test]
    fn steady_clock_reports_bpm() {
        let mut clock = MidiClock::new();
        // 120 BPM → a beat (quarter) is 0.5 s → a clock pulse is 0.5/24 s
        // ≈ 20 833 µs.
        let step_us = 500_000u64 / 24;
        assert_eq!(clock.feed(MidiRealtime::Start, 0), None);
        let mut ts = 0u64;
        let mut last = None;
        for _ in 0..6 {
            ts += step_us;
            last = clock.feed(MidiRealtime::Clock, ts);
        }
        let bpm = last.expect("steady clock should yield a tempo");
        assert!((bpm - 120.0).abs() < 0.5, "got {bpm}");
        assert!(clock.running());
    }

    /// A clock is ignored until it has two valid intervals, and a wild gap
    /// re-seeds rather than skewing the average.
    #[test]
    fn clock_needs_stable_intervals() {
        let mut clock = MidiClock::new();
        assert_eq!(clock.feed(MidiRealtime::Clock, 0), None);
        assert_eq!(clock.feed(MidiRealtime::Clock, 20_833), None); // one interval
        // A 10 s gap is outside the accepted range → re-seed, still no estimate.
        assert_eq!(clock.feed(MidiRealtime::Clock, 10_020_833), None);
    }

    /// `Stop` clears the tempo and running state (the knob/tap take back over).
    #[test]
    fn stop_clears_the_tempo() {
        let mut clock = MidiClock::new();
        let step_us = 500_000u64 / 24;
        clock.feed(MidiRealtime::Start, 0);
        let mut ts = 0u64;
        for _ in 0..4 {
            ts += step_us;
            clock.feed(MidiRealtime::Clock, ts);
        }
        assert!(clock.running());
        assert_eq!(clock.feed(MidiRealtime::Stop, ts + step_us), None);
        assert!(!clock.running());
    }

    #[test]
    fn midi_conf_defaults_to_wah_on_cc11() {
        // A *missing* file yields the legacy default; the parser itself no longer
        // seeds, so a file that exists but lists nothing stays empty (see
        // `an_emptied_table_is_not_reseeded`).
        let d = default_midi_state();
        assert!(d.enabled());
        assert!(!d.clock());
        assert_eq!(d.bindings(), vec![(11, MidiTarget::WahPosition)]);
        assert_eq!(parse_midi_conf(""), (true, Vec::new(), false));
        assert!(
            !parse_midi_conf("enabled = false\n").0,
            "enabled flag ignored"
        );
    }

    #[test]
    fn target_name_round_trips() {
        for t in MidiTarget::ALL {
            assert_eq!(MidiTarget::parse(t.name()), Some(t), "{}", t.name());
            assert!(!t.label().is_empty(), "{} has no label", t.name());
        }
        assert_eq!(MidiTarget::ALL.len(), 15);
    }

    #[test]
    fn bind_clears_colliding_cc_and_target() {
        let s = MidiState::from_conf("");
        s.bind(20, MidiTarget::DelayMix);
        assert_eq!(s.bindings(), vec![(20, MidiTarget::DelayMix)]);
        // Re-binding the same CC to another target drops the old target.
        s.bind(20, MidiTarget::ReverbMix);
        assert_eq!(s.bindings(), vec![(20, MidiTarget::ReverbMix)]);
        // Binding the same target to another CC drops the old CC.
        s.bind(21, MidiTarget::ReverbMix);
        assert_eq!(s.bindings(), vec![(21, MidiTarget::ReverbMix)]);
    }

    #[test]
    fn unbind_removes_only_that_target() {
        let s = MidiState::from_conf("11 = wah_position\n20 = delay_mix\n");
        s.unbind(MidiTarget::WahPosition);
        assert_eq!(s.bindings(), vec![(20, MidiTarget::DelayMix)]);
        s.unbind(MidiTarget::TsDrive); // not bound: no-op
        assert_eq!(s.bindings(), vec![(20, MidiTarget::DelayMix)]);
    }

    #[test]
    fn conf_round_trips() {
        let s = MidiState::from_conf(
            "enabled = false\nclock = true\n11 = wah_position\n20 = delay_mix\n",
        );
        assert!(!s.enabled());
        assert!(s.clock());
        let text = render_conf(s.enabled(), s.clock(), &s.bindings());
        let back = MidiState::from_conf(&text);
        assert_eq!(back.enabled(), s.enabled());
        assert_eq!(back.clock(), s.clock());
        assert_eq!(back.bindings(), s.bindings());
    }

    #[test]
    fn an_emptied_table_is_not_reseeded() {
        // The screen rewrites the file when the user unbinds everything; coming
        // back up it must stay unbound rather than resurrect CC 11 → wah.
        let text = render_conf(true, false, &[]);
        assert!(!text.trim().is_empty(), "the writer must emit a real file");
        assert!(MidiState::from_conf(&text).bindings().is_empty());
    }

    #[test]
    fn midi_conf_parses_legacy_and_named_bindings() {
        let (en, b, clk) = parse_midi_conf("cc = 4\n");
        assert!(en);
        assert_eq!(b, vec![(4, MidiTarget::WahPosition)]);
        assert!(!clk);

        let (_, b, _) = parse_midi_conf("20 = delay_mix\n21 = reverb_mix\n");
        assert_eq!(
            b,
            vec![(20, MidiTarget::DelayMix), (21, MidiTarget::ReverbMix)]
        );

        // Mixed legacy + named; unknown target/cc ignored; clock opt-in honoured.
        let (_, b, clk) =
            parse_midi_conf("clock = true\ncc = 11\n7 = boost_gain\n8 = nope\n200 = delay_mix\n");
        assert!(clk);
        assert_eq!(
            b,
            vec![(11, MidiTarget::WahPosition), (7, MidiTarget::BoostGain)]
        );
    }

    #[test]
    fn cc_since_reports_only_new_events() {
        let s = MidiState::from_conf("");
        assert_eq!(s.cc_since(0), None, "nothing seen yet");
        s.record_cc(20, 100);
        assert_eq!(s.cc_since(0), Some((20, 100, 1)));
        assert_eq!(s.cc_since(1), None, "the same event must not re-fire");
    }

    #[test]
    fn cc_since_returns_the_latest() {
        let s = MidiState::from_conf("");
        s.record_cc(20, 10);
        s.record_cc(21, 90);
        assert_eq!(s.cc_since(0), Some((21, 90, 2)));
        // A reader that already saw the first event still gets the second.
        assert_eq!(s.cc_since(1), Some((21, 90, 2)));
    }

    #[test]
    fn start_always_returns_the_shared_state() {
        // Even with no port (or MIDI disabled), the UI gets a state so the bind
        // screen can list and edit bindings.
        let (state, _handle) = start(Arc::new(Params::new()));
        let expected = midi_config();
        assert_eq!(state.enabled(), expected.enabled());
        assert_eq!(state.clock(), expected.clock());
        assert_eq!(state.bindings(), expected.bindings());
    }
}
