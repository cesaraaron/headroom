# MIDI learn / bind screen — design

**Status:** defaults chosen by the assistant (human opted out of the design
questions); written for review.
**Scope:** an in-app MIDI binding screen, plus the small `src/midi.rs` changes it
needs (live bindings, a learn channel, config write-back). No audio-path change.

## Intent

Today a CC can only be bound by hand-editing `~/.config/headroom/midi.conf`
(`src/midi.rs` reads it once at startup and the binding list is frozen inside the
`midir` callback). The single highest-friction gap called out in the module doc is
"a learn/bind screen is not built yet".

This adds one: a modal that lists the bindable targets, shows what each is bound
to, and lets you **press Enter on a target and move the physical control** to bind
it — no file editing.

Success = a player can point their pedal at the wah (or a knob at the delay mix)
without leaving the app, and the binding survives a restart.

## Goals / non-goals

**Goals**
- List every bindable target and its current CC.
- Learn a binding from the physical control; unbind.
- Persist to the existing `midi.conf`; the running connection picks changes up
  immediately (no restart).
- Work on a fresh machine (no `midi.conf`): the app already defaults to
  `enabled = true`, so a controller can be learned straight away.

**Non-goals**
- Editing `clock` (MIDI-clock sync) or `enabled` from the screen — config-only.
- Note / program-change / pitch-bend mappings; only the 15 named CC targets.
- Binding arbitrary knobs (only the existing `MidiTarget` set).
- A MIDI monitor / live CC readout beyond what Learn needs.

**Anti-goals**
- Anything on the audio thread. MIDI already runs on its own `midir` thread; the
  callback only does atomic stores. That discipline is preserved: the callback
  must never block, so it reads the binding table with `try_lock` and skips a CC
  if the UI happens to hold the lock (a dropped CC during a bind is inaudible).

## Constraints

- **No audio-thread work, no locks in the audio path.** Unchanged from today.
- The MIDI callback must not block: `try_lock` only.
- The UI owns all modal state (UI thread), like the other modals.
- The binding table is small (≤15) and changes only on user action.

## Design

### Shared state (new, in `src/midi.rs`)

```rust
/// Live MIDI input state shared between the `midir` callback and the UI.
pub struct MidiState {
    /// The active cc → target table. Replaced wholesale on bind/unbind.
    bindings: Mutex<Vec<(u8, MidiTarget)>>,
    /// `enabled` / `clock` as read from `midi.conf`, preserved on write-back.
    enabled: bool,
    clock: bool,
    /// Monotonic count of CC events, and the packed last one — the learn channel.
    cc_seq: AtomicU64,
    last_cc: AtomicU32, // (cc << 8) | value
}
```

- `MidiState::bind(cc, target)` — removes any other binding of `cc` *and* any other
  binding of `target`, then inserts `(cc, target)`. One CC per target, one target
  per CC.
- `MidiState::unbind(target)` — drops the target's binding.
- `MidiState::bindings() -> Vec<(u8, MidiTarget)>` — a snapshot for the list.
- `MidiState::cc_seq() -> u64` — read before arming Learn.
- `MidiState::cc_since(seq) -> Option<(u8, u8, u64)>` — the learn read: `None`
  when `cc_seq == seq`, else `(cc, value, new_seq)` from `last_cc`. The caller
  stores `new_seq` so a second event doesn't re-fire.
- `MidiState::write_conf()` — rewrites `~/.config/headroom/midi.conf` from the
  table (`enabled`, `clock`, then one `<cc> = <target>` line per binding), via a
  temp file + rename so a crash mid-write can't truncate it.

### Connection lifecycle

`start(params)` → `(Arc<MidiState>, Option<MidiHandle>)`. It **always** returns the
shared state (loaded from `midi.conf`), and connects when `enabled` and a port
exists — even with an empty binding table, so a fresh config can learn. The state
therefore exists with or without a controller, which is what lets the screen list
and edit bindings even when nothing is plugged in.

`MidiHandle` keeps only the `midir` connection alive and its `port` name;
`MidiState` is shared with the UI.

`connect(&state, params)` → `Option<MidiHandle>` (the reconnect path). The callback
closure captures `Arc<MidiState>` + `Arc<Params>` and, per message, either feeds
the clock (unchanged) or, for a CC: store `last_cc` + bump `cc_seq`, then walk the
binding table under `try_lock`.

The UI holds `Arc<MidiState>` for the whole run and `Option<MidiHandle>` that `R`
can replace via `connect`. When `enabled` is false the title reads `MIDI off` and
the same screen still edits bindings — they simply take effect once the conf is
re-enabled (the design does **not** silently flip `enabled`, so an explicit opt-out
is honoured).

### The modal (`src/ui/midi.rs`, new)

Opened/closed with **`F2`** (every letter, digit and common punctuation key is
already bound; function keys are free). Rendered like the metronome modal
(`Clear` + double border + `centered_rect`).

```
┌ M I D I ────────────────────┐  ← title; right-aligned: connected port or "no input"
│  ▲/▼  Enter bind  D unbind   │
│                              │
│  Wah position        CC 11   │  ← cursor row is highlighted
│  Wah mode (manual)   —       │
│  Delay time          move a  │  ← learning: this row shows the prompt
│  …                           │
└──────────────────────────────┘
```

Keys: `↑`/`↓` move (wrapping); `Enter` starts Learn on the cursor target;
`D`/`Delete` unbinds the cursor target; `R` reconnects; `Esc` cancels Learn, or
closes the modal when not learning; `F2` closes.

Every mutating action calls `state.write_conf()`.

### Learn state machine

`Learn::Idle` or `Learn::Armed { target, seq }`. On `Enter`,
`seq = state.cc_seq()`. Each loop iteration, if armed and `cc_since(seq)` yields
`(cc, _, _)`, then `state.bind(cc, target)`, `write_conf()`, and return to `Idle`.
(First CC after arming wins; a jittering pedal therefore binds on its first event
— good enough, and re-learn is one keypress.)

### Wiring (`src/ui/mod.rs`)

- `main.rs` drops `let _midi = …` and passes the `Option<(Arc<MidiState>, MidiHandle)>`
  into `run` (new last parameter).
- The loop keeps `midi_open: bool`, `midi_cursor: usize`, `midi_learn: Learn`.
- `F2` toggles the modal; while open, the letter/number keys that normally drive
  the app are suppressed and the modal handles `↑↓ Enter D R Esc F2` (the same
  "modal owns the keys" shape as the other browsers).
- The learn poll runs once per loop iteration, before drawing.
- The modal joins the **overlay guard** flag list and the draw closure.

### Help / docs

- `K` reference: a `F2  MIDI bindings` row.
- README "Essential keys" + `docs/guide.md` (there is an
  "Expression pedal (MIDI)" section to extend).

## Testing

- `src/midi.rs` (unit): `bind` clears colliding cc/target; `unbind`; the conf
  round-trips (`parse(write(state)) == state` for enabled/clock/bindings);
  `target` config-name ↔ parse round-trip; `take_cc_since` returns nothing when
  the seq is current and the packed `(cc, value)` otherwise.
- `src/ui/midi.rs` (unit): cursor wraps; `Enter` arms Learn; a simulated CC binds
  it and writes the expected line; `D` unbinds; `Esc` cancels learn before
  closing. Plus one snapshot of the rendered modal.
- `src/ui/mod.rs`: extend the overlay-guard test data / add the modal to the
  documented list.

## Out of scope (later)

- Reconnect-in-background / hot-plug polling (today `R` is a manual reconnect).
- A live CC monitor, note/PC mappings, per-preset bindings, MIDI out.
