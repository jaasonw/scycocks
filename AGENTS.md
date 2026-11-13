# Agent instructions

Scycocks configures gaming mice over HID. A mouse is a TOML device file (`devices/`) on
top of a Rust family (`src/family/`). The engine (`src/driver/`) and the GUI (`src/gui.rs`)
are generic. Do not add mouse-specific code to the engine or the GUI.

The device-file format is in `docs/device-files.md`. Read it before you change a device file.

## Words used here

- **Frame:** the bytes of one HID report that the app sends, before the family adds
  framing.
- **Buffer:** the settings bytes that the engine reads from the mouse. Fields point to
  offsets in this buffer.
- **Family:** Rust code that sends frames and checks replies for one protocol. One family
  can serve many mice.
- **Device file:** a TOML file that describes one mouse: USB IDs, buffer layout, fields,
  and commands.
- **Codec:** a function that converts between buffer bytes and a field value.
- **Gate:** the four commands in "Gate" below. They must all pass.

## Add a mouse

Do the steps in order. Do not skip a step. Each step ends with a **Done when** check. Do
not start the next step until the check is true. When a step says **Stop and ask**, send
the question to the user and wait for the answer.

### Step 0. Prepare

1. Read these files completely:
   - `docs/device-files.md` (the device-file format).
   - One existing device file that is similar to your mouse:
     - `devices/scyrox-v8.toml`: memory-address writes, one setting per write.
     - `devices/mchose-l7.toml`: one command per setting, variants for models and
       connections.
     - `devices/endgame-op1-8k-v2.toml`: the whole settings blob is written at once.
   - The family for that file in `src/family/`.
   - The protocol note for that family in `docs/` (`protocol.md` is Scyrox).
2. Run the fast tests to make sure that the tree is green before you change it:
   `cargo test --no-default-features --locked`.
3. Find out what the user gave you as the protocol source: a vendor app, a web driver,
   USB captures, or an open-source tool. If you do not know, **stop and ask**.

**Done when:** you have read the files, the tests pass, and you know the protocol source.

### Step 1. Get the protocol

Write the protocol note `docs/protocol-<vendor>.md`. Use `docs/protocol-endgame.md` as the
model. Each fact in the note must come from the source, with a pointer to where you found
it (function address, file and line, or capture number). If you cannot find a fact, write
"unknown". Do not guess.

The note must answer all of these questions:

| Question | Example answer |
|---|---|
| USB vendor ID and product IDs (all models and connections) | `0x3367`, `0x1978` |
| Which HID interface: usage page and usage | usage page `0xFF01`, usage `2` |
| Report type: output report, input report, or feature report | feature reports |
| Report IDs and report sizes | `0xA1` 64 bytes, `0xA0` 1041 bytes |
| Frame layout: header bytes, command byte, length, payload | `[A1, cmd, 0…]` |
| Checksum or XOR, and which bytes it covers | none |
| How a reply is recognized, and what a busy or error reply looks like | byte 1: `1` done, `3` busy |
| Delays and timeouts that the vendor code uses | 300 ms after a blob write |
| How to read all settings | cmd `0x12`, data at offset `0x10` |
| How to write each setting | whole blob with cmd `0x11` |
| For each setting: offset, size, encoding, allowed values, and label in the vendor UI | `0x05` polling rate, `0x40` = 125 Hz |
| Commands that the vendor app has but this app must not send | firmware update, `0x13` |

Keep vendor binaries, bundles, and captures in `.local/` or `reference/`. Git ignores both
folders. Never commit them.

**For a native vendor app (`.exe`):** decompile it with Ghidra headless:

```sh
analyzeHeadless <work-dir> proj -import <exe> -postScript DumpAll.java <out.c> -scriptPath scripts/ghidra -deleteProject
```

Then search `<out.c>`:

1. Find the calls to `HidD_SetFeature`, `HidD_GetFeature`, `WriteFile` or `ReadFile`. The
   functions around these calls build and check the frames.
2. Follow the callers up to the code that fills the frame from a settings structure.
3. Follow the settings structure into the dialog code. In an MFC app, `DDX_Control` calls
   map structure member offsets to control IDs.
4. Read the dialog and `DLGINIT` resources of the exe. They map control IDs to labels and
   to the items of each combo box.

**For a web driver:** save the JavaScript bundle in `.local/` and search it for `sendReport`,
`sendFeatureReport`, `receiveFeatureReport`, and `oninputreport`.

**Done when:** the note answers every question in the table, or says "unknown" with a
reason. **Stop and ask** if you cannot find how settings are read, or how they are written.
Without both, the mouse cannot be supported.

### Step 2. Choose the family

Compare your note with each family in `src/family/`:

