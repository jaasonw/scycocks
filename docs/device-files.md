# Adding a mouse

A mouse has two parts:

1. **A family** (`src/family/<name>.rs`, Rust). It opens the HID interface and frames
   reports: report IDs, padding, checksums, XOR masks, and waits for replies or
   acknowledgements. It can also add codecs for encodings that data cannot express.
2. **A device file** (`devices/<name>.toml`, data). It lists the USB IDs, the settings
   layout, the commands, and how the GUI shows each setting.

If your mouse uses a protocol that a family already supports, write only the device file.
You do not need to rebuild the app. Put the file in `%APPDATA%\scycocks\devices\`
(`~/.config/scycocks/devices/` elsewhere). A file with the same name as a built-in file
replaces that built-in file. Problems in a file show in the status line at startup.

Families: `scyrox` (output report 8, `0x55` sum checksum), `mchose` (feature reports
`0x11`/`0x12`, XOR `0xFF`, `E5` acknowledgement) and `endgame` (feature reports
`0xA1`/`0xA0`, status byte with busy polling).

Use `scycocks --list` to show the devices that match. Use `scycocks --inspect` to read
every matched device and print its decoded settings. Neither command writes to the mouse.

## How the engine works

The engine reads the settings into one byte **buffer** (`size` bytes). Each field maps
to **locations** in that buffer. To apply edits, the engine encodes the new values into
a copy of the buffer and then writes the difference:

- If a `[[command]]` lists the field, the engine sends that command once.
- If not, the engine writes the changed byte spans with the `[write]` template.

Before it writes, the engine reads the buffer again. If something outside the app
changed the buffer, the write stops. After it writes, the engine reads the buffer again
and compares it with the expected bytes. Offsets in `volatile` are not compared.

The engine orders writes so that the mouse never holds an invalid state:

- A toggle that has `requires` and turns off is written first. If it turns on, it is
  written last.
- DPI stage values are written first. The active stage is always kept below the stage
  count.
- A button that becomes left click is written before the other buttons. A button's
  `extra` span is written before its main span.

## Top level

| Key | Meaning |
|---|---|
| `name` | Display name. A variant can change it. |
| `family` | Family name. |
| `size` | Buffer size in bytes. |
| `timeout_ms` | Reply timeout for each frame. Default 800. |
| `volatile` | Buffer offsets that the firmware changes without a command. |
| `zero` | Buffer offsets always written as 0 (bytes the vendor tool never sends back). |
| `[[match]]` | HID facts that must all match: `vid`, `pid`, `usage_page`, `usage`, `interface`, `product`. Each key takes one value or a list. Add more than one `[[match]]` to accept alternatives. |

## Frames and templates

A frame is a list of bytes and variables. The family adds the report framing. You can
also write `{ frame = [...], timeout_ms = 6000 }` for a slow command.

| Variable | Bytes |
|---|---|
| `addr16be`, `addr16le`, `len`, `data` | Memory address, length, and data in `[read]`/`[write]` |
| `buffer` | The whole encoded settings buffer, for devices that write everything at once |
| `profile` | Profile index in `[profiles].set` |
| `<field>` | Field value as one byte. Negative values use two's complement. |
| `<field>:flag12` | 1 if on, 2 if off |
| `<field>:nonzero` | 1 if the value is not 0, else 0 |
| `<field>:send` | The `send` value of the selected option |
| `<field>:u16le` | Value as two little-endian bytes |
| `<dpi>.count`, `<dpi>.active` | DPI stage count, active stage (0-based) |
| `<dpi>.x*`, `<dpi>.y*` | Encoded bytes of every stage slot, in order |
| `index`, `span[N]`, `span[N]:lo4` | In `per_button` commands: button index, byte N of the encoded binding, its low 4 bits |

## Identity and variants

```toml
[[identify]]                          # optional: ask the device who it is
frame = [0x01, 0, 0, 0, 8, 0x17, 0x39, 0x53, 0x71, 0, 0, 0, 0]
facts = { model = 10, connection = 11 }   # fact name = reply byte offset

[[require]]                           # refuse to open when facts do not match
when = { model = [1, 10, 11] }
error = "Only the V8 is supported."

