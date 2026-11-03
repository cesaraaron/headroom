# Gilmour gear gap — feasibility review and implementation order

Companion to [`2026-11-02-gilmour-gear-gap-analysis.md`](2026-11-02-gilmour-gear-gap-analysis.md).
**Review only: no implementation code was written for this document.**

Scope: the five Tier 1 items — rotary speaker, Big Muff model knob, BK Butler
Tube Driver, ProCo RAT, compressor voice knob. Everything below is checked
against the code as it stands on 2026-11-02 (`CHAIN_LEN = 21`, 19 pedals,
84 knobs).

---

## 0. Verdict at a glance

| # | Item | Kind | New chain stage | New knobs | Effort | Baseline re-bless | Verdict |
| - | ---- | ---- | --------------- | --------- | ------ | ----------------- | ------- |
| 1 | Rotary (RA-200 / Leslie) | new stereo rack effect | yes (`CHAIN_LEN` 21 → 22) | 5 | **L** | no (off by default) | Feasible; the only real DSP risk in the set |
| 2 | Big Muff model knob | parameter on existing Fuzz | no | +1 (4→5? see §2.2) | **S** | no, *if* model 0 == today's Muff bit-for-bit | Feasible; the UI story is the hard part, not the DSP |
| 3 | BK Butler Tube Driver | new mono pre pedal | yes | 4 | **M** | no (off by default) | Feasible; same shape as the TS-808 |
| 4 | ProCo RAT | new mono pre pedal | yes | 3 | **M** (plumbing) / S (DSP) | no (off by default) | Feasible; cheapest DSP, same wiring cost |
| 5 | Compressor voice knob | parameter on existing Compressor | no | +1 | **S** (Dyna + CS-2), **S/M** with optical | no, *if* default voice is untouched | Feasible |

"Effort" is relative to this codebase: **S** = one small commit, no topology
change; **M** = one focused PR, one new stage; **L** = multi-day, new DSP plus a
new stage.

The important structural fact: **items 2 and 5 are parameter changes; items 1, 3
and 4 are stage changes.** A stage change is 3–4× the touch points of a parameter
change (§3), and it is the thing that can break the chain/preset/UI contract.

---

## 1. The machinery every item has to satisfy

### 1.1 The chain is a fixed, reorderable list of `ChainStage` ids

| What | Where |
| ---- | ----- |
| Enum + explicit ids | `src/dsp/mod.rs:294-318` |
| `CHAIN_LEN = 21` | `src/dsp/mod.rs:321` |
| `from_u8` / `name` / `from_name` | `src/dsp/mod.rs:328`, `:358`, `:384` |
| Shipped order | `ChainStage::default_order`, `src/dsp/mod.rs:412` |
| Stage ↔ pedal-table index | `pedal_index` `:440`, `from_pedal_index` `:465` |
| Mono vs stereo classification | `is_mono_pedal` `:492` |
| "stereo must follow the amp" guard | `:538` |
| Order sanitizer (presets, UI moves) | `:554` |
| Seqlock publish/read | `:1379`, `:1401`, `:1428` |
| Per-stage bypass crossfade + `stage_enabled` | `:1348`, `:2184-2278` |
| Dispatch | `run_mono_stage` `:1997`, `run_stereo_stage` `:2076` |
| Tail clear on reorder | `snapshot_order`, `:2303-2326` |

Consequences that decide the implementation order:

- Stage ids are **runtime-only** (`Arc<[AtomicU8; CHAIN_LEN]>`, `:986`). Only
  `ChainStage::name()` strings are persisted, in `[chain] order`
  (`src/preset.rs:926-951`). So **append new ids at the end of the enum** and
  there is no persistence migration at all — but do *not* renumber, because
  several tests and range helpers assume ids are `0..CHAIN_LEN` with no holes
  (e.g. `src/dsp/mod.rs:4019`, `:4178`, `:4188`; `src/ui/input.rs:1484`).
- `is_mono_pedal` (`:492`) is not a DSP detail: `stereo_stages_follow_the_amp`
  (`:538`) uses it to refuse a UI move that puts a stereo stage before the amp.
  A new stereo stage (rotary) must **not** be added there; two new mono pedals
  (RAT, Tube Driver) must be.
- Every enabled stage is re-read per sample (`stage_enabled` at `:2219`), so
  bypass is instant. A new effect does *not* need its own enable check inside
  `process` — see the `mono_stage!` comment at `:1519-1525`.
- A new effect that owns recirculating state must grow `snapshot_order`
  (`:2311-2322`), otherwise a `[`/`]` reorder rings the old tail into the new
  topology.

### 1.2 What a "new pedal" costs today

The `add-pedal` skill is accurate. Expanded against the current code, a new
pedal means:

**`src/dsp/effects/<name>.rs`** (new) — the effect + `#[cfg(test)]` module.

**`src/dsp/effects/mod.rs`** — `pub mod` (`:17-36`) and `pub use`
(`:38-57`). Reuse `read_cubic` (`:100`), `SmoothedGain` (`:146`), `OnePoleLp`
(`:256`), `ThreeBandEq` (`:292`), `db_to_lin`/`lin_to_db` (`:240-248`),
`param_changed` (`:66`), and `Oversampler4/8` (`src/dsp/oversample.rs:200`,
`:209`).

**`src/dsp/mod.rs`** — 18 edit sites: import (`:27-30`); `ChainStage` variant
(`:296`); `CHAIN_LEN` (`:321`); `from_u8` (`:328`); `name` (`:358`);
`from_name` (`:384`); `default_order` (`:412`); `pedal_index` (`:440`);
`from_pedal_index` (`:465`); `is_mono_pedal` (`:492`) if mono; `Params` knobs
(`:786-991`); defaults (`:600-780`); `Params::new` (`:1017`); `reset_to_defaults`
(`:1173`); `stage_enabled` (`:1348`); `DspChain` field (`:1676`); `DspChain::new`
(`:1768`); `run_mono_stage` or `run_stereo_stage` (`:1997` / `:2076`). Add
`snapshot_order` (`:2314-2320`) if it has tails.

