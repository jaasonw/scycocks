# MCHOSE L7 Pro / L7 Ultra+ mouse protocol

Reverse-engineered from the vendor's hosted web driver (`https://www.mchose.com.cn/`, the
mouse code lives in the large `assets/purify.es-*.js` chunk: read table `j3`, write table
`dst`, device capabilities `km`). It was verified on an L7 Pro, firmware `5.12.5.0`, over the cable and
through the 8K receiver (firmware `5.7.6.0`).

## Device

| | |
|---|---|
| Wired | L7 Pro: VID `0x5253`, PID `0x00B0`, product string `MCHOSE L7 Pro`. L7 Ultra+: VID `0x3837`, PID `0x4016`, product string `MCHOSE L7 Ultra+` |
| 2.4G receivers | VID `0x5253`, PID `0x1020` (8K, tested) / `0x1021` (1K, up to 1000 Hz); product string of the paired mouse (`MCHOSE L7 Pro` or `MCHOSE L7 Ultra+`), the only way to tell them apart |
| Vendor interface | interface 2, collection 2: usage page `0xFF01`, usage `0x01` |
| Sensor | L7 Pro: 26000 DPI max, LOD 1 / 2 mm. L7 Ultra+: 42000 DPI max, LOD 0.7 / 1 / 2 mm (vendor table `Em`) |
| Onboard profiles | 3 |

## Transport

All requests are HID **feature reports** on the vendor collection.

| Report ID | Payload | Used for |
|---|---|---|
| `0x11` | 20 bytes | short commands |
| `0x12` | 64 bytes | long commands |

- Payload byte 0 is the command. Unused bytes are `0x00`.
- **Every payload byte is sent XOR `0xFF`** (so padding goes out as `0xFF`). Replies are XOR `0xFF` too.
- Every command, read or write, is acknowledged with an input report on the same collection: `E5 <cmd>`
  (after XOR, report ID `0x13` in front on Windows).
- **Read:** send the command, wait for the ack, then get the feature report with the same ID. Reply byte 0 is
  `~cmd`; the data follows. Through the receiver the ack only comes once the mouse has answered over the air;
  a get before the ack returns zeros.
- **Write:** send the command and wait for the ack.
- The receiver uses the same commands and relays them; `0x03` and `04 AA` are answered by the receiver
  itself, so `0x03` is a cheap way to see whether the mouse is awake.
- Input report `E2 ...` is sent on its own when DPI, battery or the onboard profile changes.

## Read commands

| Cmd | Report | Request | Reply |
|---|---|---|---|
| `0x03` | `0x11` | `03` | receiver: `bond, vid u16, pid u16, linked (1 = mouse awake), game` |
| `0x04` | `0x11` | `04` | `len, ascii[len]` mouse firmware, e.g. `5.12.5.0` |
| `0x04` | `0x11` | `04 AA` | receiver firmware, e.g. `5.7.6.0` |
| `0x06` | `0x11` | `06` | `vid u16, pid u16, fw u32, mode/status bits, battery %, charging` |
| `0x67` | `0x12` | `67` | settings of the active profile, see below |
| `0x68` | `0x12` | `68 <profile 0-2>` | `profile, utf8 name, 0…` |
| `0x63` | `0x12` | `63 <button>` | `button, len, utf8 macro name` |
| `0x65` / `0x77` | `0x12` | `65 01 <0/1>` | macro data pages |

## Settings block (`0x67` reply)

Multi-byte numbers are little-endian. **Bit fields are LSB-first**: the first field the vendor parser
names sits in the low nibble.

| Offset | Field |
|---|---|
| 0 | active profile (0-2) |
| 1 | wired: low nibble DPI stage, high nibble polling-rate index |
| 2 | 2.4G: same layout |
| 3 | current DPI stage (mirror) |
| 4–15 | X DPI of stages 1–6, `u16` |
| 16 | number of enabled stages (1–6) |
| 17 | sensor bits, see below |
| 18 | button debounce, ms (0–20) |
| 19 | 2.4G sleep timer, minutes (0 = never) |
| 20–43 | 6 buttons × 4 bytes: `index << 4 \| type`, `value[3]` |
| 44–48 | reserved |
| 49 | sensor angle, `i8` degrees (±30) |
| 50 | firmware-maintained (`0x80`, plus a bit per extra stage); do not write |
| 51–62 | Y DPI of stages 1–6, `u16` (equal to X when X/Y is off) |

The vendor parser labels byte 1 as 2.4G and byte 2 as wired. Rate writes over the cable land in byte 1 and
rate writes through the receiver land in byte 2, so the labels are swapped. Each link keeps its own rate.

Polling-rate index: `0` 125, `1` 500, `2` 1000, `3` 2000, `4` 4000, `5` 8000 Hz.

Sensor bits (offset 17):

| Bits | Meaning |
|---|---|
| 0–1 | LOD: `0` = 0.7 mm (L7 Ultra+ only), `1` = 1 mm, `2` = 2 mm |
| 2 | ripple control |
| 3 | angle snapping |
| 4 | motion sync |
| 6–7 | performance mode: `00` Performance, `10` Gaming, `11` Extreme (≥ 20K sensor fps) |

## Write commands

Values in brackets are the payload after the command byte.

| Cmd | Report | Payload | Notes |
|---|---|---|---|
| `0x40` | `0x12` | `wired stage, 2.4G stage, current stage, X[6] u16, count, 0xFF, Y[6] u16` | DPI table |
| `0x41` | `0x11` | `wired rate index, 2.4G rate index` | each link ignores the other link's field |
| `0x42` | `0x11` | `lod, ripple, snap, motion sync, 0, 0, mode, angle on, angle i8` | toggles: `1` on, `2` off, `0` unchanged; lod `0`/`1`/`2` as in the sensor bits (`0` is 0.7 mm here, not "unchanged"); mode `1`–`3` |
| `0x43` | `0x11` | `debounce ms` | |
| `0x0A` | `0x11` | `sleep on (0/1), minutes` | |
| `0x52` | `0x12` | `button, 0, type, value[3]` | one button |
| `0x57` | `0x12` | whole settings block | used by the vendor for profile import |
| `0x58` | `0x11` | `profile` | switch onboard profile |
| `0x59` | `0x12` | `profile, utf8 name` | the vendor appends `$source$id` |
| `0x0B` | `0x11` | `AA 00` | factory reset, all profiles |
| `0x02` | `0x11` | `game (0)` | the vendor sends it after loading the performance page |
| `0x55` / `0x53` | `0x12` | macro data | |

## Buttons

Button index: `0` left, `1` middle, `2` right, `3` forward, `4` back (`5` is not shown by the vendor).

| Type | Action | Value |
|---|---|---|
| 0 | default | – |
| 1 | mouse | `mask, wheel, 0`: mask bit 0 left, 1 right, 2 middle, 3 back, 4 forward; wheel `01` up, `FF` down |
| 2 | keyboard | `modifiers, HID usage, 0` (modifier bitmask as in keyboard reports) |
| 3 | consumer | `usage lo, usage hi, 0` (`E9` vol+, `EA` vol−, `E2` mute, `CD` play, `B5` next, `B6` prev) |
| 4 | macro | – (data via `0x55`) |
| 5 | DPI | `01` cycle, `02` up, `03` down |
| 8 | shortcut | vendor presets (`0C 6F 00` brightness +, `07 01 06` copy, …) |
| 9 | disabled | `FF FF FF` |
| 10 | profile | `01`–`03` switch, `04 00` / `04 01` cycle |

The vendor app refuses to remove the last left-click button. open-mchose does the same.
