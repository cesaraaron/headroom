# Audit Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every verified finding in `docs/audit-2026-11-16.md`, leaving the repo green on both CI jobs with documentation that matches the code.

**Architecture:** Eleven tasks grouped by subsystem. Documentation corrections are folded into the task that owns the file, so no task edits a file another task also edits. Code fixes follow RED→GREEN where a focused failing test is feasible; doc-only and compile-time fixes are gated by `cargo fmt`, `cargo clippy` and the full test suite. The two help-modal goldens are re-blessed together whenever help text changes.

**Tech Stack:** Rust 1.96, ratatui, insta snapshots, `rtrb`, serde/TOML, `hound`, GitHub Actions, `gh` CLI.

**Spec:** `docs/audit-2026-11-16.md` (the audit; travels with this plan).

## Global Constraints

- Rust 1.96; gates are `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` (default/AU, 685 pass), `cargo test --no-default-features --features clap` (no-AU, 682 pass).
- No heap allocations and no blocking on the audio thread; biquad state preserved across buffers; knobs normalized 0.0–1.0; rectifier sag state not reset on model switch.
- Two help goldens: `headroom__ui__draw__tests__help_modal.snap` (macOS + `au`) and `..._help_modal_no_au.snap` (everything else). Any help-text change MUST re-bless both:
  `cargo test` then `INSTA_UPDATE=always cargo test --no-default-features --features clap --lib snapshot_help_modal`, then confirm no `*.snap.new` remains.