**`src/ui/config.rs`** — one `*_START`/`*_END` pair (`:49-94`), one row per knob
in `KNOBS` (`:96-459`), one `PEDALS` entry (`:463-618`), and the count tripwire
(`:769-774`).

**`src/ui/styles.rs`** — `PEDAL_<NAME>` (`:170-188`) and a `styles::*` re-export
is already glob-imported by `draw.rs:22`.

**`src/ui/draw.rs`** — `pedal_stage_state` arm (`:381-404`). The tile grid
(`:875-881`), detail editor (`:1101`), `+ ADD` modal (`:1811`) and
`toggle_pedal`/`add_pedal`/`remove_pedal` (`src/ui/input.rs:628`, `:637`, `:643`)
all derive from `PEDALS` and need no edit.

**`src/preset.rs`** — a `*Section` struct (`:66-361`), a field on `Preset`
(`:22-64`), `from_params` (`:429`), `apply` (`:639`, with an `else { …enabled =
false }` arm), `rig_values` (`:1108-1201`), and `hostile_params` (`:1348`).

**Tests** — effect unit tests; `table_sizes_are_stable`
(`src/ui/config.rs:769`); the chain/index tripwire (`:777-794`); likely
`add_pedal_modal.snap`; any test that pins a slot number.

### 1.3 What a "voice knob" costs

A knob on an existing pedal is **not** a one-file change either, but it is
small and, crucially, has no topology surface:

`src/dsp/effects/<fx>.rs` (signature + branch + constants + tests);
`src/dsp/mod.rs` — 5 sites: `Params` field (`:786-991`), default const
(`:600-780`), `Params::new` (`:1017`), `reset_to_defaults` (`:1173`), the
`mono_stage!` argument list (`:2024-2033` for the comp, `:2034-2038` for the
fuzz); `src/ui/config.rs` — one `KNOBS` row and the count tripwire;
`src/preset.rs` — section field + `#[serde(default)]` fn + `from_params` +
`apply` + `rig_values` + `hostile_params`.

---

## 2. Tier 1, item by item

### 2.1 Rotary speaker — new stage, effort **L**

**Files/functions**

| File | Change |
| ---- | ------ |
| `src/dsp/effects/rotary.rs` (new) | `Rotary { … }` + `process(l, r, speed_fast, ramp, depth, mix, balance) -> (f32, f32)` + `clear_buffers()` + tests |
| `src/dsp/effects/mod.rs:17-57` | module + `pub use` |
| `src/dsp/mod.rs:27-30, 296-318` | import; `ChainStage::Rotary = 21` |
| `src/dsp/mod.rs:321` | `CHAIN_LEN = 22` |
| `src/dsp/mod.rs:328-409` | `from_u8`, `name` → `"rotary"`, `from_name` |
| `src/dsp/mod.rs:412` | `default_order`: insert after `Cab` |
| `src/dsp/mod.rs:440-488` | `pedal_index` / `from_pedal_index` (all rack indices shift by 1) |
| `src/dsp/mod.rs:492` | **do not** add to `is_mono_pedal` (it is stereo) |
| `src/dsp/mod.rs:786-991` | `rot_*` knobs + `rot_enabled` |
| `src/dsp/mod.rs:1017, 1173, 1348` | defaults, reset, `stage_enabled` |
| `src/dsp/mod.rs:1676-1810` | `DspChain` field + construction |
| `src/dsp/mod.rs:2076-2166` | `run_stereo_stage` arm |
| `src/dsp/mod.rs:2311-2322` | `clear_buffers` on reorder |
| `src/ui/config.rs` | ranges, 5 `KNOBS` rows, `PEDALS` entry, tripwire |
| `src/ui/styles.rs` | colour |
| `src/ui/draw.rs:381-404` | `pedal_stage_state` arm |
| `src/preset.rs` | `RotarySection` + 5 sites |
| `src/midi.rs:54-90` | optional: `RotarySpeed` target (+ `ALL.len()` test at `:682`) |

**Why L and not M.** The wiring is the same M-sized body of work as the RAT,
plus the only genuinely new DSP in the set: two rotors with coupled
phase→doppler→pitch, a crossover, and a speed ramp. Expected 450–700 lines
including tests. Design in §5.

### 2.2 Big Muff model knob — parameter, effort **S**

**Files/functions**

| File | Change |
| ---- | ------ |
| `src/dsp/effects/fuzz.rs:29-47` | per-model constants/state: scoop centre/width/depth, input-HP corner, gain scalar, level trim |
| `src/dsp/effects/fuzz.rs:60` | the single fixed `Biquad::peak_eq(sr, 700.0, 0.7, -9.0)` becomes a rebuilt-on-change scoop |
| `src/dsp/effects/fuzz.rs:95-183` | `process(..., kind, guitar, model)`; Muff branch selects by `model` |
| `src/dsp/effects/fuzz.rs:236-390` | new tests next to the existing Goertzel helpers (`:346`) |
| `src/dsp/mod.rs` | 5 sites (field, default, `new`, `reset`, macro args `:2034-2038`) |
| `src/ui/config.rs` | 1 `KNOBS` row; `PEDALS.len()` unchanged; **`KNOBS.len()` 84 → 85** (`:773`) |
| `src/preset.rs` | `FuzzSection.model` with `#[serde(default = "fuzz_model_default")]`; `from_params`; `apply`; `rig_values` (`:1170`); `hostile_params` |

**The real problems, neither of them DSP**