[[variant]]                           # merged over the file when `when` matches
when = { pid = 0x1021 }
field = [{ id = "rate", limit = 3 }]  # list items with `id` merge by id
```

The engine merges matching variants in file order. Tables merge key by key. Lists of
items that have an `id` merge by `id`. Other values replace the old value.

## Reading, writing and device actions

```toml
[read]
frame = [0x08, 0, "addr16be", "len"]  # sent once, or once per `chunk` bytes
chunk = 10
data_at = 5                           # where the data starts in the reply

[write]                               # only for fields that no command covers
frame = [0x07, 0, "addr16be", "len", "data"]
chunk = 10
verify_each = true                    # read back each piece right after it is written

[online]                              # polled every second while connected
frame = [0x03, 0, 0, 0, 0]
byte = 5
equals = 1                            # without byte/equals, any reply counts as online
error = "The mouse is offline."

[[status]]                            # shown in the Info box
label = "Battery"
frame = [0x04, 0, 0, 0, 0]
format = "{5}%"                       # {N} decimal, {N:02x} hex, {N:str} length-prefixed text,
                                      # {N:u16le} u16, {N:u16le/100} u16 as a version (1.14)
charging = 6                          # optional: reply byte that is 1 while charging

[profiles]
count = 4
current = { frame = [0x0E, 0, 0, 0, 0], byte = 5 }   # or { offset = 0 } in the buffer
set = [0x0F, 0, 0, 0, 1, "profile"]

[reset]
frame = [0x09, 0, 0, 0, 0]
page = "Buttons"

[pair]                                # optional receiver pairing
page = "Buttons"
start = [0x05, 0, 0, 0, 2, 0, 0, 62]
poll = [0x06, 0, 0, 0, 0]
byte = 5
success = 3
failure = 2
timeout_s = 20
```

## Fields

Every field has `id`, `kind`, `label` and `page`. Pages appear as tabs in file order.
Optional keys for all kinds: `help` (a note under the control), `requires` (the id of a
toggle that must be on), and `optional` (an undecodable value reads as the default instead
of failing the read).

A **location** is `{ offset, stride = 0, mask, codec = "u8" }`. A `mask` selects bits
inside one byte, and the value is shifted down. `at` takes one location or a list; the
engine reads the first location and writes all of them (for mirrored bytes).

| Kind | Keys |
|---|---|
| `toggle` | `at`. Stored as 0 or 1. |
| `choice` | `at`, `options = [{ label, value, send? }]`, `limit` (offer only the first N options) |
| `range` | `at`, `min`, `max`, `unit`, `slider`, optional `options` (named special values outside `min`..`max`, shown as a mode list whose first entry, Off, uses the number) |
| `dpi` | `stages` (slots), `min`, `max`, `step`, `count`, `active`, `x`, optional `y`, `xy` (separate X/Y editing) |
| `buttons` | `names`, `at` (with `stride`), optional `extra` (second region per button), `actions`, `left_click` |

Button actions: `default`, `left`, `right`, `middle`, `back`, `forward`, `wheel_up`,
`wheel_down`, `dpi_cycle`, `dpi_up`, `dpi_down`, `rate_cycle`, `profile` (one entry per
onboard profile), `profile_cycle`, `disabled`, `key` (keyboard shortcut), `media`.
A binding that the codec cannot decode is kept unchanged.

## Codecs

Generic: `u8`, `i8`, `u16le`, `u16be`.
Scyrox: `scyrox_checked`, `scyrox_checked_i8` (value plus `0x55` checksum byte),
`scyrox_dpi` (X/Y pair), `scyrox_button` (binding with a 32-byte shortcut slot in `extra`).
MCHOSE: `mchose_button`.
Endgame Gear: `endgame_dpi` (5-byte X/Y stage record with an X≠Y flag), `endgame_button`.

## Adding a family

1. Create `src/family/<name>.rs` with a `pub const FAMILY: Family`. It needs:
   - `open`: returns a `Link` whose `exchange(frame, reply, timeout)` sends one frame and
     returns the reply payload.
   - `codec`: returns family-specific codecs by name, or `None`.
2. Add the module and the family to `FAMILIES` in `src/family/mod.rs`.
3. Add a device file. To ship it with the app, add it to `BUILTIN` in `src/driver/mod.rs`.
4. Add tests to `src/driver/tests.rs`. Build a snapshot from a captured buffer and assert
   the exact frames for a set of edits. The tests do not need hardware.