- Never set `INSTA_UPDATE` in CI; `.gitignore` excludes `*.snap.new`.
- Docs (including agent-written plans/specs, status blocks, commit `Gates:` lines) are claims to verify, not authority.
- Delivery: commit and push to `main` (user's chosen method, 2026-11-16); branch is unprotected and all recent history is linear on `main`.
- **Ruling (worktree):** work directly on `main` rather than creating an isolated worktree — the user explicitly chose "commit & push to main" in this session and said not to stop; a worktree would add a session move without changing the review boundary. Cost if wrong: a bad intermediate commit lands on main; mitigated by verifying every gate before each push.
- **Ruling (harness):** progress is tracked in the committed plan checkboxes plus `.superpowers/sdd/2026-11-16-audit-remediation/progress.md`; the `task-start`/`task-done` scripts are replaced by direct ledger edits because they assume a subagent-driven branch range. Cost if wrong: less automation, but the git commit log remains the durable record.
- **Ruling (doc-only tasks):** tasks with no behavioral change have no RED→GREEN test; their gate is the full verification command in the task's last step. Cost if wrong: a doc typo could survive; mitigated by grepping each corrected claim in the same task.
- **Ruling (step granularity):** a step may bundle several edits to one file when they are the same class of correction (a doc sweep), because splitting them would produce ~120 near-identical steps without changing what a reviewer can reject. Cost if wrong: one bad edit inside a bundle is harder to isolate; mitigated by `git diff` review at each commit.
- **Ruling (retain archival files):** `.opencode/agent/*.md`, `docs/superpowers/specs/*` and `docs/superpowers/plans/*` are retained but marked superseded rather than deleted — they are the historical record the roadmap handoff cites. Cost if wrong: dead files remain in the tree; they are small and clearly labelled.

## Review Focus

| Input / condition | Expected behavior | Pinned by |
| --- | --- | --- |
| `midi.conf` copied from the shipped example (inline `#` comments) | wah CC 11 binding survives | T5 `midi_conf_strips_inline_comments` |
| Preset TOML with a misspelled optional key | parse error, not a silent default | T5 `preset_rejects_unknown_fields` |
| Two `Session::new` calls in the same second | distinct `recovery_dir()` paths | T5 `sessions_created_in_the_same_second_do_not_collide` |
| IR WAV reporting `bits_per_sample = 0` | clean error / clamped scale, no panic | T6 `int_scale_handles_zero_bits` |
| Full rtrb disposal ring during a swap | command deferred, no large free in callback | T8 `a_full_disposal_ring_defers_the_command` |
| Preset switches to a 2-knob amp while focus is on amp knob 6 | focus clamped to the last real knob | T9 `resync_clamps_hidden_amp_knob_focus` |

---

### Task 1: Tracked artifacts and ledger

**Files:**
- Create: `docs/audit-2026-11-16.md`, `docs/superpowers/plans/2026-11-16-audit-remediation.md`, `.superpowers/sdd/2026-11-16-audit-remediation/progress.md`

**Interfaces:** Produces the spec path and the plan path every later task references.

- [x] **Step 1:** Write the audit spec (`docs/audit-2026-11-16.md`) with all 57 findings and the verified-clean list.
- [x] **Step 2:** Write this plan with `- [x]` steps.
- [x] **Step 3:** Create the ledger with first line `# SDD ledger — plan: docs/superpowers/plans/2026-11-16-audit-remediation.md`, record the two rulings above, and `Pre-flight: no shared interfaces`.
- [x] **Step 4:** Verify both files are tracked and `git status` shows only the two new docs.
- [x] **Step 5:** Commit: `docs: add the 2026-11-16 audit and remediation plan`.

### Task 2: Agent-facing documentation

**Files:**
- Modify: `AGENTS.md:44,47,100-105,110,113-114`, `.claude/skills/add-pedal/SKILL.md:28,30`, `roadmap-next.md:17-73,61,286-295,302-307`, `docs/parallel-work.md:11-20,30-35,52-54,67,71,97-98`, `.opencode/agent/tests-writer.md:28`, `docs/superpowers/specs/*.md:3` (7 files)

- [x] **Step 1:** `AGENTS.md`: `Space`→`D` (and note `Space`/`Enter` are transport); "Output limiter (per-channel soft-clip)"→"master-bus lookahead limiter (stereo-linked, −1 dBFS)"; add `docs/guide.md` to the user-facing guide list; list all 13 examples; demote the roadmap handoff pointer to "historical, see the audit spec".
- [x] **Step 2:** `.claude/skills/add-pedal/SKILL.md`: `D`-to-remove→`Del`-to-remove; "`Space` can bypass"→"`D` bypasses".
- [x] **Step 3:** `roadmap-next.md`: retitle the handoff `## Handoff — historical (2026-10-12, superseded)`; add a one-paragraph "Current state (2026-11-16, 83602a8)" summary; correct "opens with only the practice timeline / `Panels::timeline_only()`"→"every panel visible (test-only layout)"; `D` removes→`Del` removes; "single theme"/"Theme #2 is data entry"→two shipped themes `rust`/`teal`; note `2026-10-12-palette-design.md`'s decision was reversed.
- [x] **Step 4:** `docs/parallel-work.md`: label "Current split (2026-10-11)" as historical; add a note that `opencode.json` disables all six agents so the runbook is archival.
- [x] **Step 5:** `.opencode/agent/tests-writer.md:28`: `17 presets`→`5 presets`.
- [x] **Step 6:** Prepend `> **SUPERSEDED:** shipped; see docs/audit-2026-11-16.md.` after line 1 of each `docs/superpowers/specs/*.md`; note the two plans without specs in `roadmap-next.md`.
- [x] **Step 7:** Verify: `rg -n "Space\`|per-channel soft-clip|17 presets|Theme #2|timeline_only" AGENTS.md .claude/skills roadmap-next.md docs/parallel-work.md .opencode/agent` returns no stale claims.
- [x] **Step 8:** Commit: `docs: correct agent-facing keys, limiter, handoff and archived specs`.

### Task 3: As-built records and code-level doc comments

**Files:**
- Modify: `AGENTS.md:74`, `docs/retired-presets.md:5-8,70,121,122`, `docs/fidelity-implement.md:2684,2687,293`, `docs/research/2026-11-02-gilmour-gap-review-facts.md:5`, `CONTRIBUTING.md:73-87,253-262,331`, `src/lib.rs:17-18`, `src/preset.rs:61,389-391`, `src/export.rs:16-18`, `src/project.rs:26-27,35-43`, `src/audio/mod.rs:810-814`

- [x] **Step 1:** Collapse the preset restore/curate dates to the single commit `8ce656e` (2026-11-13) in `AGENTS.md`, `docs/retired-presets.md`, `docs/fidelity-implement.md`; "eight surviving"→"five curated".
- [x] **Step 2:** Fix the broken research link (`...gap-analysis.md`→`...gear-gap-analysis.md`).
- [x] **Step 3:** `docs/fidelity-implement.md:293` R10: replace "**resolved** (B1, B2)" with an honest note that helpers now exist in `src/analysis/metrics.rs` but six `examples/*.rs` still carry local copies.
- [x] **Step 4:** `CONTRIBUTING.md`: `reference_version = 1`→`3`; make the two-golden bless command the primary instruction; rewrite "Code style" so it says CI enforces default clippy groups and the strict policy is aspirational (no inert TOML claim); add a note that commit-message `Gates:` lines record local runs and CI is the authority (finding 26).
- [x] **Step 5:** `src/lib.rs:17-18`: looper is take/program-bus audio re-amped by `take_chain`, not monitor-only.
- [x] **Step 6:** `src/preset.rs:61,389-391`: default width `1.0` (neutral), `1.3` = `WIDE_MASTER_WIDTH`.
- [x] **Step 7:** `src/export.rs:16-18`: external AU/CLAP are re-instantiated from captured state on the worker; refusal is per-plugin when capture fails.
- [x] **Step 8:** `src/project.rs`: correct the Load/Partial-load contract to what `read_manifest`/`load_external` actually do after T6 (write this step after T6 lands, in the same commit as T6 if convenient — see Ruling in ledger).
- [x] **Step 9:** `src/audio/mod.rs:810-814`: "just over the knee, engaging only a fraction of a dB" instead of "just under … transparent".
- [x] **Step 10:** Verify with `rg` that no stale date/`1.3`/`reference_version = 1`/broken link survives; run `cargo fmt --all -- --check`.
- [x] **Step 11:** Commit: `docs: correct as-built records, dates and module contracts`.

### Task 4: Help overlay and keybinding accuracy

**Files:**
- Modify: `src/ui/draw.rs:1691-1695,1786-1789,1823,1839`, `src/ui/mod.rs:2410`, `src/ui/practice.rs:2507-2508`, `src/ui/snapshots/headroom__ui__draw__tests__help_modal.snap`, `src/ui/snapshots/headroom__ui__draw__tests__help_modal_no_au.snap`
- Test: `src/ui/draw.rs` `#[cfg(test)]` module

- [x] **Step 1:** Write the failing snapshot check: run `cargo test --no-default-features --features clap --lib snapshot_help_modal` and `cargo test --lib snapshot_help_modal` — both currently pass; the RED step is the deliberate help-text edit in Step 2, which makes them fail. Record the expected FAILED diff.
- [x] **Step 2:** `draw.rs`: move the cheat-sheet doc from `render_export_progress` to `render_help_modal`; `row("  F3", "  cycle the UI theme (saved)")`; `head("Panels")`→`head("Tracks")` before the `B` row; keep line counts unchanged so the height-clamped tail does not shift.
- [x] **Step 3:** `src/ui/mod.rs:2410`: bind `KeyCode::Char('d') | KeyCode::Char('D')`.
- [x] **Step 4:** `src/ui/practice.rs:2507-2508`: hint `M`→`m` for mute.
- [x] **Step 5:** Run both snapshot tests — expected FAILED on the two changed rows.
- [x] **Step 6:** Re-bless both goldens with the two commands in Global Constraints; confirm no `*.snap.new`.
- [x] **Step 7:** Verify both snapshot tests pass plus `cargo test --lib ui::draw` and `cargo fmt --all -- --check`.
- [x] **Step 8:** Commit: `fix(ui): correct help keys, sections and docs; re-bless both goldens`.

### Task 5: Silent-failure bugs in state and storage

**Files:**
- Modify: `src/midi.rs:454-486`, `src/preset.rs:21` + all section structs, `src/session.rs:154-164,195-226`
- Test: `src/midi.rs`, `src/preset.rs`, `src/session.rs` `#[cfg(test)]` modules

- [x] **Step 1:** RED — add `midi_conf_strips_inline_comments` asserting `parse_midi_conf("enabled = true\ncc = 11   # wah\n20 = delay_mix\n")` yields a `WahPosition` binding on CC 11; add `midi_conf_booleans_are_case_insensitive`. Run `cargo test --lib midi::tests` → FAIL.
- [x] **Step 2:** `parse_midi_conf`: strip from the first `#` before `split_once('=')`; compare booleans case-insensitively via `eq_ignore_ascii_case` against `false/0/no/off`.
- [x] **Step 3:** GREEN — `cargo test --lib midi::tests` → PASS.
- [x] **Step 4:** RED — add `preset_rejects_unknown_fields` asserting `toml::from_str::<Preset>` of a minimal valid preset with `[delay] typee = "digital"` returns `Err`. Run → FAIL (serde currently ignores it).
- [x] **Step 5:** Add `#[serde(deny_unknown_fields)]` to `Preset` and every `*Section` struct in `src/preset.rs` (mechanical, script-assisted); ensure no `#[serde(flatten)]` conflicts.
- [x] **Step 6:** GREEN — run `cargo test --lib preset::tests` and `cargo test --test bundled_presets` → PASS; confirm all 5 bundled presets still parse.
- [x] **Step 7:** RED — add `sessions_created_in_the_same_second_do_not_collide` asserting two `Session::new(48_000)` calls have different `temp_id()`; add `revision_tracks_persisted_track_metadata` asserting mutating `asset`/`input_trim_db`/`calibration_ref` changes `revision()`. Run → FAIL.
- [x] **Step 8:** `session.rs`: seed `temp_id` with seconds + nanoseconds + PID (no allocation in DSP path — this is control-thread only); hash `asset`, `input_trim_db`, `calibration_ref` in `revision()`.
- [x] **Step 9:** GREEN — `cargo test --lib session::tests` → PASS.
- [x] **Step 10:** `cargo fmt --all -- --check` + `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] **Step 11:** Commit: `fix: stop silently dropping midi bindings, preset typos, recovery collisions and dirty state`.

### Task 6: Project/export robustness and IR loading

**Files:**
- Modify: `src/project.rs:273-331,285-307,607-639`, `src/dsp/cab/external.rs:100-112`
- Test: `src/project.rs`, `src/dsp/cab/external.rs` `#[cfg(test)]` modules

- [x] **Step 1:** RED — `int_scale_handles_zero_bits`: assert `int_scale(0)` equals `int_scale(1)` (clamped) and `int_scale(16) == 1.0 / 32768.0`; run `cargo test --lib dsp::cab::external` → FAIL (function undefined).
- [x] **Step 2:** Extract `fn int_scale(bits: u16) -> f32` using `bits.clamp(1, 32)` and use it in the integer decode arm; reject `bits_per_sample == 0` in the loader with a clear error.
- [x] **Step 3:** GREEN — `cargo test --lib dsp::cab::external` → PASS.
- [x] **Step 4:** RED — `write_session_rejects_escaping_asset_rel`: build a `Session` whose track asset `rel` is `../escape.wav` and assert `write_session` returns `Err` and creates no file outside the dir. Run → FAIL.
- [x] **Step 5:** Apply `resolve_asset(dir, rel)?` on the write paths (`project.rs:323,331`) before `dir.join`; keep the existing "skip copying a file onto itself" behavior.
- [x] **Step 6:** GREEN — `cargo test --lib project::tests` → PASS.
- [x] **Step 7:** `read_manifest`: add asset-path validation (`resolve_asset`) and existence checks so `list_sessions` skips invalid folders; add `read_manifest_rejects_escaping_asset` test RED→GREEN.
- [x] **Step 8:** `load_external`: on any sidecar read/parse failure return `None` for that spec (never show an absent processor as active) and log the reason via `eprintln!` on the control thread; add `load_external_hides_a_missing_sidecar` RED→GREEN. **Ruling:** a full `Result` status channel would require changing `SessionExternal` and every `ui/mod.rs` restore call; `None` + a control-thread warning satisfies the contract's "never display as active" clause without a cross-module API change. Cost if wrong: the UI cannot name the missing plugin; it will show the built-in rig.
- [x] **Step 9:** Update `src/project.rs` module docs (T3 Step 8) to match the implemented behavior.
- [x] **Step 10:** `cargo test --lib project::tests` + fmt + clippy.
- [x] **Step 11:** Commit: `fix(project): validate asset paths on write, report missing plugin sidecars, clamp IR bit depth`.

### Task 7: DSP correctness and dead state

**Files:**
- Modify: `src/dsp/amp/mod.rs:1077,1134-1142,1143-1171,2061-2150`, `src/dsp/oversample.rs:106-120`, `src/dsp/effects/spring.rs:170-186`, `src/dsp/mod.rs:217-221`, `src/dsp/effects/rotary.rs:44,150`, `src/dsp/cab/mod.rs:359-363`, `src/host/clap_host.rs:257-260,545-566`
- Test: `src/dsp/amp/mod.rs`, `src/dsp/cab/mod.rs` `#[cfg(test)]` modules

- [x] **Step 1:** RED — `rapid_amp_switch_uses_the_outgoing_models_knobs`: drive `AmpBank::process` with a switch A→B→C inside the 6 ms ramp and assert the fade output does not jump to A's gain curve (compare against a reference render with B's knobs). Run → FAIL.
- [x] **Step 2:** Add `fading_knobs: [f32; AMP_MAX]` to `AmpBank`, set it from the outgoing model's knobs at every switch, and use it for `process_model(old, …)`; keep `prev_knobs: [f32; AMP_MAX]` for the settled case.
- [x] **Step 3:** GREEN — `cargo test --lib dsp::amp::tests` → PASS.
- [x] **Step 4:** `dsp/amp/mod.rs:1077`: use `crate::dsp::BYPASS_DECLICK_SECS` instead of `0.006`. Add `last_load: Option<(f32, f32)>` to `AmpBank` and push `set_load` to the nine models only when `load` changed (finding 51); `CabModel::speaker_load()` returns `(f32, f32)`.
- [x] **Step 5:** `oversample.rs` / `spring.rs`: move the doc comment and `#[inline]` from `reset`/`clear` onto `process`; give `reset`/`clear` their own one-line docs. `dsp/mod.rs:217-221`: rehome the orphaned Greenback paragraph.
- [x] **Step 6:** `rotary.rs`: `#[cfg(test)]`-gate `last_horn_delay` and its per-sample write. `cab/mod.rs`: drop `d_side`/`d_diag` and recompute them from `taps` in the test. `clap_host.rs:257-260`: remove the redundant `#[allow(dead_code)]` on `instance`.
- [x] **Step 7:** `clap_host.rs:545-566`: on `processor.process()` `Err`, return without copying `out_buf` into `left/right` (mirror the AU host). Add `clap_insert_passes_dry_on_process_error` if a mock processor is available; otherwise pin by the existing `host::clap_host::tests` plus static review. **Ruling recorded in ledger if no mock exists.**
- [x] **Step 8:** Delete the three permanently-ignored ripple tests (`amp/mod.rs:2061-2150`) and add a "Measured negative (Vox/Hiwatt/Fender ripple)" note to `docs/fidelity-implement.md` recording the finding; update `roadmap-next.md`'s "tests #[ignore]d" wording.
- [x] **Step 9:** `cargo test --lib dsp::` + fmt + clippy; record the new test counts.
- [x] **Step 10:** Commit: `fix(dsp): rapid-switch knobs, IR/host error paths, inline hints and dead state`.

