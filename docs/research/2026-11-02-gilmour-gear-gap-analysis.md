# Gilmour gear gap analysis — 2026-11-02

Status: **research + review complete, nothing built.** This document is the
authoritative version; it incorporates the corrections from
[`2026-11-02-gilmour-gap-review-facts.md`](2026-11-02-gilmour-gap-review-facts.md)
(fact-check) and
[`2026-11-02-gilmour-gap-review-feasibility.md`](2026-11-02-gilmour-gap-review-feasibility.md)
(code feasibility). The original first-pass claims that were wrong are called out
inline where it matters, so the errors are not re-introduced later.

Companion: `docs/research/2026-10-18-comfortably-numb-tone.md` (the two Numb
presets specifically).

## Why this exists

The rig already models the *core* Gilmour chain — Hiwatt DR103, WEM 4×12 with
Fane Crescendos, Big Muff, Fuzz Face, Uni-Vibe, Electric Mistress, Phase 90,
CE-2, Echorec/Echoplex, Whammy, Dyna-Comp-style compressor, Power-Boost-style
clean boost, GE-7. What it does **not** model is the gear that separates one
Gilmour era from another: the rotary cabinet, the Tube Driver, the RAT, the
Big Muff variants, and the different compressor voices. Those are the gaps this
document inventories.

## Method and sources

- **Gilmourish** era pages (photos, interviews and tech notes), used as the
  primary source and actually followed per era this time:
  Dark Side `?page_id=16`, WYWH `17`, Animals `18`, The Wall `20`,
  About Face `22`, MLOR/DSoT `2111`, Division Bell `31`, PULSE `2117`
  (settings `2119`), Meltdown `156`, On an Island/Gdansk `32`/`218`,
  Pompeii 2016 `7538`/`7544`, Big Muffs `79`/`3389`, compressors `7130`,
  rotary `4554`, Power Boost `7748`, RAT `262`.
  <https://www.gilmourish.com/>
- Kit Rae, “David Gilmour Tone Building”, “Effects and Gear”, and “Signal Chain
  Order”. <http://www.kitrae.net/music/David_Gilmour_Effects_And_Gear.html>
- Justin Beckner, “The gear used by David Gilmour to record *The Wall*”,
  Guitar.com, 2024-07-30.
- GroundGuitar, Hiwatt DR103 and Alembic F-2B pages (Phil Taylor quotes).
- Reverb, “How to Nail Gilmour's Comfortably Numb Solo” (good on the RA-200,
  unreliable on the delay unit).
- UberProAudio is **demoted**: its “reportedly used” list is the origin of the
  Mark II C+ error and should not be used for model numbers.

Caveat repeated by every source: rig lists are reconstructed from photos and
interviews, not session sheets. A pedal “in the rig” is not proof it was on a
given record. Where the evidence is contested it is marked **[contested]**.

---

## 1. What the app models today

| Stage | App name | Closest real unit | Gap |
| --- | --- | --- | --- |
| Gate | Noise Gate | MXR Noise Gate / generic | — |
| Pitch | Whammy | DigiTech WH-1 | — |
| Wah | Wah | Vox / Cry Baby, manual + envelope | no reversed-wah preset helper |
| Comp | Compressor | generic peak compressor; Dyna-Comp-ish | **no voice switch** |
| Fuzz | Big Muff / Fuzz Face / Tone Bender | one generic Muff voicing | **no Ram's Head / Triangle / Sovtek split** |
| Drive | TS-808 | Ibanez TS-808 | Gilmour used a **TS-10**, occasionally |
| Dist | DS-1 | Boss DS-1 | Gilmour mostly used RAT / HM-2 instead |
| Dist | ML-2 Metal Core | Boss ML-2 | not a Gilmour pedal |
| EQ | Pre-amp EQ | generic 3-band | — |
| Mod | Uni-Vibe | Univox Uni-Vibe | — |
| Boost | Power Boost | Colorsound Power Boost **but linear/clean** | real unit is a **silicon** overdrive, not clean |
| EQ | GE-7 | Boss GE-7 | — |
| Mod | Flanger (generic / Electric Mistress) | EHX Deluxe Electric Mistress | — |
| Mod | CE-2 | Boss CE-2 | the RA-200 stand-in today |
| Mod | Phaser (generic / Phase 90) | MXR Phase 90 | **no Phase 100** |
| Mod | Tremolo / Vibrato | generic | Demeter Tremulator not voiced |
| Delay | digital / Echoplex / Echorec | MXR DDL, EP-3, Binson | no 2290/DD-2 voice |
| Reverb | Spring | Fender spring | fine |
| Amp | 9 models | Hiwatt DR103 ✓, Plexi ✓, Twin ✓, AC30 ✓ | no **Mesa Mark I/III**, no **Hiwatt SA212/DR504** |
| Cab | 8 models | WEM 4×12 Fane ✓, Marshall 4×12 ✓, Fender 2×12 ✓ | no **Dual Showman 2×15 JBL**, no Hiwatt SE-4123 |

