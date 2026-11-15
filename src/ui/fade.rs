//! Panel focus cross-fade.
//!
//! A terminal has no alpha channel, so a fade is a colour interpolation, not an
//! opacity ramp. Every glyph that encodes focus already has two renderings — the
//! lit one and the receded one — and this module supplies the 0.0–1.0 *reveal*
//! the renderers blend between. Moving the focus from one panel to another ramps
//! the incoming panel up while the outgoing panel falls, so the highlight travels
//! instead of jumping.
//!
//! The state is a small, pure frame clock: the UI thread calls [`PanelFade::observe`]
//! once per frame before drawing, and the renderer samples [`PanelFade::reveal`].
//! Nothing here touches the audio thread or allocates.

use std::time::{Duration, Instant};

use super::input::panel_of;

/// How long a panel focus change takes. Short enough to read as a response to the
/// key press rather than an animation, long enough to be seen at 60 fps.
pub(super) const PANEL_FADE: Duration = Duration::from_millis(160);

/// One reveal per top-level panel. Index 0 is unused: panels are numbered 1–4,
/// matching [`panel_of`] and the number keys.
const PANEL_SLOTS: usize = 5;

/// The reveal slot of a focus. 0 (no panel lit) when nothing is focused — the
/// device picker and the early frame before focus is repaired render that way,
/// and `panel_of(None)` would otherwise claim the amp panel is lit.
fn slot(focus: Option<usize>) -> usize {
    focus.map_or(0, |f| panel_of(Some(f)) as usize)
}

/// Cross-fade state for the four top-level panels.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanelFade {
    /// Reveal of each panel when the current transition started.
    from: [f32; PANEL_SLOTS],
    /// Reveal each panel is heading toward (1.0 on the focused panel).
    to: [f32; PANEL_SLOTS],
    /// Per panel, the cell it keeps drawing lit while it recedes. Without it a
    /// panel would drop its focused knob/tile at full brightness and only then
    /// fade, which reads as a snap. One entry per panel so a held `Tab` can have
    /// several panels falling at once, each still showing its own cell.
    detail: [Option<usize>; PANEL_SLOTS],
    /// Focus observed on the previous frame, so a change is noticed centrally
    /// however it was caused (Tab, a number key, a click, a session load).
    last_focus: Option<usize>,
    started: Instant,
    running: bool,
}

impl PanelFade {
    /// Settled on `focus`: the app opens with no transition to play.
    pub(super) fn new(focus: Option<usize>) -> Self {
        let mut to = [0.0; PANEL_SLOTS];
        to[slot(focus)] = 1.0;
        Self {
            from: to,
            to,
            detail: [None; PANEL_SLOTS],
            last_focus: focus,
            started: Instant::now(),
            running: false,
        }
    }

    /// Notice a focus change and start (or restart) the transition. Called once
    /// per frame before drawing; cheap and a no-op while the panel is unchanged.
    ///
    /// Restarting mid-fade snapshots the *current* reveals as the new start, so a
    /// held `Tab` (or a fast click-through) never flashes a half-faded panel back
    /// to full.
    pub(super) fn observe(&mut self, focus: Option<usize>, now: Instant) {
        let panel = slot(focus);
        let mut to = [0.0; PANEL_SLOTS];
        to[panel] = 1.0;
        if to == self.to {
            self.last_focus = focus;
            return;
        }
        let previous = self.last_focus;
        self.from = self.snapshot(now);
        // The panel we are leaving (if any) keeps its detail lit while it fades.
        if let Some(left) = self
            .to
            .iter()
            .position(|&r| r > 0.0)
            .filter(|&p| p != panel)
        {
            self.detail[left] = previous;
        }
        self.to = to;
        self.started = now;
        self.running = true;
        self.last_focus = focus;
    }

    /// Land on `focus` with no transition. Used while animations are switched off
    /// (`F6`), so a focus change renders exactly as it did before the fade
    /// existed. Called instead of [`PanelFade::observe`], never alongside it.
    pub(super) fn jump(&mut self, focus: Option<usize>) {
        let mut to = [0.0; PANEL_SLOTS];
        to[slot(focus)] = 1.0;
        self.from = to;
        self.to = to;
        self.detail = [None; PANEL_SLOTS];
        self.last_focus = focus;
        self.running = false;
    }