1. **Two selectors on one pedal.** `fz_type` already picks Muff / Fuzz Face /
   Tone Bender with thresholds at 0.25 / 0.75 (`fuzz.rs:122-128`), and `KNOBS`
   already has 5 fuzz entries (`config.rs:203-223`). A sixth selector is inert
   unless `type < 0.25`, and the UI has no "greyed-out knob" idiom — it will
   just look like it does nothing. Options, cheapest first:
   - **(b) fold the models into `TYPE`**: 5 positions at 0.2 steps
     (Ram's Head / Triangle / Sovtek / Fuzz Face / Tone Bender). Zero knob
     growth, zero `KNOBS` tripwire, but conflates "which pedal" with "which
     Muff revision" and loses the Cornish.
   - **(a) a separate `MUFF` knob** (recommended): four positions at 0.25
     steps, documented as "MODEL only affects the Big Muff voice". Costs one
     knob and one tile value (the tile already clips: `draw.rs:1052-1055` prints
     every value on one line in a 16-col tile, so 5 values already overflow).
2. **Model 0 must be bit-identical to today.** `presets/pink_floyd_comfortably_numb_solo_1.toml:36`
   literally says `type = 0.0   # 0 = Big Muff (the Ram's-Head voice)`. So
   Ram's Head = the current `-9 dB @ 700 Hz`, gain `1 + fuzz*120`, HP 70 Hz.
   Implement the model as a `const` table whose row 0 is *the current literals*;
   do not "improve" it. Otherwise every bundled preset and
   `docs/fidelity/baseline-synth-48k.toml` moves.

Honest framing for the docs: this gives four **voicings**, not four circuits.
The units differ mostly in component tolerances and mid-scoop topology; a fixed
topology can only move scoop, HP, gain and output trim. Say "voiced like",
in the same register as `docs/fidelity-references.md`.

### 2.3 BK Butler Tube Driver — new stage, effort **M**

Same table as §1.2, plus:

- `src/dsp/effects/tube_driver.rs`: DC block → input HP → **4× OS** asymmetric
  clip with a DC bias term (starved-plate asymmetry), then a 2-band shelving EQ
  (`ThreeBandEq` is a 3-band; use two `Biquad`s or add a `TwoBandEq`) and level.
  Model it on `tube_screamer.rs` and `distortion.rs`.
- Order: after the existing drive pedals (see §6, phase 4). The "Muff → Tube
  Driver" topology is a **preset** decision via `[chain]`, not a default.
- `src/dsp/mod.rs:492` `is_mono_pedal` **must** include it.
- Preset section shape mirrors `DsSection` (`src/preset.rs:162-168`).
- `DEVICE_YEARS` (`src/preset.rs:1255`) needs an entry if bundled presets ever
  enable it — see the anachronism risk in §4.10.

Effort M: ~180–260 lines of DSP (it is a TS-shaped circuit) plus the ~200 lines
of unavoidable plumbing. When the plumbing is already proven by the RAT, the
incremental cost is mostly DSP.

### 2.4 ProCo RAT — new stage, effort **M** (plumbing), **S** (DSP)

- `src/dsp/effects/rat.rs`: input HP → gain → silicon hard-ish clip
  (`tanh`/`clamp` blend is enough; this is an op-amp clipper, not a tube) → the
  fixed ~6.5 kHz one-pole LP → **reverse** tone control (the FEEDBACK/Filter
  knob is wired backwards: `1.0 - filter` maps to the LP corner) → level. 4× OS.
- 3 knobs: `DIST`, `FILTER`, `LEVEL`.
- Same 18 `src/dsp/mod.rs` sites; `is_mono_pedal` yes.

This is the ideal *first* new stage: its DSP is ~110–160 lines and it exercises
every plumbing surface once, cheaply.

### 2.5 Compressor voice knob — parameter, effort **S** (S/M with optical)

- `src/dsp/effects/compressor.rs:62-86`: the hard-knee gain computer
  (`:109-113`) and fixed 150 ms release (`:82`) become voice-dependent.
  Dyna Comp = today's hard knee + fast attack + mid-forward makeup; Boss CS-2 =
  a soft knee (a knee-width term in the dB-domain computer — **the one place
  that needs real care**, see below); optical/Compulator = **program-dependent
  release**, which is not a constant swap: it needs a second detector
  (release slows as gain reduction persists), ~30–50 lines.
- Adding colour (a fixed mid bump / slight HF rolloff) must be *post*-gain so
  the makeup law at `:74-75` stays untouched.
- 5 `src/dsp/mod.rs` sites; 1 `KNOBS` row (84 → 85); preset `CmpSection.voice`
  with a default-0 serde fn (`src/preset.rs:74-79`).
- **Bit-exactness:** voice 0 must leave `thresh_db`, `ratio`, `knee_k`,
  `auto_makeup`, `atk`, `rel` exactly as they are. Implement the soft knee so
  that the knee width is 0 at voice 0 *and the expression reduces to the current
  branch* — or, safer, keep the current `if env > thresh_lin` branch for voice 0
  and only take the soft-knee path for voices 1/2. The existing tests
  (`makeup_is_unity_at_the_transparent_end` `:157`,
  `zero_sustain_is_level_transparent` `:207`, `single_powf_matches_db_form`
  `:299`) will catch a drift here, which is exactly what they are for.

---

## 3. Is a voice knob cheaper than a new pedal? Quantified

Yes — roughly **3–4× fewer touch points**, and the difference is structural, not
stylistic.

| Surface | Voice knob | New pedal |
| ------- | ---------- | --------- |
| Edit sites in `src/dsp/mod.rs` | **5** | **18** |
| New DSP file | 0 (edit existing) | 1 |
| `src/dsp/effects/mod.rs` | 0 | 2 |
| `src/ui/config.rs` | 1 row + 1 tripwire | ranges + N rows + `PEDALS` row + 2 tripwires |
| `src/ui/styles.rs` | 0 | 1 |
| `src/ui/draw.rs` | 0 | 1 |
| `src/preset.rs` | 4 sites | 6 sites |
| `src/midi.rs` | 0 | 0–2 (optional) |
| Chain topology (`CHAIN_LEN`, `default_order`, `[chain]` names, tile grid) | **none** | **all of it** |
| Snapshot churn | 0–1 | 0–3 |
| Approx. total diff | 60–120 lines | 250–450 lines + tables |

But the marginal cost falls: the **second and third** new pedal cost mostly DSP
plus a mechanical repeat of the 18 sites. The first new pedal (the RAT) pays the
"prove the new-stage machinery still works" price for the whole batch — that is
why it should ship first among the stage items.

Recommendation, matching the research doc's default: **RAT = new pedal, Tube
Driver = new pedal, Muff = voice knob, compressor = voice knob, rotary = new
pedal.** The only place I would deviate is the Muff knob's *shape* (§2.2): a
5-position `TYPE` avoids growing the piece of UI that is already clipping.

---

## 4. Constraints and risks

### 4.1 Audio-thread allocation and blocking

- `DspChain` is built once on the control thread (`src/audio/mod.rs:1355-1356`)
  and the callback only calls `process_block`. So `Vec` allocation in a new
  effect's `new()` is fine — that is exactly how `Chorus` (`chorus.rs:40-51`)
  and `Delay` (`delay.rs:72-98`) acquire their buffers.
- **No allocation may leak into `process`.** For the rotary this means: two
  `Vec<f32>` allocated in `new()`, fixed indices, `super::read_cubic`
  (`effects/mod.rs:100`) for fractional taps, a phasor for `sin`/`cos`, and no
  `Vec` in the mix path. A `SmoothedGain` for mix is already the house pattern
  (`effects/mod.rs:146`).
- **No per-sample transcendentals.** The house trick is `HumNoise`
  (`src/dsp/mod.rs:1623-1673`): keep `(c, s)` on the unit circle and rotate by a
  precomputed `(cos_w, sin_w)`. The rotary should use the same technique per
  rotor. Recomputing `cos_w/sin_w` while the speed ramps is acceptable
  (bounded to the ramp, ~1 s); an even cheaper option is to rotate the
  *increment* vector by a small fixed step during the ramp, so there is no
  `sin`/`cos` at all (see §5.5).
- **No tail disposal on the audio thread.** The rotary owns no `Box`/IR/plugin,
  so nothing to hand back — but its `clear_buffers()` must be called from
  `snapshot_order` (`src/dsp/mod.rs:2314-2320`), like the chorus/delay/flanger.
- **Bypass is free when fully off**: `run_ordered_stage` returns at `:2236-2238`
  before calling the effect. Two new oversampled pedals therefore cost nothing
  while off, which is the default for every bundled preset.

### 4.2 `CHAIN_LEN` growth

Going 21 → 24 (three new stages) touches, at minimum:

- `src/dsp/mod.rs:321` and every `[u8; CHAIN_LEN]` (a type alias, so it is
  mostly free): `:986`, `:1581`, `:1718`, `:1744`, `:1988`, `:2303`.
- `bypass_ramp` init (`:1755-1767`) and `bypass_step` — automatic.
- The seqlock writer/reader — automatic, but note `CHAIN_READ_ATTEMPTS` (`:325`)
  is unaffected (it counts retries, not slot count).
- `sanitize_chain_order`'s `Vec::with_capacity` (`:556`) — control thread only.
- Tests that iterate `0..CHAIN_LEN` and assume a contiguous id space
  (`src/dsp/mod.rs:4019`, `:4178`, `:4188`; `src/ui/input.rs:1484`). **These
  pass only if new ids are appended with no holes.**
- Tests that pin slot numbers. Grep for `order[` and expect ~a dozen:
  `src/ui/input.rs:1528-1541` (`Amp` at 10, `Boost` 11, `Geq` 12, `Cab` 13),
  `src/preset.rs:1655` (`insert(12, Comp)`), `src/dsp/mod.rs:3210-3218`,
  `src/ui/draw.rs:3529-3531`. Inserting a stage *before* `Amp` (RAT / Tube
  Driver) shifts everything after it; inserting the rotary *after* `Cab` shifts
  the rack only.
- The two tripwires that tie the three tables together:
  `src/ui/config.rs:772` (`PEDALS.len()`) and `:793`
  (`default_order().len() == PEDALS.len() + 2`). They are the reason a missed
  mapping fails loudly instead of silently mis-indexing the board.

**Verdict: growth is safe and cheap, provided every new stage is appended to the
enum and every derived table is edited in the same commit.**

### 4.3 UI tile count and the 84-knob layout

Current: 19 pedals / 84 knobs. After Tier 1 (3 pedals, 5+4+3 knobs, plus 2 knob
additions): **22 pedals / 98 knobs** (84 + 5 + 4 + 3 + 1 + 1).

- **Tile grid.** `cols = (width / 16).max(1).min(tile_count)`,
  `rows = ceil(tile_count / cols)` (`src/ui/draw.rs:845-847`, `:887-888`),
  tile height 3 (`:835`). 19 pedals + `+ADD` = 20 tiles:
  - 80 cols → 5 cols → 4 rows; 22 → 5 rows (**+3 screen rows**).
  - 100 cols → 6 cols → 4 rows; 22 → 4 rows (no change).
  - 120 cols → 8 cols → 3 rows; 22 → 3 rows (no change).
  So the cost is only on narrow terminals, and it is bounded. **22 tiles is
  acceptable**; a "more pedals" page is not needed until Tier 2 lands too.
- **Knob count per tile.** The tile prints every value on one line
  (`draw.rs:1052-1055`); 5 values already overflow a 16-col tile, so the tile is
  not the place to look. The detail editor splits the width evenly by
  `pedal.end - pedal.start` (`draw.rs:1171-1174`), i.e. 6 knobs at 80 cols is
  ~13 columns each; `render_compact_knob` truncates the label
  (`draw.rs:1310-1325`), so a long label like `FEEDBACK` would clip. **Keep new
  pedals to ≤5 knobs** and check the fuzz's 6-knob editor at 80 columns if the
  extra Muff knob ships.
- **Snapshot churn is smaller than it looks**, because both the ribbon
  (`src/ui/input.rs:59-76`) and the tile grid (`draw.rs:875-881`) are built from
  *on-board* pedals, and new pedals default off. The boot screen therefore does
  not reflow; `add_pedal_modal.snap` changes (it lists all `PEDALS`,
  `draw.rs:1811`), and `pedalboard_focused` changes only if a pedal it shows
  gains a knob.

### 4.4 Preset backward and forward compatibility

- **Missing fields**: every new section/field needs
  `#[serde(default = "…")]` so an old file loads at the old behaviour. The
  existing pattern is `FuzzSection.r#type` (`src/preset.rs:137-148`) and
  `DelaySection.r#type` (`:268-276`).
- **Missing sections**: `apply` must *disable* the stage when the section is
  absent (`else { params.x_enabled.store(false, …) }`, e.g. `:722-724`,
  `:863-865`) — otherwise a new pedal leaks "on" from a prior preset and
  `bundled_presets_load_deterministically` (`:1459`) fails.
- **`[chain]` order names**: old presets never mention `rotary` / `td` / `rat`;
  `sanitize_chain_order` (`src/dsp/mod.rs:554-583`) appends missing stages **at
  the end of the order**, in `default_order` sequence. That is structurally
  safe, but it means a preset with an explicit `[chain]` gets the rotary
  *after the reverb*, not after the cab. Two bundled presets have explicit
  chains (`pink_floyd_comfortably_numb_solo_1.toml` / `_2.toml`). Mitigation is
  one added line per file:
  `order = [ …, "cab", "rotary", "geq", … ]`. Do this in the phase that adds
  the stage, not later, or a user who enables the rah-200 on those presets gets
  a delayed rotary.
  - Do **not** change `sanitize_chain_order` to insert at a "positional" default:
    the code deliberately treats an authored order as intent
    (`src/dsp/mod.rs:534-537`), and changing the sanitizer is a much riskier
    commit than editing two TOMLs.
- **Forward compatibility is already fine**: serde ignores unknown keys (no
  `deny_unknown_fields`), so a newer preset opened by an older binary loses the
  new sections rather than failing to parse. Worth a line in the docs so nobody
  "fixes" it.
- **`rig_values` (`:1108-1201`) is the autosave hash source**
  (`rig_revision`, `:1206`, used at `src/ui/mod.rs:320`). A knob that is not
  listed there is invisible to dirty-detection and to
  `bundled_presets_load_deterministically`; a section that is not scrambled in
  `hostile_params` (`:1348-1452`) makes the determinism gate pass vacuously.
  Both must be updated in the same commit as the knob.
- **`all_bundled_presets_parse` (`:1226`)** catches a schema typo immediately.

### 4.5 MIDI

`MidiTarget` is a closed enum with a "every target parses and there are 15"
test (`src/midi.rs:54-90`, `:682`) plus a bind-screen list. Adding targets is
cheap but *is* a tripwire: add e.g. `RotarySpeed`, `RatDrive`, `TubeDriverDrive`
→ bump `ALL.len()` to 18 and re-bless `midi_modal.snap`. **Not required to ship
any Tier 1 item**; the one thing users will ask for is an expression pedal on
the rotary's SPEED. Recommend doing it in the rotary phase, not before.

### 4.6 Help overlay and docs

- The `K` modal (`src/ui/draw.rs:1590-1810`) is generic keybindings — it does
  **not** enumerate pedals, so it needs no change. `help_modal.snap` should not
  move.
- `AGENTS.md:17-42` (the chain diagram) and the README must be updated in the
  same PR as each stage, and `docs/fidelity-references.md`'s code-derived
  inventory (`§1`, which is generated from `presets/*.toml`) in the preset phase.
- `docs/fidelity-implement.md` gets an increment-log row for each topology
  change, per `AGENTS.md:110-112`.

### 4.7 Snapshots

`src/ui/snapshots/*.snap`. Re-bless with
`INSTA_UPDATE=always cargo test --lib 'ui::'` (see the add-pedal skill) and
commit. Expect: `add_pedal_modal` always; `pedalboard_focused` / `default_screen`
only if the pedal shown gains a knob or a tile; `midi_modal` only if targets are
added. **Never** hand-edit a `.snap`.

### 4.8 Count tripwires

Three tests exist to make table edits conscious:

1. `src/ui/config.rs:769-774` — `PEDALS.len()` and `KNOBS.len()`.
2. `src/ui/config.rs:777-794` — every `PEDALS[i]` has exactly one
   `ChainStage`, and `default_order().len() == PEDALS.len() + 2`.
3. `src/ui/config.rs:693-723` — ranges tile contiguously and
   `PEDALS.last().end == KNOBS.len()` (which is also `ADD_TILE`, `:622`).

Plus `src/midi.rs:682` for targets. Bump them in the same commit as the table
edit; they are the only mechanical defence against a silent navigation bug.

### 4.9 Fidelity baseline

- The gate is CI: `.github/workflows/test.yml:89-93` runs
  `fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml`, with
  tolerances in `CONTRIBUTING.md:313-317` (LUFS ±0.1, crest ±0.2 dB,
  correlation ±0.02, centroid ±1 %, ±0.25 dB/LTAS band). That is tight enough
  that *any* change to the Muff scoop or the compressor knee will fail it —
  which is the desired tripwire.
- **Policy for this work: no re-bless.** All three new effects default to
  disabled and are skipped before `process` (`src/dsp/mod.rs:2236-2238`), and
  both voice knobs default to today's code path. So `--check` should pass with
  an unmodified baseline in every phase. If it does not, the default path
  changed and the change is a bug, not a re-bless.
- The only intended re-bless is in the *content* phase, if a bundled preset is
  actually re-voiced onto a new model (e.g. Comfortably Numb onto the Tube
  Driver). Then: regenerate with
  `cargo run --release --example fidelity_render -- --presets all --write-baseline docs/fidelity/baseline-synth-48k.toml`,
  and record the numbers in `docs/fidelity-implement.md` (`CONTRIBUTING.md:349-352`).
- The in-repo bit-exact gate `bundled_presets_render_identically_after_hostile_state`
  (`src/preset.rs:1486`) compares fresh vs hostile **within one build**, so it
  catches missing `apply` arms but *not* a voicing drift. The harness baseline
  is the only old-vs-new guard.

### 4.10 The anachronism test

`no_new_device_anachronisms` (`src/preset.rs:1280-1337`) uses `DEVICE_YEARS`
(`:1255`) and `KNOWN_ANACHRONISMS` (`:1271`). Adding RAT/Tube Driver entries and
then re-voicing the 1979 Comfortably Numb presets onto them will **fail the
build** (Tube Driver is mid-80s, RAT II 1987). This is a real gate on the
research doc's open question 4, not a docs issue. Either:

- keep the 1979 presets on the TS/Muff and put the Tube Driver only on the
  post-1987 presets (Shine On, Sorrow, Marooned), or
- add `(preset, device)` to `KNOWN_ANACHRONISMS` with a comment, exactly as the
  TS-808 entries there already do.

---

## 5. Designing the rotary

### 5.1 Where in the chain

**Default position: immediately after `Cab`, first in the rack.** Reasons:

- It needs a stereo input; every stage before `Amp` is mono by contract, and
  `stereo_stages_follow_the_amp` (`src/dsp/mod.rs:538`) makes a pre-amp stereo
  stage pointless (the amp folds it — see `Sig::into_mono`, `:1549`).
- The RA-200 was a speaker cabinet, so it belongs after the amp/cab, and every
  later rack effect (EQ, modulation, delay, reverb) should colour the rotary's
  output rather than being modulated by it. Putting it after the reverb would
  chop the reverb tail with the rotor.
- It stays movable with `[`/`]`, so a user can place it elsewhere; that is a
  judgement call the chain already supports.

**Caveat to document:** a cab sim followed by a rotary means two speaker
colorations in series. This models "the rotary in the room, mic'd", not "the
rotary *instead of* the cab". Do not claim the latter; the honest wording is
"post-cab rotary".

### 5.2 Mono or stereo

Input: sum to mono internally (`0.5*(l+r)`) — a Leslie is driven by one amp
channel. Output: **stereo**, produced by the mic pair. The dry/wet mix keeps
the incoming stereo when `mix` is low and replaces it with the rotary image as
`mix` rises. This is physically right and avoids a hard mono fold at `mix = 0`
(the flanger had exactly that bug; see `flanger.rs:164-182`).

### 5.3 The signal model

```
mono ──► HP 800 Hz ──► horn rotor ──► horn mic L/R ─┐
     └─► LP 800 Hz ──► drum rotor ──► drum mic L/R ─┴─► sum ──► dry/wet
```

- **Crossover**: one 2nd-order LP + HP at ~800 Hz (`Biquad::lowpass` /
  `highpass`, `src/dsp/biquad.rs`). Two biquads, mono, cheap.
- **Horn rotor**: rotating source. Two virtual mics at opposite sides. For each
  mic `m` with angular offset `φ_m` (0 and π):
  - `delay_m = BASE + DEPTH * cos(θ_h + φ_m)` — read with `read_cubic`.
  - `am_m = 0.5 + 0.5 * depth_am * cos(θ_h + φ_m)`.
  This is the whole trick: **because the delay is derived from the angle, the
  pitch shift falls out of `d(delay)/dt`.** A real horn accelerating from slow
  to fast bends up in pitch; an independent "doppler LFO" would not. Do not
  implement doppler and tremolo as two unrelated LFOs.
- **Drum rotor**: slower angle, mostly AM (a 15" cone is a large, smeared
  source), a small doppler depth, and a fixed LP on its path. Two mics as well,
  with a wider, less directional pattern.
- **Horn character**: a fixed compression-driver colouration (a gentle peak
  around 1–2 kHz and an HF rolloff) is enough. An earlier idea in the research
  doc — "a corner-frequency sweep" — implies rebuilding a biquad's coefficients
  every sample as the horn rotates. That is expensive, zipper-prone and the
  least audible part of a Leslie. **Ship the fixed horn EQ + moving
  AM/doppler**; defer the moving resonance. (§7.1.)
- **Rotors are not phase-locked**: horn and drum get independent angles and
  slightly different speed ratios (e.g. horn fast 6.8 Hz / drum fast 5.7 Hz),
  which is what makes a Leslie sound like one cabinet rather than a chorus.

### 5.4 Fast/slow and the ramp

- Two target speeds per rotor (slow ≈ 0.8 Hz horn / 0.7 Hz drum; fast ≈ 6.8 /
  5.7). A `SPEED` knob selects the target with a threshold at 0.5 — the same
  idiom as `TYPE`/`MODE` (e.g. `delay.rs:140-141`). Expose `RAMP` as the
  acceleration time (≈0.2–3 s, default ~1 s, asymmetric: decel is slower than
  accel on a real Leslie).
- The ramp is a one-pole on the *speed*, not on the angle:
  `speed += (target - speed) * accel_coeff`. The angle integrates the speed, so
  the pitch bend is automatic (see above).
- Keep the angle across bypass and across `clear_buffers` — the rotor should
  not jump when the pedal is switched in (the chorus keeps its LFO phase for
  the same reason, `chorus.rs:55-59`).

### 5.5 Staying allocation-free

- Two `Vec<f32>` ring buffers (`horn`, `drum`), sized for
  `BASE_MS + DEPTH_MS` plus the cubic guard (≈8 ms at 48 kHz ≈ 400 samples),
  allocated in `new()`. That is ~3 KB, one-time.
- Fractional taps via `super::read_cubic` (`effects/mod.rs:100`), which clamps
  to `2 .. len-3` and never touches the unwritten slot.
- `sin`/`cos` via a per-rotor phasor and the `HumNoise` recurrence
  (`src/dsp/mod.rs:1657-1673`), with the same first-order renormalisation.
  During a ramp the increment changes; the cheapest correct option is to rotate
  the *increment* vector by the small delta each sample
  (`c' = c - δ·s`, `s' = s + δ·c`, then renormalise), which needs no
  transcendentals at all. Recomputing `cos_w`/`sin_w` on a `param_changed`
  boundary is also acceptable — that is one `sin`+`cos` per rotor per sample for
  at most the ramp duration.
- `SmoothedGain` for `mix` (`effects/mod.rs:146`) so a MIDI CC sweep does not
  staircase.
- `clear_buffers()` zeroes the two rings and the write cursor, keeps the rotor
  angles and the mix smoother (mirrors `chorus.rs:55-59`, `phaser.rs:80-85`),
  and is wired into `snapshot_order` (`src/dsp/mod.rs:2314-2320`).
- No `Vec` in `process`, no `Box`, no locks. Everything is `f32` scalar state.

### 5.6 Proposed knobs (5)

`SPEED` (slow↔fast, threshold 0.5) · `RAMP` (accel time) · `DEPTH` (doppler/AM
amount) · `MIX` · `BALANCE` (horn↔drum). Five fits the tile/editor budget
(§4.3). If a sixth is wanted, `SLOW`/`FAST` rate trims are the obvious pair —
but that means checking the 80-column editor.

### 5.7 Tests

- `fully_dry_is_passthrough` at `mix = 0` for both channels (the flanger
  regression test is the template, `flanger.rs:164-182`).
- finite/bounded over a second at max depth + fast + full mix.
- **rotor actually rotates**: with a steady sine in, the wet output's envelope
  must modulate, and the *left* and *right* envelopes must be out of phase
  (the chorus has the first half of this, `chorus.rs:131-149`; the L/R phase
  relationship is the rotary-specific assertion).
- **speed ramp bends pitch** on the horn path (estimate a short-window
  zero-crossing/phase rate during the ramp) — this is the test that fails if
  someone implements doppler as an independent LFO.
- **no DC and no runaway**: the drum LP and the horn HP sum to roughly flat
  response.
- `clear_buffers` silences the wet path within one buffer length.

### 5.8 Reuse verdict

Yes, but less than the research doc implies: there is **no shared ring-buffer
type**. The reusable pieces are the free function `read_cubic`
(`effects/mod.rs:100`) and the *pattern* (own `Vec` + `write` cursor +
`clear_buffers`), as in `chorus.rs:20-29` / `:68-92` and `delay.rs:19-43`. A
rotary is a new struct on that pattern, not a wrapper around `Chorus`.

---

## 6. Implementation sequence

Design goal: **the tree is green after every phase**, and each phase is
independently shippable.

### Phase 0 — freeze the decisions (no code)

Answer the four open questions in the research doc, with these recommended
defaults: RAT new pedal, Tube Driver new pedal, Muff = `MUFF` knob (or the
5-position `TYPE` alternative), compressor = `TYPE` knob, rotary = new stage
after `Cab`. Also freeze the knob counts, because they set the tripwire numbers.

Exit: this document plus the answers; no source change.

### Phase 1 — Compressor voice (S)

Smallest possible test of the "voice knob" pattern, and the one where a DSP
mistake would be most audible.

- Files: `compressor.rs`, 5 sites in `dsp/mod.rs`, 1 `KNOBS` row, 4 sites in
  `preset.rs`.
- Tests: new per-voice distinctness + boundedness; the five existing tests must
  pass unchanged (they are the bit-exactness gate for voice 0).
- Tripwires: `KNOBS.len()` 84 → 85.
- Baseline: `--check` must pass **untouched**. A failure means the default path
  moved — fix the code, do not re-bless.
- Exit: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `fidelity_render -- --check`.

### Phase 2 — Big Muff model (S)

Same shape. Guard rails:

- Model 0 = the current literals (`-9 dB @ 700 Hz`, `gain = 1 + fuzz*120`,
  HP 70 Hz, `level_scalar 0.5`).
- New test: the four models must be measurably distinct (reuse the Goertzel
  helper at `fuzz.rs:346` and assert e.g. the 700 Hz band and the 2–5 kHz band
  differ), mirroring `fuzz_voicings_are_distinct_and_bounded` (`:304`).
- Preset: `[fuzz] model`, default 0; the two Comfortably Numb presets can add
  `model = 0.0  # Ram's Head` to make their existing comment true.
- Tripwire: 85 → 86.
- Baseline untouched.

### Phase 3 — ProCo RAT (M) — the first new stage

This is where the topology surface is exercised. Do it before the Tube Driver
because its DSP is the simplest.

- New file + the 18 `dsp/mod.rs` sites + `config.rs` + `styles.rs` +
  `draw.rs` + `preset.rs`.
- **Append** the stage id (21) and insert it in `default_order` after `Metal`.
  All later stage ids stay put, so only slot-indexed tests move.
- `is_mono_pedal` gains it; `stereo_stages_follow_the_amp` must still pass.
- Chain tests: extend `preset_chain_order_applies_and_round_trips`
  (`preset.rs:1651`) with a `"rat"` name, and confirm an unknown/missing stage
  still sanitizes.
- Re-bless `add_pedal_modal.snap`; check `default_screen` (should be unchanged
  because the pedal is off and therefore has no tile).
- Baseline untouched (`--check`).
- Docs: `AGENTS.md` chain, README.

### Phase 4 — Tube Driver (M)

Same checklist. Append id 22, default order after the RAT. `DEVICE_YEARS` entry
now (even if no bundled preset uses it yet) so the anachronism gate is already
armed when the content phase lands.

### Phase 5 — Rotary (L)

The only phase with real DSP risk, done last so the plumbing is proven.

- Append id 23, default order **after `Cab`** — the only insertion that shifts
  the rack indices (Geq onwards, +1).
- Add `clear_buffers` to `snapshot_order`.
- Update the two explicit-chain presets (`…numb_solo_1.toml`,
  `…numb_solo_2.toml`) to place `"rotary"` after `"cab"`; otherwise a user who
  switches it on gets it post-reverb.
- Optional in this phase: `MidiTarget::RotarySpeed` (+ `ALL` test 15 → 16) and
  the `midi_modal` re-bless.
- Tests per §5.7. Baseline untouched.
- Re-bless any snapshot that shows the new colour/tile; `add_pedal_modal` again.

### Phase 6 — content (separate PR, no new machinery)

- New presets (Shine On / Sorrow / Echoes / Run Like Hell / Dogs / Marooned) and
  the chain-order A/B experiments in the research doc.
- Anachronism decisions (§4.10).
- Update `docs/fidelity-references.md` §1 inventory.
- **This is the only phase where a baseline re-bless is legitimate**, and only
  if a bundled preset's audio actually changes. Regenerate + record in
  `docs/fidelity-implement.md`.

### Test strategy matrix

| Phase | Unit | Chain/order | Preset | Snapshot | Fidelity |
| ----- | ---- | ----------- | ------ | -------- | -------- |
| 1 | voice distinctness, existing 5 unchanged | — | field absent → voice 0; `rig_values` updated | none | `--check`, no re-bless |
| 2 | model distinctness (Goertzel), existing tests | — | `[fuzz] model` round-trip | maybe `pedalboard_focused` | `--check`, no re-bless |
| 3 | finite/bounded, filter direction | `default_order` permutation, `pedal_index` tripwire, `sanitize` with `"rat"` | `from_params`/`apply` round-trip, deterministic load | `add_pedal_modal` | `--check`, no re-bless |
| 4 | as 3 + bias/asymmetry | as 3 | as 3 | `add_pedal_modal` | `--check` |
| 5 | §5.7 (dry, bounded, L/R phase, ramp pitch, clear) | rack indices, stereo-before-amp guard | two explicit-chain presets updated | any tile/colour shot | `--check` |
| 6 | — | chain-order experiments | new presets parse + determinism | modal shots | **re-bless if audio changed** |

---

## 7. Things in the research doc that are infeasible or more expensive

1. **"Corner-frequency sweep" inside the rotary (§1).** A rotating-source
   resonance sweep means rebuilding biquad coefficients every sample from the
   rotor angle. It is the least audible part of a Leslie and the most
   zipper-prone. Feasible, but not worth it in v1: ship AM + phase-derived
   doppler + crossover + horn colour. Budget it as a separate follow-up if ears
   demand it.
2. **"Can reuse the chorus/delay ring-buffer plumbing" (§1).** Only partly:
   there is no reusable ring-buffer *type*, just `read_cubic`
   (`effects/mod.rs:100`) and a convention. The rotary still owns its buffers
   and `clear_buffers`. This does not change the effort much, but don't plan for
   a shared abstraction that would have to be invented first.
3. **"Medium — one new stereo effect, two rotors" for the rotary.** The DSP is
   medium-plus *for this codebase* because it is the only item that is neither a
   copy of a stompbox topology nor a parameter. Rate it L and order it last.
4. **"Low — one extra parameter and per-model scoop/gain/HP constants" for the
   Muff (§2).** True for `fuzz.rs`; false for the change as a whole. It also
   touches `Params` (5 sites), the preset schema, `rig_values`,
   `hostile_params`, the `KNOBS` tripwire and the UI. Still S, but ~5 files, not
   one.
5. **"Optical (Compulator; program-dependent release)" (§5).** This is not a
   voice-table entry like Dyna vs CS-2; it needs a second release detector.
   S/M, and it belongs after the first two voices ship. Consider deferring it or
   cutting it to "CS-2 + Dyna Comp".
6. **The four Big Muffs are not four circuits.** With one fixed topology you
   get four *voicings* (scoop centre/depth, HP corner, gain, level trim). The
   doc's framing ("four eras for the price of one effect") is fine as a
   marketing sentence but the preset descriptions and
   `docs/fidelity-references.md` must say "voiced like", not "models the Ram's
   Head circuit".
7. **"Adding 4–6 pushes it to 23–25 tiles" (§Open questions 3).** Tier 1 adds
   **3** pedals (rotary, Tube Driver, RAT) → 22 tiles, 98 knobs. The 23–25 range
   only applies if Tier 2 pedals are included. The layout cost is one extra tile
   row on ≤85-column terminals (§4.3) — acceptable; no paging needed.
8. **Open question 2's "post-cab rack stage" is right, but not for the stated
   reason.** It is right because a Leslie is *mic'd*, and because the later rack
   should colour the rotary rather than the reverse. It is *not* a cabinet
   replacement in this architecture: the cab IR still runs, so it is two speaker
   colorations in series. Document it as "rotary in the room".
9. **Open question 4 hides a build failure.** Re-voicing the 1979 Comfortably
   Numb presets onto the Tube Driver trips
   `no_new_device_anachronisms` (`preset.rs:1280`). Not a docs decision — a test
   failure. Decide before phase 6 (§4.10).
10. **The `[chain]` tail-append behaviour is a real (if small) correctness
    trap.** Two bundled presets have explicit chains, so the rotary would land
    after the reverb for them. One line per preset fixes it (§4.4).
11. **Minor: the research doc's effort column is optimistic for *any* new
    pedal** because it counts the DSP and not the 18 `dsp/mod.rs` sites, the
    three tripwires, the preset sections and the snapshot re-bless. The RAT is
    the cheapest possible example of this and still lands at M for the PR.

---

## Appendix A — touch-point counts, Tier 1

| Item | New files | Files touched | Sites | New `KNOBS` rows | New stages | Approx. diff |
| ---- | --------- | ------------- | ----- | ---------------- | ---------- | ------------ |
| Compressor voice | 0 | 5 | 11 | 1 | 0 | 60–120 |
| Muff model | 0 | 5 | 12 | 1 | 0 | 80–150 |
| RAT | 1 | 10 | 30 | 3 | 1 | 250–400 |
| Tube Driver | 1 | 10 | 31 | 4 | 1 | 300–450 |
| Rotary | 1 | 11 | 34 | 5 | 1 | 450–700 |

(Sites are distinct edited functions/entries, not lines.)

## Appendix B — checklist for every phase

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-features`
- [ ] `INSTA_UPDATE=always cargo test --lib 'ui::'`, review the diff, commit the `.snap`
- [ ] `cargo run --release --example fidelity_render -- --check docs/fidelity/baseline-synth-48k.toml`
- [ ] `AGENTS.md` chain diagram + README (stage phases)
- [ ] `docs/fidelity-implement.md` increment-log row (topology phases)
- [ ] `src/preset.rs` `rig_values` + `hostile_params` (knob phases)
- [ ] tripwires bumped (`config.rs:772`, `:773`; `midi.rs:682` if targets)
- [ ] manual audio pass (`CONTRIBUTING.md`, "Manual audio pass")
