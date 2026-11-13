# Endgame Gear OP1 8k v2 protocol

## Source

- Artifact: `Endgame Gear Configurator.exe` from `%LOCALAPPDATA%\EndgameGear-Configurator`,
  SHA-256 `e0494dd635e0029fcac1a90e6fa09b00f3ab72d3cfd2abf33e33425153c36c4f`.
  Native 32-bit MFC application, window title "Endgame Gear OP1 8k v2 Configuration Tool",
  PDB path `K26_050_config_v1.14_8k_3950U_3955U`. It links hidapi statically.
- Method: Ghidra 12.1.4 headless decompilation (`scripts/ghidra/DumpAll.java`), PE resource
  parsing (dialog templates and `DLGINIT` combo items), and capstone disassembly to confirm
  call sites. Addresses below are virtual addresses in that build.
- Read-only hardware check: an OP1 8k v2 on firmware V2.73 decoded to plausible settings
  through `scycocks --inspect`. Writes through this app are not yet verified on hardware.

## Device

| | |
|---|---|
| USB | VID `0x3367`, PID `0x1978` (`FUN_004035f0(0x1978)`) |
| Interface | the HID collection with usage page `0xFF01`, usage `2` |

## Transport

Two feature reports. Byte 1 of every reply is a status: `1` done, `3` busy.

| Report | Size | Use |
|---|---|---|
| `0xA1` | 64 | Commands `[A1, cmd, 0…]` and their replies |
| `0xA0` | 1041 | Settings blob: `[A0, cmd/status, 14 bytes]`, then 1024 bytes of settings at offset `0x10` |

- `FUN_00403850`: `HidD_SetFeature`; on errors 0x15/0x17/0x1D/0x57/0x65B it retries up to
  four times, 50 ms apart.
- `FUN_00403920`: `HidD_GetFeature`; while status is `3` it polls every 100 ms for up to 1 s.

| Command | Function | Sequence |
|---|---|---|
| `0x02` version | `FUN_004045e0` | send `[A1, 02]`, wait 50 ms, get `0xA1`; u16 LE at `0x11` ÷ 100 (`V%.2f`) |
| `0x12` read | `FUN_00403b20` | send `[A1, 12]`, wait 80 ms, get `0xA0`; status must be `1` |
| `0x11` write | `FUN_00404180` | set `0xA0` = `[A0, 11, 0…]` + blob at `0x10`, wait 300 ms, get `0xA1` (status not checked) |
| `0x13` | `FUN_00404720` | send, wait 1.1 s, get status. No caller found; not used by this app |

## Settings blob

`FUN_00403c40` copies the blob into the tool's settings structure and `FUN_004042d0`
copies it back. The write path starts from a zeroed buffer, so blob bytes `1..4` and
everything after `0x72` are always written as 0. Offsets are blob offsets.

| Offset | Setting | Encoding |
|---|---|---|
| `0x05` | Polling rate | `0x40` 125, `0x20` 250, `0x10` 500, `0x08` 1000, `0x04` 2000, `0x02` 4000, `0x01` 8000 Hz |
| `0x06` | Flags | bit 0 Slamclick filter, bit 4 Motion Jitter Filter (control hidden by the tool) |
| `0x08` | LED on lift-off | 1 on, 0 off ("Disable LED on Lift-Off" is the inverse) |
| `0x09` | Lift-off distance | 0..10 = 0.7..1.7 mm. In glass mode: 1 = 1.0 mm, 2 = 2.0 mm |
| `0x0A` | Angle snapping | 0/1 |
| `0x0B` | Tuning | bits 0-1 smoothing: 0 ripple off, 1 ripple on, 2 force off. Bits 2-3 CPI downshift: 0 default, 1 medium timer, 2 force off, 3 light timer |
| `0x0C` | Motion Sync | 0/1 |
| `0x0D` | Active CPI stage | 0-based |
| `0x0E` | CPI stage count | 1..4 |
| `0x23 + 5n` | CPI stage n | `[x≠y flag, x u16 LE, y u16 LE]`; 10..30000, steps of 10 to 10000 then 50 |
| `0x37 + 7n` | Button n | `[type, value, u16 param, u16 param, multiclick]` |
| `0x6F` | Sensor Glass Mode | 0/1 (control hidden by the tool) |
| `0x70` | Sensor angle tuning | signed byte, slider −127..127 |
| `0x71` | Force max sensor fps | 0/1 |
| `0x72` | Multiclick acknowledgement | 1 after "I understand…" is checked; unlocks the tool's filter controls. This app copies it back unchanged |

Unmapped bytes inside `0x00..=0x72` (`0x00`, `0x07`, `0x0F..0x22`, button 5, `0x72`) are copied back
unchanged.

Buttons `n`: 0 left, 1 right, 2 middle, 3 forward, 4 back, 5 unused by the tool, 6 wheel up,
7 wheel down.

Multiclick byte (buttons 0-4): 0..25 ms (default 8). For left and right it can also hold
`0xF1` GX Speed Mode or `0xF0` GX Safe Mode, the SPDT switch modes; the slider is then
disabled (`FUN_004062b0`, `FUN_00406180`).

Binding types (`FUN_00408e40` decodes, `FUN_004076c0` and `FUN_00408650` encode):

| Type | Value | Params | Action |
|---|---|---|---|
| `0x00` | button mask: 1 left, 2 right, 4 middle, 8 back, 16 forward | 0, 0 | mouse button |
| `0x01` | `0x01` / `0xFF` | 0, 0 | wheel up / down |
| `0x02` | modifiers: 1 Ctrl, 2 Shift, 4 Alt, 8 Win | HID key usage, 0 | keyboard |
| `0x09` | `0xF1` | 0, 0 | CPI loop |
| `0x0C` | 0 | X, Y | hold for CPI (kept as a raw binding in this app) |
| `0x18` | `0x96` / `0x94` | 1, 0 | browser / explorer (consumer `0x196` / `0x194`) |
| `0x20` | consumer usage low byte (`0xCD`, `0xB5`, `0xB6`, `0xE2`, `0xE9`, `0xEA`) | 0, 0 | media |
| `0xFF` | 0 | 0, 0 | disabled |

## Not used here

- Left-handed mode swaps the left and right bindings (`FUN_00409290`); the binding table
  covers it.
- Glass mode changes the meaning of the lift-off byte and forces item 0 of the polling-rate
  list. The tool hides the control, so this app leaves the byte unchanged.
- The tool's stage-4 X/Y flag reuses stage 3's comparison (`FUN_0040ec20`). This app
  computes each stage's flag from its own values.