    /// The cell the outgoing panel should keep drawing lit. Substituting it back
    /// in while the panel recedes is what makes the fade read as one motion; once
    /// the panel reaches zero the caller uses the live focus again, which is
    /// exactly the settled unfocused rendering.
    pub(super) fn focus_in(&self, panel: u8, live: Option<usize>, now: Instant) -> Option<usize> {
        let i = (panel as usize).min(PANEL_SLOTS - 1);
        // The panel being entered always draws the live cell; only a panel that
        // is on its way out substitutes the cell it was showing.
        if self.to[i] > 0.0 {
            return live;
        }
        match self.detail[i] {
            Some(focus) if self.reveal(panel, now) > 0.0 => Some(focus),
            _ => live,
        }
    }

    /// How lit `panel` is this frame: 1.0 focused, 0.0 receded, in between while
    /// the transition runs. Every focus-coloured glyph blends its two settled
    /// renderings by this value.
    pub(super) fn reveal(&self, panel: u8, now: Instant) -> f32 {
        let i = (panel as usize).min(PANEL_SLOTS - 1);
        let t = self.progress(now);
        self.from[i] + (self.to[i] - self.from[i]) * t
    }

    /// Whether a frame is still moving, so the event loop can poll faster than
    /// its idle tick and the fade gets real frames to play on.
    pub(super) fn running(&self, now: Instant) -> bool {
        self.running && now.saturating_duration_since(self.started) < PANEL_FADE
    }

    /// The eased 0.0–1.0 progress of the transition.
    fn progress(&self, now: Instant) -> f32 {
        if !self.running {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= PANEL_FADE {
            return 1.0;
        }
        smoothstep(elapsed.as_secs_f32() / PANEL_FADE.as_secs_f32())
    }

    fn snapshot(&self, now: Instant) -> [f32; PANEL_SLOTS] {
        let t = self.progress(now);
        let mut out = [0.0; PANEL_SLOTS];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.from[i] + (self.to[i] - self.from[i]) * t;
        }
        out
    }
}