## 2. Gilmour's rigs by era (corrected)

| Era | Amp / cab | Drive / fuzz | Modulation | Delay / other |
| --- | --- | --- | --- | --- |
| **1968–73** | Hiwatt DR103, WEM 4×12 Fane; Maestro Rover + Leslie 147 | Fuzz Face (germanium → silicon BC108 c. 1972), Colorsound Power Boost from early 1972 | Uni-Vibe, Phase 90 (74–75) | Binson Echorec II |
| **1975 WYWH** | Hiwatt, WEM, Fender Dual Showman + 2×15 JBL, Leslie 147 | Fuzz Face, Power Boost | Phase 90 | Echorec |
| **1977 Animals** | Hiwatt + Marshall Super Lead 1959, WEM, **Yamaha RA-200** | Ram's Head Muff, Cornish P-1, Power Boost | MXR **Phase 100** | Echorec, MXR DDL |
| **1979–83 Wall/Final Cut** | Hiwatt DR103; Alembic F-2B as the **RA-200 preamp**; WEM; 2× RA-200 live; Mesa Mark I (live) | Ram's Head Muff, Fuzz Face BC109, **Cornish ST-2** (Power Boost clone) | Electric Mistress, Phase 90; Small Stone + 2× CE-2 on the 80–81 tour only | MXR DDL I |
| **1984–87 About Face** | **Fender Twin Reverb II**, Mesa/Boogie **Mark I** (as preamp), Marshall JCM800 4×12 **G12M**, WEM | **Boss HM-2**, Ram's Head Muff (live), RAT **[contested]** | Boss CE-3 | 2× MXR DDL I/II, 2× Boss DD-2 |
| **1987–90 MLOR/DSoT** | Fender Twins (1987) → Hiwatt + Alembic (1988–90); 2× WEM Fane + 2× **Hiwatt SE-4123**; Mesa **Mark III** | Ram's Head Muff, HM-2, **Cornish P-2 (1989+)**; RAT **[contested]**; **TC Booster+**, TS-10 | 2× CE-2; no Mistress documented | MXR DDL, TC 2290, PCM70 |
| **1993–94 Division Bell/PULSE** | Hiwatt + Alembic, 2× WEM Fane, 2× Marshall JCM800 G12M; **Maestro Rover** (studio) / 2× **Doppola** (tour) | **Ram's Head (studio) / Sovtek Civil War Muff (tour)**, Cornish P-2 (backup), **Tube Driver ×2**, RAT II, SS-2 | Electric Mistress, Uni-Vibe, CE-2, Demeter Tremulator, **Dynacord CLS-222** | MXR DDL, TC 2290, PCM70 |
| **2001–02 Meltdown (acoustic)** | Fender **tweed Deluxe** (2001) / **tweed Twin** (2002) + AER | Cornish G-2, Ram's Head, SS-2, one Chandler Tube Driver | — | T.E.S. echo |
| **2006 OAI / Gdansk** | Hiwatt DR103, WEM | **P-1, G-2, Tube Driver ×2**, Triangle Muff + T-Rex Replica (Echoes, from May) | Uni-Vibe, CE-2 | MXR DDL, SDE-3000, T-Rex Replica |
| **2016 Pompeii** | **Hiwatt Custom 50 (DR504)** + Alessandro Redbone, 2× WEM Fane | Ram's Head Muff, Sovtek Muff, **Tube Driver ×3** (boost / OD / distortion) | Electric Mistress, Uni-Vibe — **no CE-2, no rotary** | Providence Chrono, Free The Tone |

Confidence per row (after review): 1968–73 0.8, WYWH 0.7, Animals 0.8,
Wall 0.75, About Face 0.5, MLOR 0.55, Division Bell/PULSE 0.7, Meltdown 0.8
(after splitting), 2006 0.75, 2016 0.7. **No row is good enough to justify a
preset on gear attribution alone; each preset below names the arrangement it
targets.**

## 3. The gaps, ranked

