# Gilmour Tier 1 Gear — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the five Tier 1 Gilmour gaps — compressor voices, Big Muff models, ProCo RAT, BK Butler Tube Driver, and an RA-200 rotary — without changing any existing preset's sound.

**Architecture:** Two parameter additions on existing pedals (compressor voice, fuzz model) and three new chain stages (RAT, Tube Driver, Rotary). New stages are appended to `ChainStage` so no persisted id moves; the UI/table order is edited in the same commit as the DSP. New effects default off and are skipped before `process`, so the fidelity baseline is untouched in phases 1–5. Content (new presets and chain-order experiments) is a separate task that never overwrites the eight bundled presets.

**Tech Stack:** Rust 1.96+, ratatui TUI, lock-free `Params` (`Arc<AtomicF32/Bool>`), 4×/8× oversampling, in-module `#[cfg(test)]`.

**Spec:** `docs/research/2026-11-02-gilmour-gear-gap-analysis.md` (approved 2026-11-02), with the two reviews beside it.

## Global Constraints

- **No heap allocation and no blocking on the audio thread.** Buffers live in `new()`; `process` only writes into pre-allocated `Vec`s.
- **New effects default `enabled = false`** and must be skipped before `process` (the chain already does this).
- **Voice/model 0 of an existing effect is bit-identical to today's code path.** If `fidelity_render -- --check` moves, the change is a bug — fix the code, do not re-bless.
- **Append new `ChainStage` ids; never renumber.** Ids must stay contiguous `0..CHAIN_LEN` with no holes.
- **Every new preset field is `#[serde(default = "...")]`** so old files load at the old behaviour; every new section disables its stage when absent.
- **Keep new pedals to ≤5 knobs.**
- **Update the tripwires in the same commit** (`src/ui/config.rs` pedal/knob counts, `src/midi.rs` target count if touched).
- **Only Task 6 may re-bless the fidelity baseline**, and only if a bundled preset's audio actually changes.
- Do not touch the pre-existing uncommitted change in `src/ui/draw.rs` unless the task requires it (then keep the user's edit).

## Review Focus

- A preset written before this work loads and sounds exactly as before (`voice = 0` / `model = 0`).
- A preset with an explicit `[chain]` does not silently get the rotary after the reverb.
- A new pedal never leaks “on” from a previous preset when the new preset omits its section.
- Reordering while the rotary is ringing clears the tail instead of folding it into the new topology.
- Placing the stereo rotary before the amp is either rejected or folded sanely, never silently corrupting the mono chain.

---

### Task 1: Compressor voices (Dyna Comp / CS-2)

**Files:**
- Modify: `src/dsp/effects/compressor.rs` (detector/gain computer, `process` signature, tests)
- Modify: `src/dsp/mod.rs` (Params field/default/new/reset, `mono_stage!` args)
- Modify: `src/ui/config.rs` (one `KNOBS` row, tripwire)
- Modify: `src/preset.rs` (`CmpSection.voice`, `from_params`, `apply`, `rig_values`, `hostile_params`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Compressor::process(&mut self, x: f32, sustain: f32, attack: f32, level: f32, voice: f32) -> f32`; `Params.cmp_voice: Arc<AtomicF32>`; preset `[compressor] voice` (default `0.0`).

- [ ] **Step 1: Write the failing test** in `compressor.rs`:

```rust
#[test]
fn dyna_and_cs2_voices_are_distinct() {
    // Same input, same knobs: CS-2 is darker/smoother than the Dyna.
    let run = |voice: f32| {
        let mut c = Compressor::new(SR);
        let mut out = Vec::new();
        for n in 0..SR as usize {
            let x = if n % 4800 == 0 { 0.8 } else { 0.05 };
            out.push(c.process(x, 0.6, 0.3, 0.5, voice));
        }
        out
    };
    let dyna = run(0.0);
    let cs2 = run(1.0);
    let diff: f32 = dyna.iter().zip(&cs2).map(|(a, b)| (a - b).abs()).sum();
    assert!(diff > 1.0, "voices are not audibly distinct");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib dsp::effects::compressor::tests::dyna_and_cs2_voices_are_distinct`
Expected: FAIL — `process` takes 4 arguments.

- [ ] **Step 3: Implement the voice branch.** Add `voice: f32` as the fifth parameter. `voice < 0.5` keeps today's exact branch (hard knee, 150 ms release, no colour). `voice >= 0.5` (CS-2) uses a soft knee of 6 dB, a release of 250 ms, and a post-gain fixed −1.5 dB high shelf at 3 kHz. Keep every existing helper untouched for the Dyna path.

- [ ] **Step 4: Wire the parameter** through `dsp/mod.rs` (`DEFAULT_CMP_VOICE: f32 = 0.0`, field, `Params::new`, `reset_to_defaults`, macro args), `ui/config.rs` (a `KNOBS` row labelled `VOICE` after `LEVEL`), and `preset.rs` (`CmpSection.voice` with `#[serde(default = "cmp_voice_default")]`, read in `from_params`, written in `apply`, included in `rig_values`, scrambled in `hostile_params`).

- [ ] **Step 5: Bump the tripwires** — `KNOBS.len()` 84 → 85 in `src/ui/config.rs::table_sizes_are_stable`.

- [ ] **Step 6: Run the full gate**

Run: `cargo test --lib && cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS; the five existing compressor tests pass **unchanged** (they are the bit-exactness gate for voice 0).

- [ ] **Step 7: Fidelity check**

Run: `cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml`
Expected: PASS with the baseline untouched.

- [ ] **Step 8: Commit**

```bash
git add src/dsp/effects/compressor.rs src/dsp/mod.rs src/ui/config.rs src/preset.rs
git commit -m "feat(dsp): add Dyna Comp / CS-2 compressor voices"
```

---

### Task 2: Big Muff models (Ram's Head / Triangle / Sovtek / Cornish P-2)

**Files:**
- Modify: `src/dsp/effects/fuzz.rs` (model table, scoop rebuild, `process` signature, tests)
- Modify: `src/dsp/mod.rs` (Params field/default/new/reset, `mono_stage!` args)
- Modify: `src/ui/config.rs` (one `KNOBS` row, tripwire)
- Modify: `src/preset.rs` (`FuzzSection.model`, four sites)

**Interfaces:**
- Consumes: Task 1's pattern only.
- Produces: `Fuzz::process(&mut self, x, fuzz, tone, level, kind, guitar, model) -> f32`; `Params.fz_model`; preset `[fuzz] model` (default `0.0` = Ram's Head).

- [ ] **Step 1: Write the failing test** in `fuzz.rs`:

```rust
#[test]
fn muff_models_are_distinct_and_bounded() {
    let run = |model: f32| {
        let mut f = Fuzz::new(SR);
        let mut out = Vec::new();
        for n in 0..4800 {
            let x = (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin() * 0.3;
            out.push(f.process(x, 0.7, 0.5, 0.6, 0.0, 1.0, model));
        }
        out
    };
    let a = run(0.0);
    let b = run(1.0);
    assert!(a.iter().all(|x| x.is_finite() && x.abs() < 8.0));
    let diff: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
    assert!(diff > 0.5, "Triangle and Ram's Head are identical");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib dsp::effects::fuzz::tests::muff_models_are_distinct_and_bounded`
Expected: FAIL — `process` takes 6 arguments.

- [ ] **Step 3: Implement the model table.** Add a `MuffModel` const table selected by `model` (`<0.25` Ram's Head, `<0.5` Triangle, `<0.75` Sovtek, else Cornish P-2). Row 0 is **exactly today's literals**: scoop `700 Hz, Q 0.7, −9 dB`, gain `1 + fuzz*120`, input HP `70 Hz`, `level_scalar 0.5`. The other rows only move scoop centre/depth, input-HP corner, gain scalar and level trim. Rebuild the scoop biquad on `param_changed` like `set_tone` does; keep filter state (no reset) when only the model changes.

- [ ] **Step 4: Wire the parameter** (`DEFAULT_FZ_MODEL: f32 = 0.0`) and add the `MUFF` knob after `TYPE` in `ui/config.rs`; add `FuzzSection.model` with `#[serde(default)]`, `from_params`, `apply`, `rig_values`, `hostile_params`.

- [ ] **Step 5: Add the preset comment truth** — add `model = 0.0  # Ram's Head` to `presets/pink_floyd_comfortably_numb_solo_1.toml` and `_2.toml` (value unchanged, only the comment becomes honest). Do not change any other value.

- [ ] **Step 6: Bump the tripwire** — `KNOBS.len()` 85 → 86.

- [ ] **Step 7: Run the full gate** — `cargo test --lib && cargo clippy …`, then the fidelity `--check`. Expected: PASS, baseline untouched.

- [ ] **Step 8: Commit**

```bash
git add src/dsp/effects/fuzz.rs src/dsp/mod.rs src/ui/config.rs src/preset.rs presets/pink_floyd_comfortably_numb_solo_*.toml
git commit -m "feat(dsp): add Big Muff model voicings"
```

---

### Task 3: ProCo RAT (new stage)

**Files:**
- Create: `src/dsp/effects/rat.rs`
- Modify: `src/dsp/effects/mod.rs` (module + re-export)
- Modify: `src/dsp/mod.rs` (18 stage/param sites)
- Modify: `src/ui/config.rs` (ranges, 3 `KNOBS` rows, `PEDALS` row, tripwires)
- Modify: `src/ui/styles.rs` (colour), `src/ui/draw.rs` (`pedal_stage_state` arm)
- Modify: `src/preset.rs` (`RatSection` + six sites, `DEVICE_YEARS`)

**Interfaces:**
- Consumes: `read_cubic`, `SmoothedGain`, `param_changed`, `Oversampler4`.
- Produces: `Rat::process(&mut self, x: f32, dist: f32, filter: f32, level: f32) -> f32`; `ChainStage::Rat = 21`; `Params.rat_enabled/rat_dist/rat_filter/rat_level`; preset `[rat]`.

- [ ] **Step 1: Write the failing DSP test** in `rat.rs`: a bounded/finite check and a reverse-Filter check (higher `filter` value must mean *darker*, because the real knob is wired backwards).

- [ ] **Step 2: Implement `Rat`** — input HP 60 Hz → gain `1 + dist*60` → 4× OS `tanh`/hard-blend clip → fixed 6.5 kHz one-pole LP → reverse tone (`corner = 6500 * (0.08_f32).powf(filter)`) → level. 3 knobs: `DIST`, `FILTER`, `LEVEL`.

- [ ] **Step 3: Wire the stage.** Append `Rat = 21` to `ChainStage`; `CHAIN_LEN = 22`; `from_u8`, `name` = `"rat"`, `from_name`; `default_order` inserts it **after `Metal`**; `pedal_index`/`from_pedal_index` insert at pedal index 8 (all later indices +1); add it to `is_mono_pedal`; `Params` fields; `DspChain` field + construction; `run_mono_stage` arm; `stage_enabled`.

- [ ] **Step 4: UI** — insert the `RAT` pedal after `ML-2 METAL CORE` in `PEDALS`, move the range constants, add the three `KNOBS` rows, `PEDAL_RAT` colour in `styles.rs` (a dark charcoal/brown), and the ribbon label `"RAT"` in `draw.rs`.

- [ ] **Step 5: Preset schema** — `RatSection { enabled, dist, filter, level }`; `from_params`, `apply` (with the `else { enabled = false }` arm), `rig_values`, `hostile_params`, `DEVICE_YEARS["rat"] = 1979`, and add `"rat"` to the chain round-trip test.

- [ ] **Step 6: Bump tripwires** and fix slot-indexed tests — `PEDALS.len()` 19 → 20, `KNOBS.len()` 86 → 89, and every test that pins a slot after `Metal` (grep `order[`, `insert(`, and `Amp` slot numbers in `dsp/mod.rs`, `ui/input.rs`, `preset.rs`, `ui/draw.rs`).

- [ ] **Step 7: Re-bless the add-pedal modal snapshot**

Run: `INSTA_UPDATE=always cargo test --lib 'ui::'` then review the diff (only `add_pedal_modal` should change).

- [ ] **Step 8: Full gate** — `cargo test --all-features`, clippy, fidelity `--check` (baseline untouched).

- [ ] **Step 9: Commit**

```bash
git add src/dsp/effects/rat.rs src/dsp/effects/mod.rs src/dsp/mod.rs src/ui src/preset.rs src/ui/snapshots
git commit -m "feat(dsp): add ProCo RAT"
```

---

### Task 4: BK Butler Tube Driver (new stage)

**Files:**
- Create: `src/dsp/effects/tube_driver.rs`
- Modify: the same plumbing sites as Task 3.
- Modify: `src/preset.rs` (`TubeDriverSection`, `DEVICE_YEARS["tube_driver"] = 1993`).

**Interfaces:**
- Consumes: Task 3's proven stage plumbing.
- Produces: `TubeDriver::process(&mut self, x, drive, high, low, level) -> f32`; `ChainStage::TubeDriver = 22`; preset `[tube_driver]`.

- [ ] **Step 1: Write the failing tests** — bounded/finite, and a bias/asymmetry check: the positive and negative peak magnitudes must differ (the starved-plate asymmetry) while the DC offset stays near zero after the post block.

- [ ] **Step 2: Implement** — input HP 40 Hz → gain `1 + drive*36` → 4× OS asymmetric clip around a small DC bias → post DC block → low shelf 120 Hz + high shelf 2.5 kHz → smoothed level. 4 knobs: `DRIVE`, `HIGH`, `LOW`, `LEVEL`.

- [ ] **Step 3: Wire the stage** exactly as Task 3, appended as id 22 and default-ordered **after `Rat`**; pedal table after `RAT`; `is_mono_pedal` yes.

- [ ] **Step 4: Preset schema** + `DEVICE_YEARS`, then tripwires (`PEDALS.len()` 20 → 21, `KNOBS.len()` 89 → 93), slot tests, snapshot re-bless.

- [ ] **Step 5: Full gate** and fidelity `--check`.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(dsp): add BK Butler Tube Driver"
```

---

### Task 5: RA-200 rotary (new stereo stage)

**Files:**
- Create: `src/dsp/effects/rotary.rs`
- Modify: `src/dsp/effects/mod.rs`, `src/dsp/mod.rs` (stereo path, `snapshot_order`), `src/ui/config.rs`, `src/ui/styles.rs`, `src/ui/draw.rs`, `src/preset.rs`
- Modify: `presets/pink_floyd_comfortably_numb_solo_1.toml` and `_2.toml` (add `"rotary"` after `"cab"` in `[chain]`)

**Interfaces:**
- Consumes: `read_cubic`, `SmoothedGain`, `Biquad`.
- Produces: `Rotary::process(&mut self, l: f32, r: f32, speed: f32, ramp: f32, depth: f32, mix: f32, balance: f32) -> (f32, f32)` and `Rotary::clear_buffers()`; `ChainStage::Rotary = 23`; preset `[rotary]`.

- [ ] **Step 1: Write the failing tests** — `mix = 0` is bit-exact passthrough on both channels; bounded/finite at fast + full mix; L/R envelopes are out of phase; **the speed ramp bends pitch** (a short-window phase-rate estimate rises during a slow→fast ramp); `clear_buffers` silences the wet path.

- [ ] **Step 2: Implement** — mono-sum the input → crossover at 800 Hz → two rotors with independent angles (RA-200 family: spinning speakers, chorus-like; no horn/drum split in v1). Per rotor: `delay = BASE + DEPTH*cos(angle)` read with `read_cubic`, and `am = 0.5 + 0.5*DEPTH*cos(angle)`; the pitch bend falls out of the delay slope. Stereo output from two virtual mics at opposite angular offsets; `SmoothedGain` on mix. Two `Vec<f32>` rings in `new()`; phasor rotation (no per-sample `sin`/`cos`); keep angles across bypass; `clear_buffers` zeroes rings only.

- [ ] **Step 3: Wire as a stereo rack stage** — id 23, `CHAIN_LEN = 24`, `default_order` **immediately after `Cab`**, `pedal_index` after `BOOST`/before `GEQ` (rack order), **do not** add to `is_mono_pedal`, `run_stereo_stage` arm, `snapshot_order` calls `clear_buffers()`.

- [ ] **Step 4: UI** — `ROTARY` pedal with 5 knobs (`SPEED`, `RAMP`, `DEPTH`, `MIX`, `BALANCE`) after `POWER BOOST`, a colour in `styles.rs`, ribbon label `"ROTARY"`.

- [ ] **Step 5: Fix the explicit-chain presets** — add `"rotary"` after `"cab"` in both Numb TOMLs, so a user who enables it does not get a post-reverb rotary.

- [ ] **Step 6: Preset schema** + tripwires (`PEDALS.len()` 21 → 22, `KNOBS.len()` 93 → 98), slot tests (rack indices +1 from `GEQ` on), snapshot re-bless.

- [ ] **Step 7: Optional MIDI** — add `MidiTarget::RotarySpeed` (+ `ALL.len()` 15 → 16, re-bless `midi_modal.snap`). Skip if it makes the task noisy; note it in the PR.

- [ ] **Step 8: Full gate** and fidelity `--check`.

- [ ] **Step 9: Commit**

```bash
git commit -m "feat(dsp): add RA-200 rotary stage"
```

---

### Task 6: Content — chain-order experiments and new presets

**Files:**
- Create: `presets/pink_floyd_shine_on_pulse.toml`, `presets/pink_floyd_sorrow_1994.toml`, `presets/pink_floyd_run_like_hell_1994.toml`, `presets/pink_floyd_dogs_animals.toml`, `presets/pink_floyd_echoes_seagull.toml`, `presets/pink_floyd_marooned.toml`
- Create `_alt` variants for the experiments below (never overwrite the eight originals)
- Modify: `docs/fidelity-references.md` §1 inventory, `AGENTS.md` chain diagram, `README.md`, `docs/fidelity-implement.md` increment log

**Interfaces:**
- Consumes: every knob/section from Tasks 1–5.
- Produces: shippable presets; no new code.

- [ ] **Step 1: Chain-order A/Bs as `_alt` files.** Each changes one thing, per the spec's §6 table: `numb_alt_muff_td` (`fuzz → tube_driver`), `numb_alt_comp_first` (`comp → fuzz`), `numb_alt_rotary` (rotary instead of chorus), `time_solo_alt_boost_first` (`power_boost → fuzz` — the 1973 order), `time_alt_vibe_after_delay`, `mother_alt_comp_first`.
- [ ] **Step 2: New presets** with era-correct gear and an honest description: Shine On (Pulse), Sorrow (1994 live), Run Like Hell (1994), Dogs (Animals), Echoes (seagull), Marooned.
- [ ] **Step 3: Anachronism check** — `cargo test --lib preset::tests::no_new_device_anachronisms`. If a 1979 preset is intentionally re-voiced, add a documented `KNOWN_ANACHRONISMS` entry instead of weakening the test.
- [ ] **Step 4: Baseline** — only if a *bundled* preset's audio changed, re-bless and record the numbers in `docs/fidelity-implement.md`. The `_alt` files are new, so they do not move the baseline.
- [ ] **Step 5: Docs** — update the `AGENTS.md` chain diagram, README pedal list, and the `docs/fidelity-references.md` inventory.
- [ ] **Step 6: Full gate** — `cargo test --all-features`, clippy, fidelity `--check`.
- [ ] **Step 7: Commit**

```bash
git commit -m "feat(presets): add Gilmour tier-1 presets and chain-order variants"
```

## Self-review notes

- **Coverage:** every Tier 1 item in the spec has a task; the four open decisions are fixed in the spec's §8.
- **Bit-exactness:** Tasks 1–2 both pin "default value reproduces today's branch" and use the existing tests as the gate; Tasks 3–5 default off.
- **The `[chain]` trap** is explicitly handled in Task 5 Step 5.
- **Anachronism gate** is explicitly handled in Task 6 Step 3.
- **Deferred by design:** Leslie voice, optical compressor, Phase 100, Cornish SS-2/G-2, HM-2, Power Boost drive, Alembic, sound-on-sound, and the Tier 3 amps/cabs. They are not in this plan.