### Task 8: Audio-thread disposal safety

**Files:**
- Modify: `src/audio/mod.rs:990-1050`
- Test: `src/audio/mod.rs` `#[cfg(test)]` module

- [x] **Step 1:** RED — add `a_full_disposal_ring_defers_the_command`: create `RingBuffer<u32>` command and `RingBuffer<Box<Vec<f32>>>` disposal rings of capacity 1, pre-fill disposal, push one command, call `drain_with_disposal`, assert the command is still in the ring and no `Box` was dropped; drain the disposal ring, call again, assert the command is consumed and the displaced buffer is now in the disposal ring. Run → FAIL (function undefined).
- [x] **Step 2:** Add `fn drain_with_disposal<C, P>(cmds: &mut Consumer<C>, disposal: &mut Producer<P>, apply: impl FnMut(C) -> Option<P>)` that loops only while `!disposal.is_full()`, pops a command, applies it, and pushes the displaced value (guaranteed to succeed). Use it for the six insert/IR/amp swap loops and the track-command loop, adapting each closure to return the displaced value.
- [x] **Step 3:** Correct the `audio/mod.rs:1030-1032` comment to describe the actual deferral, and remove the "drop it here as a last resort" wording at `:993-995`.
- [x] **Step 4:** GREEN — `cargo test --lib audio::tests` → PASS.
- [x] **Step 5:** Verify `cargo test --lib audio::` and `cargo clippy --all-targets --all-features -- -D warnings`; confirm no `let _ = …push(old)` remains in `on_input`/`on_input_chunk`.
- [x] **Step 6:** Commit: `fix(audio): defer swaps when the disposal ring is full instead of freeing on the callback`.

