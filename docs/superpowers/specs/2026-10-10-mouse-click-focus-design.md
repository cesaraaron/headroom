# Mouse click-to-focus — design

**Status:** approved in conversation 2026-10-10; awaiting written-spec review.
**Scope:** a new *input subsystem* (mouse capture + hit-testing) that also changes the
`draw`↔input-loop interface.

## Intent

Add the mouse as a **parallel input to the keyboard**: click a panel to give it focus,
click a knob / pedal tile / ribbon stage / master cell to focus that cell, and in the
timeline click a track row to select it and click the ruler or waveform to seek.

It is **not** a replacement for the keyboard and **not** a way to edit values by mouse.
Success = a click produces exactly the focus state the keyboard would have reached, with
no surprises while overlays are open, and the audio path untouched.

## Decisions (agreed)

1. **Focus only.** A click moves keyboard-style focus (and, on the timeline, selects a
   track / seeks). No drag-to-adjust, no click-to-grab.
2. **Timeline = row-select + seek.** A click on a track row selects that track; a click
   on the ruler or waveform moves the playhead to that time.
3. **Overlays are keyboard-only.** While *any* modal / overlay is open, mouse events are
   ignored entirely.
4. **Mouse capture always on** while the app runs (standard). Trade-off: normal
   click-drag text selection is disabled; the terminal's Shift/Option selection still
   works. Accepted.

## Constraints

- UI-thread only; nothing on the audio thread changes.
- Must not fight the existing focus model — it reuses `focus`, `chain_cursor`,
  `header_on_master`, `panels`, `panel_mem`, and the timeline `Session::select` / seek.
- Overlay set today: help, preset browser, session browser, amp/cab/IR browsers,
  add-pedal, tuner, metronome, calibration, export, gain/move dialogs, unsaved prompt.

## Architecture

### Event plumbing
- Enable `crossterm` `EnableMouseCapture` at startup and `DisableMouseCapture` on exit,
  alongside `EnterAlternateScreen` / `enable_raw_mode` (and their teardown).
- The event loop handles `Event::Mouse(me)` next to `Event::Key(key)`.
  1. If any overlay is open → drop the event.
  2. Otherwise → hit-test the last frame's `HitMap` and apply the target.
- Only press events (`MouseEventKind::Down`) are acted on; move/scroll are ignored for
  now.

### Hit map (the interface change)
- `draw(...)` returns a `HitMap` it builds while rendering the **main panels**:
  `HitMap = Vec<(Rect, HitTarget)>`. It is small (tens of entries) and rebuilt each
  frame.
- The loop owns a `HitMap` (a plain `Vec`). The `terminal.draw` closure assigns
  `draw(...)`'s return into it (`let h = draw(...); …omit overlays…; *hit = h;`), then
  the next mouse event consults it.
- The overlays are rendered *after* `draw` inside the same closure; because overlays are
  keyboard-only, they contribute nothing to the map, and step "any overlay open → drop"
  means the map is never consulted while one is up.
- `HitTarget`:
  ```
  Panel(u8)                     // 1..=4, focus that panel (show if hidden)
  Knob(usize)                   // a KNOBS/pedal focus index
  ChainStage(ChainStage)        // a ribbon stage -> move the ribbon cursor there
  MasterCell                    // the ribbon's master header cell
  TimelineTrack(TrackId)        // select a track row
  TimelineSeek(TimelineAxis)    // seek to the clicked time (carries the axis)
  ```

### Focus actions
Reuse the exact keyboard semantics (no new focus rules):
- `Panel(n)` → `panels` show `n` if hidden; `focus = panel_entry(n, board, order)`.
  (Never hides — that is the number keys' job.)
- `Knob(i)` → `focus = Some(i)`. Reachable by construction: the region was drawn, so the
  knob is visible on the current board / amp model.
- `ChainStage(s)` → `chain_cursor = s`, `focus = Some(CHAIN_TILE)`,
  `header_on_master = false`.
- `MasterCell` → `focus = Some(CHAIN_TILE)`, `header_on_master = true`.
- Pedal tiles, amp knobs and cab/mic knobs record plain `Knob(i)` regions — no new logic.
- Every applied target also calls `panel_mem.remember(focus)` so `Tab` recall stays
  consistent with a keyboard visit.

### Timeline
- Each track row records `TimelineTrack(id)` over its row rect; applying it selects that
  row exactly as `↑/↓` does, and focuses the timeline panel.
- The plot region (ruler + waveform body) records `TimelineSeek(axis)` where `TimelineAxis`
  carries: the plot's left column `x0`, the time at that column `t0`, and
  `sec_per_col` — the same mapping the renderer already uses (playhead-centred zoom,
  gutter width, etc.). **This is recorded after the row rects** so a click on the
  waveform seeks while a click on the row's label/gutter selects the row.
- Applying a seek: `t = clamp(t0 + (col - x0) * sec_per_col, 0, extent)`, then the same
  seek the `←/→` keys use. `TimelineAxis::time_at(col)` and `col_at(time)` are pure and
  unit-tested round-trip.

### Testing
- `hit_test(&HitMap, col, row) -> Option<HitTarget>`: the **last** region containing the
  point wins, so the recorder must insert the more specific region last (a knob inside a
  panel, the timeline plot inside a row). Pure, unit-tested with synthetic rects and edge
  points (borders, gaps, overlap order).
- `TimelineAxis` round-trip: `col_at(time_at(c)) == c` within a pixel, clamping at both
  ends.
- Focus-application tests: applying `Panel`, `Knob`, `ChainStage`, `MasterCell` sets the
  expected `focus` / `chain_cursor` / `header_on_master` / panel visibility (reuse the
  existing `input.rs` test harness).
- Snapshot tests are unaffected (no rendering change).

## Out of scope (later layers)
- Drag-to-adjust, click-to-grab, scroll-wheel adjust/scroll.
- Clicks inside overlays (selecting preset/session rows, modal knobs).
- Hover highlight / cursor feedback.
- A runtime toggle for mouse capture.

## Risks
- **Hit map drift.** If a region is drawn without being recorded (or vice versa) a click
  silently misses or hits the wrong cell. Mitigation: record in the same helper that
  draws each cell (`knob_cell`, `render_pedal_tile`, the timeline row/plot), so the rect
  and the target are co-located; the hit-test tests pin the mapping.
- **Narrow terminals.** Regions can be zero-width; the recorder should skip empty rects.
- **Text selection.** Documented trade-off (Shift to select).
