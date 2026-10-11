//! The MIDI bind screen (`F2`): list the bindable targets, learn a binding from
//! the physical control, unbind, and persist to `midi.conf`.
//!
//! The modal owns the cursor and the learn state; every mutating action reports
//! `true` so the caller persists. Persistence lives in `crate::midi`, so the UI
//! tests here never touch the user's config.

use std::sync::Arc;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::midi::{MidiState, MidiTarget};

use super::styles::{ACCENT, AMBER, CHROME, DIM, HOT, SAFE, panel_style};

/// What the modal is waiting for: nothing, or a physical control to move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Learn {
    Idle,
    Armed { target: MidiTarget, seq: u64 },
}

/// UI state for the MIDI bind screen. Owned by the UI thread.
pub(super) struct MidiUi {
    pub(super) open: bool,
    pub(super) cursor: usize,
    learn: Learn,
}

impl MidiUi {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            cursor: 0,
            learn: Learn::Idle,
        }
    }

    /// The target under the cursor.
    pub(super) fn cursor_target(&self) -> MidiTarget {
        MidiTarget::ALL[self.cursor.min(MidiTarget::ALL.len() - 1)]
    }

    /// The target currently waiting for a control, if any.
    pub(super) fn learning(&self) -> Option<MidiTarget> {
        match self.learn {
            Learn::Armed { target, .. } => Some(target),
            Learn::Idle => None,
        }
    }

    /// Move the cursor, wrapping at both ends.
    pub(super) fn move_cursor(&mut self, dir: i32) {
        let n = MidiTarget::ALL.len() as i32;
        self.cursor = (((self.cursor as i32 + dir.signum()) % n + n) % n) as usize;
    }

    /// Handle one key while the modal owns input. Returns `true` when the binding
    /// table changed and the caller should persist it.
    pub(super) fn handle_key(&mut self, code: KeyCode, state: &Arc<MidiState>) -> bool {
        match code {
            KeyCode::Up => {
                self.move_cursor(-1);
                false
            }
            KeyCode::Down => {
                self.move_cursor(1);
                false
            }
            KeyCode::Enter => {
                let target = self.cursor_target();
                self.learn = Learn::Armed {
                    target,
                    seq: state.cc_seq(),
                };
                false
            }
            // `F2` toggles the screen, so a second press closes it (the caller
            // opens it; this arm only ever runs while it is open).
            KeyCode::F(2) => {
                self.open = false;
                false
            }
            // `D` on the pedalboard removes a pedal, so the modal's unbind is
            // `D` here too, plus `Delete`.
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                state.unbind(self.cursor_target());
                true
            }
            KeyCode::Esc => {
                // Esc cancels Learn first, so a stray key never closes the screen.
                if self.learning().is_some() {
                    self.learn = Learn::Idle;
                } else {
                    self.open = false;
                }
                false
            }
            _ => false,
        }
    }

    /// Bind the target being learned to the first CC seen since arming. Polled
    /// once per frame; returns `true` when a binding was made.
    pub(super) fn pump(&mut self, state: &Arc<MidiState>) -> bool {
        let Learn::Armed { target, seq } = self.learn else {
            return false;
        };
        let Some((cc, _value, _seq)) = state.cc_since(seq) else {
            return false;
        };
        state.bind(cc, target);
        self.learn = Learn::Idle;
        true
    }
}

/// Render the MIDI bind modal. `port` is the connected input's name, if any.
pub(super) fn render_midi(f: &mut Frame, ui: &MidiUi, state: &MidiState, port: Option<&str>) {
    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let status = match port {
        Some(name) => format!(" {name} "),
        None if state.enabled() => " no input ".to_owned(),
        None => " MIDI off ".to_owned(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " M I D I ",
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ))
        .title(Line::from(Span::styled(status, Style::default().fg(DIM))).right_aligned())
        .style(panel_style());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint
            Constraint::Min(1),    // targets
            Constraint::Length(1), // footer
        ])
        .split(inner);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ↑↓ · Enter bind · D unbind · R reconnect · Esc",
            Style::default().fg(DIM),
        ))),
        rows[0],
    );

    let bindings = state.bindings();
    let learning = ui.learning();
    let lines: Vec<Line> = MidiTarget::ALL
        .iter()
        .enumerate()
        .map(|(i, &target)| {
            let selected = i == ui.cursor;
            let name_style = if selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(CHROME)
            };
            let marker = if selected { "▶" } else { " " };
            let right = if learning == Some(target) {
                Span::styled("move the control…", Style::default().fg(HOT))
            } else {
                match bindings.iter().find(|(_, t)| *t == target) {
                    Some((cc, _)) => Span::styled(format!("CC {cc}"), Style::default().fg(SAFE)),
                    None => Span::styled("—", Style::default().fg(DIM)),
                }
            };
            Line::from(vec![
                Span::styled(format!(" {marker} {:<22}", target.label()), name_style),
                right,
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), rows[1]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "bound to ~/.config/rusty-riff/midi.conf",
            Style::default().fg(DIM),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );
}