### Task 9: UI focus repair and per-frame work

**Files:**
- Modify: `src/ui/mod.rs:149-163`, `src/ui/input.rs:414-425`, `src/ui/draw.rs:127,339,988`
- Test: `src/ui/input.rs` `#[cfg(test)]` module

- [x] **Step 1:** RED — `resync_clamps_hidden_amp_knob_focus`: build `params`/`board`/`panels`, set focus to `AMP_START + 6`, switch the amp model to a 2-knob one, call `resync_board_after_rig_change`, assert the returned focus is `< AMP_START + amp_knob_count()`. Run → FAIL.
- [x] **Step 2:** Make `ensure_focus_visible` use `focus_reachable`, or add the `f - AMP_START >= params.amp_knob_count()` clamp to `resync_board_after_rig_change` (choose whichever keeps existing `hiding_the_focused_panel_repairs_focus` passing).
- [x] **Step 3:** GREEN — `cargo test --lib ui::input::tests` → PASS.
- [x] **Step 4:** `draw.rs`: compute `rendered_stages(&params.chain_slots(), board)` once in `draw_at` and pass `&[(…)]` into `render_header` and `render_rig`; keep signatures explicit and update all callers incl. tests.
- [x] **Step 5:** `cargo test --lib ui::draw::tests` + fmt + clippy; re-run both help snapshots if any rendered output changed (none expected).
- [x] **Step 6:** Commit: `fix(ui): repair hidden amp-knob focus and compute rendered stages once per frame`.