Ranking: **(1) how many iconic Gilmour parts it unlocks, (2) how far it is from
anything the app already does, (3) DSP + plumbing cost**. Effort is from the
feasibility review.

### Tier 1 — build these

**1. Rotary speaker — new effect. Effort: L.**
The single biggest missing sound, and the one most often mis-described. There
are **two families**, not one:
- **Leslie 147** (1968–75): rotating horn + drum; doppler, amplitude tremolo,
  woody midrange. Dark Side, WYWH.
- **Yamaha RA-200 / Doppola / Maestro Rover** (1977 onward): the **speakers
  themselves spin**, which gives a light, chorus-like swirl rather than a full
  Leslie. Animals, The Wall, Division Bell (Rover), PULSE (Doppola).
Today the presets stand in a CE-2 chorus for it, which is the right family but
the wrong motion. **No rotary at Pompeii 2016** — do not cite that.
*Placement:* post-cab, "rotary in the room", mic'd, with its own level; the RA-200
was fed in parallel and mixed under the amps. It must sit **after the delays**
in spirit, but as a chain stage the default is immediately after `Cab`.
*DSP:* mono in → crossover ~800 Hz → horn rotor (delay derived from angle, so
doppler falls out of the delay slope) + drum rotor (mostly AM, slower) → stereo
mic pair → dry/wet. No per-sample transcendentals, two ring buffers allocated in
`new()`, `read_cubic`, `clear_buffers` wired into `snapshot_order`.
*Do not* implement the "corner-frequency sweep" from the first pass: expensive,
zipper-prone, inaudible next to the AM/doppler.
*Knobs (5):* SPEED (slow/fast), RAMP, DEPTH, MIX, BALANCE.
*Why first in the ranking, last in the build order:* biggest sound, only real
new DSP.

**2. Big Muff voicings — extend the Fuzz. Effort: S.**
Gilmour's Muff changed more than any other pedal:
- **Ram's Head** — Animals, The Wall studio, Final Cut, MLOR studio; the
  default.
- **Triangle** — 2006 Echoes only (with a T-Rex Replica).
- **Sovtek “Civil War”** — the PULSE live sound; smoother, bassier, brighter
  mids, less gain.
- **Cornish P-1** — a straight Ram's Head clone (2006 board).
- **Cornish P-2** — **not** a Ram's Head clone: Civil-War-adjacent, smoother and
  deeper, 1989–94, mostly a backup once the Sovtek arrived.
*Caveat:* one topology gives **voicings**, not circuits. Say “voiced like” in
presets and references.
*Model 0 must be bit-identical to today's — the two Numb presets literally
document `type = 0.0 # Ram's-Head voice` and the fidelity baseline depends on
it.*
*Effort note:* the DSP is one file; the change as a whole touches 5 files
(`Params`, preset schema, `rig_values`, `hostile_params`, UI tripwire).
*UI decision needed:* a separate `MUFF` knob (one more knob, 84 → 85, clearly
labelled “Big Muff model only”) vs folding four Muff positions into the existing
`TYPE` (5 positions, no knob growth, but conflates pedal and revision).

**3. BK Butler Tube Driver — new overdrive. Effort: M.**
The defining lead pedal **1993–2016** (not 1987 — the 87–90 rig has none):
Sorrow, Coming Back to Life, Marooned, A Great Day for Freedom, On an Island,
Shine On (Pulse), and the Gdansk “Muff into Tube Driver”.
Starved-plate 12AX7 → asymmetric soft clip → 2-band shelving EQ → level. It is
smoother and thicker than a TS. Gilmour ran **one before the Muff as a boost and
one after as the lead blend** — so the “after the Muff” order is a preset
decision, not a default.
*DSP shape:* `tube_screamer.rs` + `distortion.rs` are the templates; 4× OS.

**4. Compressor voices — extend the Compressor. Effort: S (+M for optical).**
Gilmour always compresses, and the unit changed:
MXR Dyna Comp (1977+), Boss CS-2 (1984–2004), Ibanez CP-9 (1994, Time intro),
Demeter Compulator (2004–16), Origin Cali76 (2015+).
*Corrected voicing:* the first pass had this backwards. **Dyna = fast attack,
bright/twangy, squashy, coloured. CS-2 = darker, smoother, more transparent.**
Optical/Compulator needs program-dependent release (a second detector), so ship
Dyna + CS-2 first and treat optical as a follow-up.

