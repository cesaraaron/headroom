//! Mouse hit-testing helpers for click-to-focus.
//!
//! `draw` records the interactive regions it renders into a [`HitMap`], and the
//! event loop resolves a mouse press against it with [`hit_test`]. A
//! [`TimelineAxis`] carries the waveform's column→frame mapping so a click seeks
//! to the time under the cursor.

use crate::dsp::ChainStage;
use ratatui::layout::Rect;

/// What a recorded region does when clicked. Applied through the existing focus
/// model by [`crate::ui::input::apply_hit`] (or, for the timeline targets, by the
/// event loop).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum HitTarget {
    /// Focus panel `n` (1 ribbon, 2 amp/cab, 3 timeline, 4 pedalboard), showing
    /// it first if it is hidden.
    Panel(u8),
    /// Focus the knob / tile at this focus index.
    Knob(usize),
    /// Move the ribbon cursor onto this stage.
    ChainStage(ChainStage),
    /// Move the ribbon cursor onto the master-output cell.
    MasterCell,
    /// Select this timeline track row.
    TimelineTrack(u64),
    /// Seek to the time under the clicked column.
    TimelineSeek(TimelineAxis),
}

/// The interactive regions of one frame, in draw order. `hit_test` returns the
/// **last** match, so the more specific region must be recorded last.
pub(super) type HitMap = Vec<(Rect, HitTarget)>;

/// Resolve a terminal cell to the last region that contains it, or `None` when
/// the point is in a border, a gap, or off every recorded region.
pub(super) fn hit_test(map: &HitMap, col: u16, row: u16) -> Option<&HitTarget> {
    map.iter()
        .rev()
        .find(|(rect, _)| rect.width > 0 && rect.height > 0 && rect.contains((col, row).into()))
        .map(|(_, target)| target)
}

/// The waveform's column↔frame mapping, captured while rendering so a click maps
/// to the same time the playhead line would occupy at that column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TimelineAxis {
    /// Left column of the waveform area.
    pub(super) x0: u16,
    /// First frame shown at column `x0`.
    pub(super) start: usize,
    /// Number of frames the waveform area spans.
    pub(super) len: usize,
    /// Width of the waveform area in columns.
    pub(super) wave_w: usize,
}

impl TimelineAxis {
    /// Frame under column `col`, clamped into `[start, start + len)`. Columns
    /// left of `x0` map to the window start; columns past its right edge clamp to
    /// the last frame.
    pub(super) fn frame_at(&self, col: u16) -> usize {
        let span = self.wave_w.saturating_sub(1).max(1) as i64;
        let len = self.len as i64;
        let start = self.start as i64;
        let offset = (i64::from(col) - i64::from(self.x0)).max(0);
        let frame = start + offset * len / span;
        let hi = (start + len - 1).max(start);
        frame.clamp(start, hi) as usize
    }

    /// Column at which `frame` is drawn (the inverse of [`frame_at`]). Only used
    /// by tests today; the renderer still computes the playhead column inline.
    #[cfg(test)]
    pub(super) fn col_at(&self, frame: usize) -> u16 {
        let span = self.wave_w.saturating_sub(1).max(1) as i64;
        let len = (self.len.max(1)) as i64;
        let offset = (frame as i64 - self.start as i64).max(0);
        self.x0 + (offset * span / len) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn axis() -> TimelineAxis {
        TimelineAxis {
            x0: 5,
            start: 1000,
            len: 48_000,
            wave_w: 100,
        }
    }

    #[test]
    fn axis_round_trips() {
        let a = axis();
        for col in a.x0..=a.x0 + 99 {
            let frame = a.frame_at(col);
            let back = a.col_at(frame);
            assert!(
                (i32::from(back) - i32::from(col)).abs() <= 1,
                "col {col} -> frame {frame} -> col {back}"
            );
        }
    }

    #[test]
    fn axis_clamps() {
        let a = axis();
        let lo = a.frame_at(a.x0.saturating_sub(3));
        let hi = a.frame_at(a.x0 + 200);
        assert!((a.start..a.start + a.len).contains(&lo), "lo out of range: {lo}");
        assert!((a.start..a.start + a.len).contains(&hi), "hi out of range: {hi}");
    }

    #[test]
    fn hit_test_returns_last_match() {
        let map: HitMap = vec![
            (Rect::new(0, 0, 10, 10), HitTarget::Panel(1)),
            (Rect::new(2, 2, 4, 4), HitTarget::Panel(2)),
        ];
        assert_eq!(hit_test(&map, 3, 3), Some(&HitTarget::Panel(2)));
        assert_eq!(hit_test(&map, 9, 1), Some(&HitTarget::Panel(1)));
        assert_eq!(hit_test(&map, 50, 50), None);
    }

    #[test]
    fn hit_test_ignores_zero_width() {
        let map: HitMap = vec![(Rect::new(0, 0, 0, 10), HitTarget::Panel(1))];
        assert_eq!(hit_test(&map, 0, 0), None);
    }
}
