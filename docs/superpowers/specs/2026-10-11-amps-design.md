# Amp gaps remainder (C2–C5) — design

**Status:** written for review; implements roadmap Item 3, third workstream
("C2–C5 amp gaps", `roadmap-next.md` + `docs/dsp-findings-2026-09.md` Phase C).
**Scope:** `src/dsp/amp/**` only. Each model independently listenable; the
baseline is re-blessed as tone moves (bless-now precedent from F1).

## Starting state: what already shipped (verify-and-close, not this work)

The findings doc predates most of the rework. Verified in code — all tested,
all staying:

- **C2 core:** `GlobalNfb` loop with LF-shaped divider + presence bleed
  (`amp/mod.rs:641`), `DynamicPresence` crossfading knob→open-loop lift by sag
  envelope (`:403`), loop authority falling via `sagged_rail` coupling.
  Master deliberately NOT in the divider (pre-phase-inverter volume —
  documented per model, e.g. `marshall.rs:25`).
- **C3:** rail-voltage sag via `sagged_rail` in all 8 tube models —
  collapsing supply compresses harder, small-signal gain untouched (tested).
- **C4 core:** parameterized `TubeClip` per valve (AX7/EL84/V6/V6_PREAMP +
  `PUSH_PULL` odd-dominant power stage), front-loaded `split_gain`,
  `GridBlock` flat-top at 8×, Mesa power stage on `PUSH_PULL` + rail sag.
  The 3/2 power law was measured and **deliberately not shipped**
  (memoryless clipper limit, documented in `fidelity-implement.md`).
- **C5 shipped:** Vox dead filter removed, Marshall docs corrected, OT
  non-linearity inside 8× `PowerOs`, shared `Components` deliberate
  (Mesa/Supro on FENDER, documented), tonestack has tests, Tweed single-Tone
  correct, master drives power-stage input on master models
  (no-master models documented), chain-level `HumNoise` floor (gate-nullable).
- **Randall** substantially fleshed out (NFB, OT, Baxandall, VoiceBalance).

## Intent

Close the actual remainder: the four models whose supplies never breathe.
Vox, Hiwatt, Fender and Randall have no `SupplyRipple` — and the Vox most of
all is hum-bearing in reality — while ripple elsewhere only rides load, so
idle never breathes anywhere. Plus one naming lie and whatever the listening
pass surfaces.

Success = every model's supply breathes under load in its own mains (50/100
vs 60/120 Hz already per model), the Vox hums most, idle stays clean, and
the baseline moves only where the new modulation lives before re-bless.

## Goals / non-goals

**Goals**
- R1 (RESCOPED by measurement — see plan ledger Task 1 ruling):
  `SupplyRipple` ships **only where the power stage clips hard enough to
  imprint it**: Randall (tanh rail, no cancellation — shipped, tested).
  Vox/Hiwatt/Fender keep NO ripple: at sibling depths their clean-running
  power stages cancel supply exactly (`sagged_rail` linear zone), so
  0.035–0.05 ripple is immeasurable (floors dominate 10–50x). Their ghost
  tests are committed `#[ignore]`d with the measured numbers, pending
  ear-set depths or a power-drive re-voice (out of scope). Ripple stays
  load-riding everywhere (no idle hum — gate + meter floor depend on it).
- R2: rename `SpeakerLoad::process(x, sag)` → `(x, drive_level)`: every
  caller passes the level envelope, not the rail. Pure rename + doc, no
  behavior.
- R3: listening pass over all 9 (human) + re-bless; Randall thinness and
  Tweed gaps are listen-first, code only if heard.

**Non-goals**
- No NFB topology changes (divider/master decisions stand).
- No 3/2 law (deliberately unshipped).
- No clipper-curve changes (C4 done).
- No idle-hum generator (idle silence is a feature: gate + meter floor).
- No tone-stack component changes.

**Anti-goals**
- No allocation/blocking/locking on the audio thread (ripple is phasor
  state, already the pattern).
- No per-model re-voicing beyond ripple depth (that is a new project, not
  a gap close).

## Constraints

- Gates before push: `cargo fmt --check`, `clippy --all-targets
  --all-features -D warnings`, `cargo test --release` + debug, fidelity
  `--check` (expected to move where ripple lives, then re-bless per
  CONTRIBUTING).
- Work directly on `main`; never push unprompted.
- Ripple depth per model is a listening value: start from sibling values
  (0.035–0.05), adjust by ear, record the choice in the model file.

## Design

### R1: ripple where the stage can imprint it (rescoped — see Decisions)

Randall (shipped): load-only envelope follower driving just the ripple
(rails stay stiff, no `sagged_rail`), 120 Hz US, depth 0.02, wired as a
rail-scale multiplier on the tanh — the tanh has no `sagged_rail`
cancellation, so the wobble imprints. Tested: ghost sidebands under load,
none at idle, rails-stay-stiff.

Vox/Hiwatt/Fender (NOT shipped — measured negative): the same wiring was
implemented, proven correct by a 10x-depth scaling probe (exactly 10x
sidebands, no structural block), and reverted — at sibling depths their
clean-running power stages cancel supply exactly, so the effect sits
10–50x under the models' floors. Ghost tests committed `#[ignore]`d with
measured numbers. Revisit only with ear-set depths (~10x) or a power-drive
re-voice. Mains Hz, had it shipped: Vox/Hiwatt 100 Hz UK, Fender 120 Hz US.

### R2: the rename

`SpeakerLoad::process(&mut self, x: f32, drive_level: f32)` + doc update
("level envelope, not the rail"). All 9 call sites unchanged (positional).
A rename-only commit, separately reviewable.

### R3: listening + re-bless

Human plays all 9 (or at minimum Vox + Randall + one reference, e.g.
Marshall), then `--write-baseline` + review the diff + commit. Ripple-only
movement expected (ghost sidebands around played notes, small LTAS wiggles
near 100/120 Hz + harmonics); LUFS must not move.

## Acceptance

- Unit: 4 new ripple tests (sidebands under load, silence at idle); all
  existing amp tests green (touch-sensitivity, presence authority, sag).
- Harness: movement confined to ripple bands; LUFS-I unmoved; re-blessed.
- Bench: phasor state per sample is noise (record anyway).
- The human's ear on Vox + Randall at minimum.

## Decisions (locked by the human, 2026-10-12)

0. **Randall: load-only envelope for the ripple.** Rails stay stiff, no
   `sagged_rail`; shallowest depth that still breathes.
1. **Ripple depths: sibling-matched guesses, adjusted by ear together.**
2. **Re-bless: one batch at the end.** Single reviewable diff.