**5. ProCo RAT — new distortion. Effort: M (plumbing) / S (DSP).**
Documented on the **1993 Division Bell sessions** and the **1994 PULSE** board
(“What Do You Want From Me”). About Face and MLOR are **[contested]** — Kit Rae
hears one, no board list has it. “RAT II” is 1988+, so a pre-1988 RAT would be a
big-box/small-box.
*DSP:* input HP → gain → silicon op-amp clip → fixed ~6.5 kHz LP → **reverse**
Filter knob → level. 3 knobs. This is the cheapest first *new stage* and proves
the plumbing.

### Tier 2 — strong, era-specific

| # | Item | Kind | Why | Effort |
| --- | --- | --- | --- | --- |
| 6 | **Cornish SS-2 / G-2** | new or Tube-Driver voice | G-2 = 2001–06 solos (Meltdown, OAI, Echoes seagull); SS-2 = Division Bell/Pulse smooth sustain | M |
| 7 | **MXR Phase 100** | phaser voice | the actual Animals-era phaser (1976–79). **Not** the Small Stone, which is 1980–81 live only | S |
| 8 | **Boss HM-2** | distortion voice/pedal | About Face era, into a Mesa Mark I or Twin; the studio *Sorrow* intro is HM-2 + DD into a Super Champ | S–M |
| 9 | **Colorsound Power Boost as drive** | extend `clean_boost` | the real unit is **silicon** (not germanium) and adds grit/compression; needed for Animals/Wall “hairy Muff” | S |
| 10 | **Alembic F-2B** | amp model (not pedal) | two roles: RA-200 preamp (early 70s), Hiwatt front end (late 80s–early 2000s). The amp model is the honest option | M |
| 11 | **Sound-on-sound / long delay** | delay feature | how Gilmour plays *Shine On* live from 2001: volume swell + 1500 ms, ~20 s repeats into a separate amp. A delay feature, not a pedal — promote above the fuzz trivia | M |

### Tier 3 — nice-to-have (or skip)

| Candidate | Why | Effort | Verdict |
| --- | --- | --- | --- |
| TC 2290 / Boss DD-2 delay voices | Run Like Hell 1994 / Sorrow 750 ms; pristine digital repeat | S | keep |
| Mesa/Boogie **Mark I + Mark III** | About Face/Wall live (Mark I, as a preamp/overdrive) and 1987–90 live (Mark III). **The Mark II C+ is not documented — drop it** | M | keep (renamed) |
| Fender Dual Showman 2×15 JBL cab | WYWH era; open-back 15s | S–M | keep |
| Hiwatt SE-4123 cab (Fane) | 1987–94 live alongside the WEMs | S | keep |
| Hiwatt SA212 50 W combo (Fane) | recurring across MLOR studio, Division Bell, OAI, Rattle That Lock | M | keep |
| Demeter Tremulator voice | 1994 rack | S | keep |
| Univox Super Fuzz | 1993 session sighting, no song | S | footnote only |
| Roland BF-1 / AP-7 Jet Phaser | 1993 sighting, no song | — | drop |
| Boss MZ-2, GK 250ML, Super Champ, SD-1, CE-3 | board furniture / studio *Sorrow* chain | — | don't model |
| Talk box, E-Bow | define “Keep Talking” / “Pigs” but need a mic or a sustainer | — | say so, don't fake it |

## 4. The hard limit worth stating

Several famous “tones” are **direct-to-console**, with no amp and no cab:
the *Shine On* intro solo, *Another Brick in the Wall pt. 2*, and *Coming Back to
Life*. An amp+cab model cannot match them by construction; the closest is
compressor + EQ + console-ish saturation with the amp/cab bypassed. Put this in
the preset notes rather than pretending a Hiwatt preset will get there.

## 5. Amp and cab notes

- **Hiwatt DR103** is the right core amp and exists. The **WEM 4×12** exists and
  is voiced as Fane Crescendo — the key cab. Good.
- `CabModel::speaker_load` labels the WEM/Fane as **“Alnico”**. Fane Crescendos
  are **ceramic/ferrite** (confirmed). The voicing file describes the right
  sound, so this is a docs/parameter-label bug, not necessarily an audio bug —
  check before repeating it.
- Missing cabs that matter: **Fender Dual Showman 2×15 JBL** (WYWH) and
  **Hiwatt SE-4123** (87–94). The Marshall Greenback cab already covers the
  “Marshall cab” role; **do not** add a G12-65 cab (unsupported).