| Family | Transport | Check |
|---|---|---|
| `scyrox` | output report 8, 17 bytes | `0x55` sum checksum |
| `mchose` | feature reports `0x11`/`0x12` | XOR `0xFF`, `E5` acknowledgement |
| `endgame` | feature reports `0xA1`/`0xA0` | status byte with busy polling |

A family matches only if **all** of these are the same: report type, report IDs, report
sizes, checksum, and how a reply is recognized. The commands and the settings layout can
be different; the device file describes those.

- If a family matches: use it. Go to step 4.
- If no family matches: go to step 3.
- If a family almost matches (for example, the same framing but a different report size):
  **stop and ask**. Do not change a family that other mice use without approval.

**Done when:** you know which family to use, or you know that you need a new one.

### Step 3. Add a family (only if no family matches)

1. Copy the structure of the closest family. Create `src/family/<name>.rs`. Start the file
   with a `//!` comment that names the reports and points to the protocol note.
2. Define `pub const FAMILY: Family` with:
   - `name`: the name that device files use in `family = "<name>"`.
   - `open`: opens the HID device and returns a `Box<dyn Link>`.
   - `codec`: returns the family's own codecs by name, or `None`.
3. Implement `Link::exchange(frame, reply, timeout)`:
   - Add the framing: report ID, padding to the report size, checksum.
   - Send the report.
   - If `reply` is true, read the reply and return its payload.
   - Reject a reply when its report ID, command, header, or checksum does not match the
     request. Return an error message. Never return a wrong reply as valid.
   - Use the delays from the protocol note. Put the reason in a comment.
4. Add codecs only if a setting cannot be expressed with `offset`, `mask`, and a generic
   codec (`u8`, `i8`, `u16le`, `u16be`). Examples that need a codec: a value with a checksum
   byte, an X/Y DPI record, a button binding.
   - A button codec must decode unknown bindings to `Action::Raw` and encode
     `Action::Raw` back to the same bytes. A binding is then never lost.
   - A decode error must be an error. Never return a default value instead.
5. Register the module and the family in `src/family/mod.rs`: add `mod <name>;` and add
   `<name>::FAMILY` to `FAMILIES`.
6. Add unit tests for each codec at the end of the family file, in a `#[cfg(test)]`
   module:
   - Round trip: encode, decode, and get the same value.
   - Boundary values: minimum, maximum, and one step past each.
   - Corrupt input: a wrong checksum or an unknown type decodes to an error or to
     `Action::Raw`.
7. Run `cargo test --no-default-features --locked`.

**Done when:** the family compiles, it is in `FAMILIES`, and its tests pass.

### Step 4. Write the device file

Create `devices/<vendor>-<model>.toml`. Write it in this order:

1. **Top level:** `name`, `family`, `size` (buffer size in bytes), and `timeout_ms` if the
   default of 800 ms is too short.
2. **`[[match]]`:** as narrow as possible. Always give `vid`, `pid`, and `usage_page`.
   Add `usage` if the mouse has more than one vendor collection. A wrong match sends
   frames to the wrong device.
3. **`[[identify]]` and `[[require]]`:** use them if the protocol can report a model ID.
   They refuse unsupported models before any write.
4. **`[read]`, `[write]`, `[online]`, `[[status]]`:** copy the frames from the protocol
   note.
5. **`[[variant]]`:** use a variant for each model or connection difference. Do not copy
   the file for a second model.
6. **`[[field]]`:** one per setting. For each field:
   - Use the same label as the vendor UI.
   - Put it on the same page (tab) as the vendor UI where possible.
   - Give `min`, `max`, `step`, and `options` exactly as the vendor UI allows. Do not
     allow a value that the vendor UI does not allow.
   - Add `requires` if the vendor UI disables the setting while a toggle is off.
   - Add `optional = true` only if the vendor app also accepts an invalid stored value.
   - Add `help` only for a fact that the user needs, such as a side effect.
7. **`volatile`:** offsets that the firmware changes without a command (for example a
   counter). The readback check skips them.
8. **`zero`:** offsets that the vendor tool always writes as 0.
9. **`[[command]]`:** if the protocol writes settings with commands instead of memory
   writes, add one command per group and list the fields it covers.
10. **`[profiles]`, `[reset]`, `[pair]`:** only if the protocol source documents them.

Settings that the vendor UI hides, or that you do not understand: leave them out of the
file. The engine copies bytes that no field covers back unchanged.

