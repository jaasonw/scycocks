# Scycocks

Offline Windows configurator for the Scyrox V8, written in Rust with a Slint interface. It talks to the mouse through the HID driver Windows already provides. No browser, account, background service, or vendor application is required.

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
3. Open the app. It connects and reads settings automatically.
4. Edit settings. Select **APPLY** to save directly to the mouse. There is no confirmation step.

The app also reads settings after every save. If the mouse is absent or asleep, connection attempts retry every two seconds. While connected, it checks online status once per second. Removing the mouse or receiver therefore starts the retries again, and plugging one back in lets the next retry read it. Status checks do not touch pending edits. Reconnect and Load profile discard them. Connection attempts do not write settings.

## Controls

The layout follows a classic Windows utility: an Info box, four tabs, and an APPLY button.

- **Basic:** four on-device profiles where the firmware supports them, polling rate limited by the connected device, 0.7/1/2 mm lift-off distance, one to eight DPI stages from 50 to 30,000 in steps of 50, the active stage, and a slider per stage with a numeric input. New stages start at 800 DPI.
- **Advanced Sensor:** Motion Sync, angle snapping, ripple control, Competitive Mode, 20K scanning, sleep time, and angle tuning.
- **Buttons:** global debounce, receiver pairing, and factory reset. Endgame Gear's per-button multiclick filters are not Scyrox debounce controls.
- **Button Mapping:** mouse clicks, Back/Forward, DPI and rate cycling, disabled, keyboard shortcuts, and media keys for six buttons.

20K scanning requires Competitive Mode. Both settings raise power draw. Sleep time runs from 10 seconds to 30 minutes. Angle tuning runs from -30 to +30 degrees and has its own enable toggle.

Keyboard shortcuts take one key plus up to four modifiers, such as `Ctrl+C`, `Ctrl+Shift+A`, `Alt+F4`, `Win+D`, or `RCtrl+NumEnter`. Media keys are `VolumeUp`, `VolumeDown`, `Mute`, `PlayPause`, `Stop`, `NextTrack`, `PreviousTrack`, `BrowserBack`, `BrowserForward`, `BrowserHome`, `BrowserRefresh`, and `Calculator`.

The Info box shows the mouse firmware version and battery status.

**Load profile** switches immediately and discards pending edits without saving them. Profile switching is disabled when the firmware does not support it.

To pair, connect only the wireless receiver. Hold the mouse's left, right, and middle buttons for three seconds until its light flashes. Keep it near the receiver and select **Pair receiver**. Pairing waits up to 20 seconds and also works when the receiver is recognized but the mouse is offline.

**Factory reset** clears stored device settings. Click once to arm it, then click **Click again to reset**. This safeguard applies only to factory reset, not to APPLY.

Only V8 company and model identities are enabled. V6 and PAW3955 models are rejected. Macros, independent X/Y DPI, and firmware flashing are excluded. Unsupported existing button actions are preserved unless you replace them. The write commands match the vendor web implementation, but settings persistence, profile switching, pairing, and reset still need hardware validation. Automated tests do not send these commands.

## Safety

Apply checks that no other program changed the settings, then writes the modified blocks directly to the mouse. No local backup is created or required. Each write needs an acknowledgement and a matching readback. At least one left-click binding must remain.

If a write or readback fails, some settings may already be saved. The app stops and disconnects its configuration session. Automatic connection retries pause so the error stays visible. Reconnect to inspect the current settings. The app does not retry writes or restore settings on its own.

## Diagnostics

These commands send identity, status, and configuration-read requests. Neither one writes settings:

```powershell
cargo run --no-default-features -- --list
cargo run --no-default-features -- --inspect
```

`--list` prints HID interfaces and report descriptors. `--inspect` reads the first supported V8 configuration interface. No arbitrary command or firmware-write interface is exposed.

## Development

Requires the stable Rust toolchain and the Windows MSVC linker and build tools.

`.cargo/config.toml` selects Slint's `fluent-light` widget style. With the default style, the software renderer drew invisible controls on the tested Windows system.

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The desktop smoke test opens the real window, clicks through all four tabs, checks the connection button and the single-click APPLY action, and saves rendered images under `.local/research/`. It prints the connection status. The APPLY check replaces the save callback, so the test does not write mouse settings:

```powershell
cargo test --locked window_renders_controls -- --ignored --nocapture
```

Hardware-independent packet and settings tests run without compiling Slint:

```powershell
cargo test --no-default-features --locked
```

Protocol sources, artifact hashes, firmware findings, and investigation limits are in [docs/protocol.md](docs/protocol.md). Proprietary artifacts stay in the ignored `.local/` directory and are not part of the application.

## License

This project's source code is MIT licensed; see [LICENSE](LICENSE).

The app uses Slint under the [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md), which requires attribution. That requirement is met by the badge below, per section 2(b) of the license. The MIT license covers this project's own code. Distributed binaries also carry Slint's license terms. Other dependencies keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The MIT license does not grant rights to vendor firmware or artwork.

[<img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" width="120" alt="Built with Slint">](https://slint.dev)
