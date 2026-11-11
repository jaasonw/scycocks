# Agent instructions

Scycocks configures gaming mice over HID. A mouse is a TOML device file (`devices/`) on
top of a Rust family (`src/family/`). The engine (`src/driver/`) and the GUI (`src/gui.rs`)
are generic. Do not add mouse-specific code to the engine or the GUI.

The device-file format is in `docs/device-files.md`. Read it before you change a device file.

## Add a mouse

Do these steps in order. Stop and ask the user when a step says so.

1. **Get the protocol.** Use a source that the user gives: a vendor web driver, a
   vendor app, USB captures, or an existing open-source tool. Write down the report IDs,
   the frame layout, the checksum, how settings are read, and how each setting is written.
   Put the notes in `docs/protocol-<vendor>.md`. Do not commit proprietary binaries or
   bundles; keep them in the ignored `.local/` directory.
2. **Choose the family.** Compare the framing with `src/family/scyrox.rs` and
   `src/family/mchose.rs`. If one of them matches, use it and go to step 4.
3. **Add a family only if no family matches.** Create `src/family/<name>.rs` with a
   `pub const FAMILY: Family` and register it in `FAMILIES` in `src/family/mod.rs`.
   - `Link::exchange` frames one request, sends it, and returns the reply payload. It must
     reject replies that do not match the request (report ID, command, header, checksum).
   - Add a codec only for an encoding that a location with `offset`, `mask` and a generic
     codec cannot express. Keep unknown button bindings as `Action::Raw` so they are
     never lost.
   - Add unit tests for every codec: round trip, boundary values, and corrupt input.
4. **Write the device file** in `devices/<vendor>-<model>.toml`.
   - Add `[[match]]` rules that are as narrow as possible: `vid`, `pid`, and `usage_page`
     (and `usage` when the device has several vendor collections).
   - Use `[[identify]]` and `[[require]]` when the protocol can report a model ID, so
     that unsupported models are refused before any write.
   - Use `[[variant]]` for model and connection differences. Do not copy the file.
   - Use `optional = true` only when the vendor app also accepts the invalid value.
   - Use `requires` for settings that depend on a toggle.
   - Put shared, firmware-maintained bytes in `volatile`.
5. **Register a built-in file** (optional) in `BUILTIN` in `src/driver/mod.rs`, and add a
   row to the "Supported mice" table in `README.md`.
6. **Add engine tests** in `src/driver/tests.rs`. Use a settings buffer captured from a
   real mouse when one is available; if not, build one with `Device::encode`. Assert the
   decoded values, assert that unchanged values produce no frames, and assert the exact
   frame bytes for each kind of edit. The expected bytes must come from the protocol source
   (vendor code or captures), not from the output of the engine.
7. **Run the gate** (see below). All checks must pass.
8. **Check against hardware, read-only.** Ask the user to connect the mouse. Run
   `scycocks --list`, then `scycocks --inspect`. These commands send only identify,
   status and read requests. Compare the printed values with the vendor app.
9. **Ask the user before any write to hardware.** Apply, profile switch, factory reset
   and pairing change the mouse. Do not run them yourself. Tell the user what to test in
   the GUI and what result to expect.

## Safety rules

- Never add firmware flashing, raw command consoles, or bootloader commands.
- Never send a command that the protocol source does not document.
- Never relax a check in the engine to make a device file pass. Fix the device file or
  the family instead.
- A decode error must stop the read. Do not guess values for settings that will be
  written back.
- Keep at least one left click (`left_click = true` on every `buttons` field).

## Gate

Run these commands before you report work as done:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo clippy --no-default-features --all-targets --locked -- -D warnings
cargo test --locked
```

`cargo test --no-default-features --locked` runs the same tests without compiling
wxWidgets. Use it for fast iteration.

The GUI build needs CMake and LLVM. If they are not on `PATH`, set `CMAKE` to `cmake.exe`
(Visual Studio Build Tools includes one) and `LIBCLANG_PATH` to the folder that contains
`libclang.dll`.

## Style

- Match the existing code. Comment only the reason for non-obvious code.
- Keep dependencies to a minimum. Do not add a crate for something a few lines can do.
- Write docs and error messages in short, plain sentences.
- Do not commit unless the user asks. Use Conventional Commits.