Validate the file without a build: put a copy in `%APPDATA%\scycocks\devices\` and run
`cargo run --no-default-features -- --list`. A broken file prints "Skipped device file"
with the reason.

**Done when:** the file loads with no "Skipped" message, and every setting in the protocol
note is either a field or listed in the note as left out, with the reason.

### Step 5. Register the built-in file

1. Add the file to `BUILTIN` in `src/driver/mod.rs`, in the same form as the other
   entries.
2. Add a row to the "Supported mice" table in `README.md`, and list the settings.
3. Add the family to the families list in `docs/device-files.md` if it is new, and add
   any new codecs to the "Codecs" section.

**Done when:** `cargo test --no-default-features --locked` passes. The test
`shipped_files_and_every_variant_parse` now also checks your file.

### Step 6. Add engine tests

Add tests at the end of `src/driver/tests.rs`. Use the OP1 tests (`op1()`,
`op1_decodes_vendor_layout`, `op1_writes_whole_blob_like_the_vendor_tool`) as the model.

1. Write a helper `fn <model>() -> (Device, Snapshot)`:
   - Call `device("<file>.toml", &[facts])` to load the file.
   - Build the buffer. Use bytes captured from a real mouse if you have them. If not,
     set each byte by hand with a comment that gives the value.
   - Call `snapshot(&device, buffer)`.
2. Write a decode test. Assert the decoded value of each kind of field. The values have
   these types:

   | Field kind | Value |
   |---|---|
   | `toggle` | `Value::Bool` |
   | `choice`, `range` | `Value::Int` |
   | `dpi` | `Value::Dpi { count, active, stages }` |
   | `buttons` | `Value::Buttons(Vec<Action>)` |

   Also assert that unchanged values produce no frames:
   `assert!(frames(&device, &old, &old.values).unwrap().is_empty());`
3. Write a write test. Change one field of each kind with `set(...)`. Call
   `frames(...)`. Assert the exact bytes of each frame.
4. Write a test for each variant: load the file with the variant's facts and assert the
   difference.
5. Write a test for each rejected input, such as a value out of range or a removed left
   click. Assert that `device.frames(...)` returns an error.

The expected bytes must come from the protocol source: vendor code, captures, or the
protocol note. Never copy the engine's output into the test as the expected value. Such a
test passes even if the bytes are wrong.

**Done when:** the new tests pass and fail when you change one expected byte. (Change it,
run, see the failure, change it back.)

### Step 7. Run the gate

Run all four commands in "Gate" below. Fix every error and warning. Do not add `allow`
attributes to silence a warning unless the reason is in a comment next to it.

**Done when:** all four commands pass.

### Step 8. Check against hardware, read-only

**Stop and ask** the user to connect the mouse and to tell you when it is connected. Then
run:

```sh
cargo run --no-default-features -- --list
cargo run --no-default-features -- --inspect
```

These commands send only identify, status, and read requests. They never write.

1. `--list` must show the mouse once, with your device file name. If the mouse does not
   show, check `[[match]]`. If it shows more than once, make `[[match]]` narrower.
2. `--inspect` must print every field with a plausible value.
3. Ask the user to open the vendor app and compare the values. Each value must be the
   same.

**Done when:** the user confirms that the values match the vendor app.

### Step 9. Ask before any write to hardware

Apply, profile switch, factory reset, and pairing change the mouse. Never run them
yourself. Give the user a test plan:

1. One safe change first, such as an LED setting or the polling rate. Name the field and
   the value.
2. The expected result: the status line shows no error, and the vendor app shows the new
   value.
3. Then one change of each field kind (DPI, a toggle, a button binding).
4. How to undo: set the old value again, or use the vendor app.

**Done when:** the user reports the result. If a write fails, fix the device file or the
family, run the gate again, and repeat this step.

### Step 10. Report

Tell the user:

- The files you added or changed.
- The settings that are supported, and the settings left out with the reason.
- What was checked on hardware, and what was not.
- A suggested commit message. Do not commit unless the user asks.

## Common mistakes

- **A `[[match]]` that is too wide.** Without `usage_page`, the app can open the mouse's
  keyboard or mouse interface and send it vendor frames.
- **Expected bytes copied from the engine.** The test then checks nothing.
- **Guessed values.** If the source does not show what a byte means, leave the byte out.
  The engine keeps unmapped bytes.
- **A wider range than the vendor UI.** The firmware can accept a value that the vendor
  never sends and behave in an unknown way.
- **Mouse-specific code in the engine or the GUI.** Add a generic feature, or put the code
  in the family.
- **Line endings.** Keep LF. Some editing tools on Windows write CRLF. Check with
  `git diff --stat`: a small change must not show as a whole-file change.

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

If the release build fails with "failed to remove file … scycocks.exe", the app is still
open. Ask the user to close it. Do not stop the process yourself.

## Style

- Match the existing code. Comment only the reason for non-obvious code.
- Keep dependencies to a minimum. Do not add a crate for something a few lines can do.
- Write docs and error messages in short, plain sentences.
- Do not commit unless the user asks. Use Conventional Commits.