- Missing amps that matter: **Mesa Mark I** (About Face / Wall live, used as an
  overdrive), **Mesa Mark III** (87–90 live), **Hiwatt SA212** (recurring),
  **Hiwatt DR504** (2016). The Tweed Twin is the 2002 Meltdown amp — not
  trivia.

## 6. Chain-order experiments for the existing presets

Order is persisted per preset (`[chain] order`). Each row changes one thing.
Prefer **new `_alt` preset files** over overwriting the originals, so the
existing eight stay as the reference.

| Preset | Try | Why |
| --- | --- | --- |
| Numb 1 & 2 | `fuzz → tube_driver` vs the current `ts → fuzz` | 2006 Gdansk is Muff *into* Tube Driver; the current TS-before-Muff is empirical, not historical |
| Numb 1 & 2 | `comp → fuzz` vs `fuzz → comp` | compressing after a Muff tames the sustain; before it tightens the input |
| Numb 1 & 2 | rotary (RA-200 family) instead of chorus | the real studio modulation |
| Time (Solo) | `power_boost → fuzz` | the **1973** order was boost first; `fuzz → boost` is the 1975–77 order (the first pass had this inverted) |
| Time (Solo) | `comp → fuzz` vs `fuzz → comp` | a Fuzz Face cleans up with the guitar volume; a comp before it fights that |
| Time (Intro/Verse) | `uni_vibe → delay` vs `delay → uni_vibe` | vibe into echo smears the swirl; echo into vibe modulates the repeats |
| Time (Intro/Verse) | two delays (450 ms + 120 ms) | Gilmour used a slap-back and a long delay together on the intro |
| Mother (Solo) | `comp → fuzz`, no chorus | the solo is a mid-forward Muff into Hiwatt |
| All | `pre_eq` before vs after the drive | moving the pre-fuzz low cut after the fuzz changes which stage sees the mud |

New presets these additions make possible (name the arrangement):
**Shine On (Pulse)** — CS-2 + Tube Driver + 2290 + CE-2 + Doppola;
**Sorrow (1994 live)** — Sovtek Muff + Tube Driver + 2290 (the studio intro is
HM-2 + DD, not a Muff); **Echoes (seagull)** — reversed wah + delay;
**Run Like Hell (1994)** — CS-2 + CE-2 + Doppola + 380 ms; **(2016)** — comp +
Tube Driver boost + Mistress; **Dogs** — Ram's Head + Power Boost + RA-200;
**Marooned** — Tube Driver + Whammy + delay.

## 7. Build order (from the feasibility review)

The tree stays green after every phase; each phase is shippable.

0. **Decide** the two open UI/scope questions (below). No code.
1. **Compressor voice** (S) — smallest test of the “voice knob” pattern.
2. **Big Muff model** (S) — model 0 bit-identical; existing tests are the gate.
3. **ProCo RAT** (M) — first new stage; proves the plumbing.
4. **Tube Driver** (M) — same checklist; `DEVICE_YEARS` entry armed now.
5. **Rotary** (L) — only phase with new DSP risk; done last.
6. **Content** (separate PR) — chain-order A/Bs, new presets, docs.

Rules for every phase: new effects default **off** and are skipped before
`process`, so `fidelity_render -- --check` must pass **without a re-bless**.
A failure means the default path moved — fix the code. Only phase 6 may
re-bless, and only if a bundled preset's audio actually changes.
**Anachronism gate:** re-voicing the 1979 Numb presets onto the Tube Driver will
fail `no_new_device_anachronisms`; either keep those presets on TS/Muff or add a
documented `KNOWN_ANACHRONISMS` entry.
**Explicit-chain trap:** the two Numb presets have authored `[chain]` orders;
`sanitize_chain_order` appends a new stage **at the end**, so add `"rotary"`
after `"cab"` in both files in the rotary phase, or a user who switches it on
gets a post-reverb rotary.

Costs at a glance (Tier 1): 22 pedals / 98 knobs, one extra tile row only on
≤85-column terminals, 3 new chain stages, no paging needed.

## 8. Decisions (approved 2026-11-02)

1. **Scope:** all of Tier 1. Phases 1–5 shipped in order; content is phase 6.
2. **Muff UI:** a separate `MUFF` knob, labelled as Big Muff model only.
3. **Rotary:** RA-200/Doppola family in v1 (`SPEED`/`RAMP`/`DEPTH`/`MIX`/
   `BALANCE`); the Leslie horn/drum voice is a later addition.
4. **Presets:** the eight bundled presets stay untouched. Chain-order
   experiments and new-gear versions ship as new `_alt`/new-name files.
