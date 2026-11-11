# Scycocks

Offline configurator for several gaming mice, written in Rust with a native wxWidgets interface (through wxDragon). It talks to each mouse through the HID driver the OS already provides. No browser, account, background service, or vendor application is required.

Supported mice:

| Mouse | Connection | Device file |
|---|---|---|
| Scyrox V8 | Cable or receiver | [devices/scyrox-v8.toml](devices/scyrox-v8.toml) |
| MCHOSE L7 Pro, L7 Ultra+ | Cable, 8K or 1K receiver | [devices/mchose-l7.toml](devices/mchose-l7.toml) |

Each mouse is described by a TOML device file on top of a small Rust "family" that frames its reports. To add a mouse, see [docs/device-files.md](docs/device-files.md). Coding agents follow the step-by-step procedure in [AGENTS.md](AGENTS.md). A mouse that uses a supported family needs only a TOML file, and the app loads it from `%APPDATA%\scycocks\devices\` without a rebuild.

## Run

```powershell
cargo run --release
```

Or run `./target/release/scycocks.exe` after building (works in PowerShell and Git Bash). The exe is self-contained: wxWidgets, the C runtime and the built-in device files are linked in, so it needs no DLLs beyond those Windows ships.

```powershell
cargo build --release --locked
```

1. Close vendor apps and websites so they do not compete for the device.
2. Connect a mouse or receiver and wake the mouse.
3. Open the app. It connects to the first supported mouse and reads its settings. If several are connected, pick one in the **Mouse** list.
4. Edit settings. Select **APPLY** to save directly to the mouse. There is no confirmation step.

The app also reads settings after every save. If the mouse is absent or asleep, connection attempts retry every two seconds. While connected, it checks online status once per second. Removing the mouse or receiver therefore starts the retries again, and plugging one back in lets the next retry read it. Status checks do not touch pending edits. Reconnect and Load profile discard them. Connection attempts do not write settings.

## Controls

The window uses standard native controls: an Info box with the mouse picker, one tab per settings page of the connected mouse, and an APPLY button. The pages come from the device file, so each mouse shows only what it supports.

- **Scyrox V8:** four onboard profiles where the firmware supports them, polling rate limited by the connection, 0.7/1/2 mm lift-off, one to eight DPI stages (50 to 30,000 in steps of 50), Motion Sync, angle snapping, ripple control, Competitive Mode, 20K scanning (requires Competitive Mode), sleep time, angle tuning, debounce, receiver pairing, factory reset, and six button bindings.
- **MCHOSE L7 Pro / Ultra+:** three onboard profiles, polling rate (the 1K receiver stops at 1000 Hz, and wired and wireless keep separate rates), lift-off (0.7 mm on the Ultra+ only), one to six DPI stages with separate X/Y (to 26,000 or 42,000), angle snapping, ripple control, Motion Sync, Performance/Gaming/Extreme mode, sensor angle, debounce, sleep timer, factory reset, and five button bindings.

Keyboard shortcuts take one key plus modifiers, such as `Ctrl+C`, `Ctrl+Shift+A`, `Alt+F4`, `Win+D`, or `RCtrl+NumEnter`. Media keys are `VolumeUp`, `VolumeDown`, `Mute`, `PlayPause`, `Stop`, `NextTrack`, `PreviousTrack`, `BrowserBack`, `BrowserForward`, `BrowserHome`, `BrowserRefresh`, and `Calculator`.

**Load profile** switches immediately and discards pending edits. **Factory reset** clears stored settings; click once to arm it, then click **Click again to reset**.

To pair a Scyrox receiver, connect only the receiver. Hold the mouse's left, right, and middle buttons for three seconds until its light flashes. Keep it near the receiver and select **Pair receiver**. Pairing works while the mouse is offline.

Macros, firmware flashing, and MCHOSE profile names are excluded. Button bindings the app cannot decode are preserved unless you replace them. Scyrox V6 and PAW3955 models are rejected. Settings reads are verified on a Scyrox V8 and an MCHOSE L7 Pro; writes match the vendor implementations byte for byte in tests, but profile switching, pairing, and reset through this app still need hardware validation.

## Safety

Apply checks that no other program changed the settings, then writes only what changed, directly to the mouse. No local backup is created or required. Each write needs an acknowledgement, and the app reads all settings back afterwards and compares them. Scyrox writes are also read back one by one. At least one left-click binding must remain.

If a write or readback fails, some settings may already be saved. The app stops and disconnects its configuration session. Automatic connection retries pause so the error stays visible. Reconnect to inspect the current settings. The app does not retry writes or restore settings on its own.

## Diagnostics

These commands send only identity, status, and settings-read requests. Neither one writes settings:

```powershell
cargo run --no-default-features -- --list
cargo run --no-default-features -- --inspect
```

`--list` prints every HID interface that a device file matches. `--inspect` opens each one and prints its decoded settings. No arbitrary command or firmware-write interface is exposed.

## Development

Requires the stable Rust toolchain, the Windows MSVC linker and build tools, CMake, and LLVM. The first GUI build downloads the wxWidgets source and compiles it, which takes a few minutes. Put `cmake` on `PATH` or set `CMAKE` to `cmake.exe`. Visual Studio Build Tools ships a copy. Set `LIBCLANG_PATH` if LLVM's `libclang.dll` is not on `PATH`.

```powershell
winget install Kitware.CMake LLVM.LLVM
```

`build.rs` embeds a Windows manifest so the controls use the system theme and scale on high-DPI displays.

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Engine tests check the exact frames each device file produces against the original hand-written drivers. They need no hardware and run without compiling wxWidgets:

```powershell
cargo test --no-default-features --locked
```

Protocol notes: [docs/protocol.md](docs/protocol.md) (Scyrox: sources, artifact hashes, firmware findings, investigation limits) and [docs/protocol-mchose.md](docs/protocol-mchose.md) (MCHOSE, ported from the open-mchose project). Proprietary artifacts stay in the ignored `.local/` directory and are not part of the application.

## License

This project's source code is MIT licensed; see [LICENSE](LICENSE).

The GUI statically links wxWidgets under the [wxWindows Library Licence](https://www.wxwidgets.org/about/licence/). That licence permits distribution of binaries under the distributor's own terms. Other dependencies keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The MIT license does not grant rights to vendor firmware or artwork.