/// Symmetric ease, so a cross-fade has no visible start or stop: `3t² − 2t³`.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::config::{ADD_TILE, CHAIN_TILE, PRACTICE_TILE};

    /// One cell per panel, for driving changes: ribbon, amp, timeline, rig.
    const RIBBON: Option<usize> = Some(CHAIN_TILE);
    const AMP: Option<usize> = Some(0);
    const TIMELINE: Option<usize> = Some(PRACTICE_TILE);
    const RIG: Option<usize> = Some(ADD_TILE);

    /// The settles are exact: a fresh fade reports the focused panel fully lit
    /// and every other panel fully receded, exactly as the pre-fade renderer.
    #[test]
    fn a_settled_fade_matches_the_old_boolean_focus() {
        let fade = PanelFade::new(TIMELINE);
        let now = Instant::now();
        assert_eq!(fade.reveal(1, now), 0.0);
        assert_eq!(fade.reveal(2, now), 0.0);
        assert_eq!(fade.reveal(3, now), 1.0);
        assert_eq!(fade.reveal(4, now), 0.0);
        assert!(!fade.running(now));
    }

    /// Halfway through, the panel being left and the panel being entered are both
    /// partially lit and sum to one — the highlight is in transit, not doubled.
    #[test]
    fn a_transition_cross_fades_between_the_two_panels() {
        let start = Instant::now();
        let mut fade = PanelFade::new(AMP);
        fade.observe(TIMELINE, start);
        let mid = start + PANEL_FADE / 2;
        let (a, b) = (fade.reveal(2, mid), fade.reveal(3, mid));
        assert!((a - 0.5).abs() < 1e-3, "outgoing panel: {a}");
        assert!((b - 0.5).abs() < 1e-3, "incoming panel: {b}");
        assert_eq!(a + b, 1.0);
        assert_eq!(fade.reveal(1, mid), 0.0, "uninvolved panels stay dark");
        assert_eq!(fade.reveal(4, mid), 0.0);
    }

    /// The transition ends exactly on the target and stops asking for frames.
    #[test]
    fn a_transition_settles_on_the_new_panel() {
        let start = Instant::now();
        let mut fade = PanelFade::new(AMP);
        fade.observe(RIG, start);
        let done = start + PANEL_FADE;
        assert_eq!(fade.reveal(2, done), 0.0);
        assert_eq!(fade.reveal(4, done), 1.0);
        assert!(!fade.running(done), "a settled fade must not spin the loop");
    }

    /// The outgoing panel keeps drawing its own cell while it recedes, and hands
    /// it back the moment it is dark — that hand-off is the settled rendering.
    #[test]
    fn the_outgoing_panel_keeps_its_cell_while_it_recedes() {
        let start = Instant::now();
        let mut fade = PanelFade::new(RIG);
        fade.observe(RIBBON, start);
        let rig = panel_of(RIG);
        let mid = start + PANEL_FADE / 2;
        assert_eq!(fade.focus_in(rig, RIBBON, mid), RIG);
        assert_eq!(
            fade.focus_in(rig, RIBBON, start + PANEL_FADE),
            RIBBON,
            "once dark, the panel uses the live focus again"
        );
        // The incoming panel always draws the live focus.
        assert_eq!(fade.focus_in(1, RIBBON, mid), RIBBON);
    }

    /// Retriggering mid-fade starts from the current reveals instead of jumping:
    /// a held `Tab` walks a ramp, it does not flash panels to full.
    #[test]
    fn a_retrigger_starts_from_the_current_reveals() {
        let start = Instant::now();
        let mut fade = PanelFade::new(AMP);
        fade.observe(TIMELINE, start);
        let mid = start + PANEL_FADE / 2;
        let before = fade.reveal(3, mid);
        fade.observe(RIG, mid);
        let after = fade.reveal(3, mid);
        assert!(
            (before - after).abs() < 1e-3,
            "the incoming panel must not jump: {before} -> {after}"
        );
        let amp = fade.reveal(2, mid);
        assert!((amp - 0.5).abs() < 1e-3, "the left panel keeps fading");
    }

    /// A panel entered while another is still falling is not a special case: both
    /// keep their own cell, and the panel being entered draws the live one. This
    /// is what a held `Tab` looks like.
    #[test]
    fn panels_falling_at_once_keep_their_own_cells() {
        let start = Instant::now();
        let mut fade = PanelFade::new(AMP);
        fade.observe(TIMELINE, start);
        let mid = start + PANEL_FADE / 2;
        fade.observe(RIG, mid);

        assert_eq!(
            fade.focus_in(2, RIG, mid),
            AMP,
            "the amp panel is still falling from its own cell"
        );
        assert_eq!(
            fade.focus_in(3, RIG, mid),
            TIMELINE,
            "the timeline is still falling from its own cell"
        );
        assert_eq!(
            fade.focus_in(4, RIG, mid),
            RIG,
            "the panel being entered draws the live cell"
        );
    }

    /// With animations off the clock is parked (`jump`), so every panel reads its
    /// settled reveal immediately — the pre-animation rendering — and nothing asks
    /// the loop for extra frames.
    #[test]
    fn a_jump_settles_immediately() {
        let start = Instant::now();
        let mut fade = PanelFade::new(AMP);
        fade.observe(TIMELINE, start);
        let mid = start + PANEL_FADE / 2;
        assert!(fade.reveal(2, mid) > 0.0, "a transition is under way");

        fade.jump(RIG);
        assert_eq!(fade.reveal(2, mid), 0.0);
        assert_eq!(fade.reveal(3, mid), 0.0);
        assert_eq!(fade.reveal(4, mid), 1.0);
        assert!(!fade.running(mid));
        assert_eq!(fade.focus_in(3, RIG, mid), RIG, "no detail hand-off");
    }

    /// `smoothstep` is symmetric, monotone, and pinned at both ends.
    #[test]
    fn the_ease_is_symmetric_and_pinned() {
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(0.5), 0.5);
        assert!((smoothstep(0.25) + smoothstep(0.75) - 1.0).abs() < 1e-6);
        let mut last = -1.0;
        for i in 0..=100 {
            let v = smoothstep(i as f32 / 100.0);
            assert!(v >= last, "the ease must not go backwards");
            last = v;
        }
        assert_eq!(smoothstep(-3.0), 0.0, "clamped");
        assert_eq!(smoothstep(9.0), 1.0);
    }
}