### Task 10: Build, CI and lint-policy cleanup

**Files:**
- Modify: `Cargo.toml:88-186`, `clippy.toml`, `CONTRIBUTING.md:73-87`, `.github/workflows/test.yml:36,45-57`, `.github/workflows/release.yml:29-31`, `.gitignore`
- Delete: `.config/nextest.toml`

- [x] **Step 1:** `Cargo.toml`: delete the inert `[workspace.lints.*]` block; add a short comment pointing at `CONTRIBUTING.md` for the aspirational style policy. Confirm `cargo metadata --no-deps` still resolves and `cargo clippy … -D warnings` stays green.
- [x] **Step 2:** Rewrite `clippy.toml` to only keys that affect default-enabled clippy lints (`too-many-arguments-threshold`, `type-complexity-threshold`, `vec-box-size-threshold`, `array-size-threshold`, `stack-size-threshold`, `single-char-binding-names-threshold`, `enum-variant-name-threshold`, `large-error-threshold`) with honest comments; drop the rest.
- [x] **Step 3:** `CONTRIBUTING.md` "Code style": state that CI enforces the default clippy groups via `-D warnings`, that the strict no-unwrap/panic policy is a target for non-test code, and that there is no workspace lint opt-in.
- [x] **Step 4:** `test.yml`: remove the unreachable "Check for un-blessed UI snapshots" step; add `cargo test --no-default-features --features clap` as a Linux step; wrap the apt install in a 3-attempt retry loop.
- [x] **Step 5:** `release.yml`: add `toolchain: "1.96"` to the Install Rust step.
- [x] **Step 6:** Delete `.config/nextest.toml`; confirm no `nextest` reference remains.
- [x] **Step 7:** Verify locally: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, both test branches, and `cargo check --no-default-features`.
- [x] **Step 8:** Commit: `chore(ci): remove dead lint config, cover the minimal build, pin release toolchain`.

### Task 11: Final verification, review, delivery

**Files:**
- Modify: this plan's checkboxes, the ledger

- [x] **Step 1:** Re-read `docs/audit-2026-11-16.md` and tick every item addressed; list any item deliberately not fixed as a `Ruling:` in the ledger with its cost.
- [x] **Step 2:** Full gate run: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo test --no-default-features --features clap`, `cargo check --no-default-features`, `find . -name '*.snap.new'`.
- [ ] **Step 3:** Whole-branch review: dispatch one fresh `general` subagent with the diff range `83602a8..HEAD`, this plan, the spec, the Review Focus table verbatim, and the ledger's `Ruling:` lines; ask for severity-graded findings.
- [ ] **Step 4:** Re-grade findings by user effect; fix Critical/Important in one RED→GREEN pass with a full green suite; ledger deferred Minors.
- [ ] **Step 5:** Mark all checkboxes complete, delete `.superpowers/sdd/2026-11-16-audit-remediation/`, commit the plan updates.
- [ ] **Step 6:** Push to `main`; watch the CI run with `gh run watch --exit-status` and confirm both jobs pass.
- [ ] **Step 7:** Report: findings fixed, rulings, deferred minors, CI run URL.
