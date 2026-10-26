# 🎸 headroom

A guitar amp and pedalboard rig that runs in your terminal. Keyboard- and
mouse-driven, with live metering and artist-inspired presets.

See the [user guide](docs/guide.md) for a walkthrough of calibration, the signal
chain, presets, practice, and plugins.

## Requirements

- Rust 1.96+
- A guitar and an audio interface with a high-impedance instrument input
- Speakers or headphones

## Build & run

```bash
git clone https://github.com/cesaraaron/headroom
cd headroom
cargo run --release
```

Use `--release`: a debug build can underrun the audio callback. On first launch
the app asks for your input device, input channel, and output device.

Panels are transparent by default, so terminal transparency (e.g. Ghostty)
shows through. Pass `--opaque` for solid black panel backgrounds:

```bash
cargo run --release -- --opaque
```

## Essential keys

| Key | Action |
| --- | --- |
| `K` | Full in-app key reference |
| `A` / `C` | Amp / cabinet browser |
| `P` | Preset browser |
| `↑` / `↓` or `+` / `-` | Select a knob / adjust its value |
| `Space` | Bypass the focused stage |
| `[` / `]` | Move the selected stage earlier or later |
| `1`–`4` | Focus the chain, amp, timeline, or pedals |
| `Shift+1`–`4` | Solo a panel: close the others, keep just that one |
| `R` | Record / stop a dry take |
| `/` | Search the preset, session, or import list |
| `L` | Import library settings (path, only, subpaths) |
| `N` | Calibrate the input level |
| `F2` | MIDI bindings (learn a controller) |
| `F4` / `Ctrl+G` | Toggle the mains-hum floor (live A/B) |
| `Q` | Quit |

## Mouse

The mouse is a second way to move focus, never a way to edit a value:

- Click a panel to focus it (a hidden panel is shown).
- Click a knob, a pedal tile, or a knob in the pedal detail editor to focus it;
  click a chain-ribbon stage or the `MASTER` cell to select it.
- In the timeline, click a track row to select it, or the ruler / waveform to
  move the playhead.

Clicks are ignored while a browser or dialog is open. Mouse capture stays on for
the whole run, so normal click-drag text selection is disabled — hold **Shift**
(some terminals use **Option**) to select text.

## Input level

Amp breakup depends on how hot your interface drives the input, so calibrate
once per guitar/interface. Press `N`, set the interface gain as low as
practical, pick the pickup class, and follow the prompts. The header shows
`IN uncal` until you calibrate and the applied trim after (e.g. `IN +7.5`).
Recalibrate with `N` if you change the interface gain, guitar, or pickups. The
trim is applied to the dry input before the noise gate, so the tuner, meters,
recordings, and every effect see the calibrated signal.

## Where things live

- Presets: `~/.config/headroom/presets/`
- External IRs: `~/.config/headroom/irs/`

## License

Apache 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