/// A centred box `percent_x` × `percent_y` of the screen (the metronome modal's
/// helper, kept local to this module).
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let width = (area.width * percent_x / 100).max(40).min(area.width);
    let height = (area.height * percent_y / 100).max(8).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(conf: &str) -> Arc<MidiState> {
        Arc::new(MidiState::from_conf(conf))
    }

    fn index_of(target: MidiTarget) -> usize {
        MidiTarget::ALL
            .iter()
            .position(|t| *t == target)
            .expect("target is in ALL")
    }

    #[test]
    fn cursor_wraps_both_ways() {
        let mut ui = MidiUi::new();
        ui.move_cursor(-1);
        assert_eq!(ui.cursor, MidiTarget::ALL.len() - 1);
        ui.move_cursor(1);
        assert_eq!(ui.cursor, 0);
    }

    #[test]
    fn enter_arms_learn_on_the_cursor_target() {
        let mut ui = MidiUi::new();
        let s = state("");
        ui.move_cursor(1);
        ui.handle_key(KeyCode::Enter, &s);
        assert_eq!(ui.learning(), Some(ui.cursor_target()));
    }

    #[test]
    fn a_cc_binds_the_armed_target() {
        let mut ui = MidiUi::new();
        let s = state("");
        ui.cursor = index_of(MidiTarget::DelayMix);
        ui.handle_key(KeyCode::Enter, &s);
        s.record_cc(20, 100);
        assert!(ui.pump(&s), "binding is a change");
        assert_eq!(s.bindings(), vec![(20, MidiTarget::DelayMix)]);
        assert_eq!(ui.learning(), None, "learning ends once bound");
        // No further CC: nothing to pump.
        assert!(!ui.pump(&s));
    }

    #[test]
    fn d_unbinds_the_cursor_target() {
        let mut ui = MidiUi::new();
        let s = state("20 = delay_mix\n");
        ui.cursor = index_of(MidiTarget::DelayMix);
        assert!(ui.handle_key(KeyCode::Delete, &s));
        assert!(s.bindings().is_empty());
    }

    #[test]
    fn esc_cancels_learn_before_closing() {
        let mut ui = MidiUi::new();
        ui.open = true;
        let s = state("");
        ui.handle_key(KeyCode::Enter, &s);
        assert!(ui.learning().is_some());
        ui.handle_key(KeyCode::Esc, &s);
        assert!(ui.open, "the first Esc only cancels Learn");
        assert_eq!(ui.learning(), None);
        ui.handle_key(KeyCode::Esc, &s);
        assert!(!ui.open);
    }

    #[test]
    fn f2_closes_the_modal() {
        let mut ui = MidiUi::new();
        ui.open = true;
        let s = state("");
        ui.handle_key(KeyCode::F(2), &s);
        assert!(!ui.open, "F2 must toggle the screen closed");
    }

    fn screen_text(term: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_targets_bindings_and_the_learn_prompt() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let s = state("11 = wah_position\n20 = delay_mix\n");
        let mut ui = MidiUi::new();
        ui.open = true;
        ui.cursor = index_of(MidiTarget::DelayMix);
        let mut term = Terminal::new(TestBackend::new(90, 30)).expect("test backend");
        term.draw(|f| render_midi(f, &ui, &s, Some("Arturia KeyStep")))
            .expect("draw");
        let text = screen_text(&term);
        assert!(text.contains("M I D I"), "title missing: {text}");
        assert!(text.contains("Arturia KeyStep"), "port missing: {text}");
        assert!(text.contains("Wah position"), "labels missing: {text}");
        assert!(text.contains("CC 11"), "binding missing: {text}");
        assert!(text.contains('—'), "unbound targets read as a dash");

        // Arming a row shows the learn prompt on it.
        ui.handle_key(KeyCode::Enter, &s);
        term.draw(|f| render_midi(f, &ui, &s, Some("Arturia KeyStep")))
            .expect("draw");
        let armed = screen_text(&term);
        assert!(armed.contains("move the control"), "learn prompt: {armed}");
        insta::assert_snapshot!("midi_modal", armed);
    }
}
