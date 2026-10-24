# Scycocks

Offline Windows configurator for the Scyrox V8, written in Rust and Slint. It communicates through Windows' existing HID driver. No browser, account, background service, or vendor application is required.

## Run

```powershell
cargo run --release
```

Or run `./target/release/scycocks.exe` after building (works in PowerShell and Git Bash):

```powershell
cargo build --release --locked
```

1. Close the Scyrox website so it does not compete for the device.
2. Connect one V8 or receiver. Wake the mouse. If both cable and receiver are attached, leave only one connected.
3. Open the app. It connects and reads settings automatically. If the mouse is absent or asleep, it retries every two seconds.
4. Edit settings. Select **APPLY** to save directly to the mouse. There is no confirmation step.

The app reads settings automatically on startup and after saving. Failed connection attempts retry every two seconds until settings are read successfully. While connected, the app checks the mouse's online status once per second. Removing the active mouse or receiver starts automatic connection retries. Connecting a supported mouse or receiver lets the next retry read its settings. These status checks do not refresh over pending edits. Reconnect reads current settings again and discards pending edits. Connection attempts do not write settings.

## Controls

The layout follows a classic Windows utility: an Info box, four tabs, and an APPLY button.

- **Basic:** profiles, polling rate, lift-off distance, DPI stage count, active stage, and DPI sliders with numeric inputs.
- **Advanced Sensor:** Motion Sync, angle snapping, ripple control, Competitive Mode, 20K scanning, sleep time, and angle tuning.
- **Buttons:** global debounce, receiver pairing, and factory reset. Endgame Gear's per-button multiclick filters are not Scyrox debounce controls.
- **Button Mapping:** mouse actions, keyboard shortcuts, and media keys for six buttons.

Supported settings:

- One to eight DPI stages, active stage, and each stage's DPI, from 50 to 30,000 in steps of 50. Newly enabled stages start at 800 DPI.
- Polling rate, limited by the connected mouse/receiver.
- Basic button mapping: mouse clicks, Back/Forward, DPI/rate cycling, and disabled buttons.
- Debounce, 0.7/1/2 mm lift-off distance, Motion Sync, angle snapping, and ripple control.
- Competitive Mode and 20K sensor scanning. 20K requires Competitive Mode. Both increase power consumption.
- Sleep time from 10 seconds to 30 minutes. Angle tuning from -30 to +30 degrees, with an enable toggle.
- Four on-device profiles, when supported by the connected firmware.
- Keyboard shortcuts with one key and up to four modifiers. Examples: `Ctrl+C`, `Ctrl+Shift+A`, `Alt+F4`, `Win+D`, `RCtrl+NumEnter`.
- Media keys: `VolumeUp`, `VolumeDown`, `Mute`, `PlayPause`, `Stop`, `NextTrack`, `PreviousTrack`, `BrowserBack`, `BrowserForward`, `BrowserHome`, `BrowserRefresh`, `Calculator`.
- Mouse firmware version and battery status.

**Load profile** switches immediately and discards pending edits. It does not save them first. Unsupported profile switching is disabled.

To pair, connect only the wireless receiver. Hold the mouse's left, right, and middle buttons for three seconds until its light flashes. Keep it near the receiver and select **Pair receiver**. Pairing waits up to 20 seconds. It also works when the receiver is recognized but the mouse is offline.

**Factory reset** clears stored device settings. Click once to arm it, then click **Click again to reset**. This safeguard applies only to factory reset, not APPLY.

Only V8 company/model identities are enabled. V6 and PAW3955 models are rejected. Macros, independent X/Y DPI, and firmware flashing remain excluded. Unsupported existing button actions are preserved unless you replace them. New write commands match the vendor web implementation, but settings persistence, profile switching, pairing, and reset still require hardware validation. Automated tests do not send these commands.

## Safety

Apply checks that settings did not change outside this app, then writes modified setting blocks directly to the mouse. No local backup is created or required. Each write requires acknowledgement and matching readback. At least one left-click binding must remain.

If a write or readback fails, some settings may already be saved. The app stops and disconnects its configuration session. Automatic connection retries pause so the error remains visible. Reconnect to inspect current settings. The app does not retry writes or restore settings automatically.

## Diagnostics

These commands issue identity/status/configuration-read requests, not setting writes:

```powershell
cargo run --no-default-features -- --list
cargo run --no-default-features -- --inspect
```

`--list` prints HID interfaces and report descriptors. `--inspect` reads the first supported V8 configuration interface. No arbitrary command or firmware-write interface is exposed.

## Development

Requires the stable Rust toolchain and the Windows MSVC linker/build tools.

`.cargo/config.toml` selects Slint's `fluent-light` widget style. The default style produced invisible controls with the software renderer on the tested Windows system.

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The desktop smoke test opens the real window, clicks through all four tabs, checks the connection button and single-click APPLY action, and saves rendered images under `.local/research/`. It prints the connection status. The APPLY check replaces the save callback, so the test does not write mouse settings:

```powershell
cargo test --locked window_renders_controls -- --ignored --nocapture
```

Hardware-independent packet/settings tests can run without compiling Slint:

```powershell
cargo test --no-default-features --locked
```

Protocol sources, artifact hashes, firmware findings, and investigation limits are in [docs/protocol.md](docs/protocol.md). Proprietary artifacts remain under the ignored `.local/` directory during investigation and are not part of the application.

## License

Project code: MIT. Slint and other dependencies retain their own licenses. Review [Slint's license options](https://slint.dev/license/) before distributing a binary. The MIT license does not grant rights to vendor firmware or artwork.
