# Scyrox V8 protocol notes

This is an independent implementation. Vendor binaries, firmware, artwork, and JavaScript are not redistributed.

## Evidence

| Artifact | SHA-256 | Observation |
| --- | --- | --- |
| Supplied `V8 mouse Upgrade Tool.zip` | `1b15ec9c83ad60e9683ef104117921fb2088c3c0c30ca1e7e952471a98eef8ef` | Contains one executable, not an INF/SYS driver package. |
| Extracted upgrade executable | `6a9e6fd79ec319b2ff04d555c9551c933d9a23332e171ad6ee83c786e98286f9` | .NET Framework 4.8, x86, assembly `UpgradeTool` 2.0.0.0. |
| Embedded `UpgradeTool.Resources.res.dev0.upgrade.bin` | `19378133d26f866fee4daefc1a48b0d841cc3c2482d7e3f41b1b82646fb8e1cd` | 96,272 bytes. Header contains `ComUsbUpgradeFile`, `NRF52840`, bootloader path `vid_3554&pid_f403&col01`, normal wired path `vid_3554&pid_f5f6&mi_01&col05`. |
| Installed Endgame Gear executable | `e0494dd635e0029fcac1a90e6fa09b00f3ab72d3cfd2abf33e33425153c36c4f` | Native PE32. Dialog strings identify polling rate, button mapping, Motion Sync, ripple control, angle snapping, and advanced sensor controls. Used as a feature reference only. |
| `https://www.scyrox.net/assets/index-DUfu8aky.js` | `d560f4865814980807d4e2a58d6340474405c66b75626f572077ea370236532f` | Static source for HID packet format and settings encodings. |
| `https://www.scyrox.net/cfg.json` | `676d20a3f2617feee39b247c4f681b28c4990261c52a47802e1d322d710c7d49` | VID/PIDs, company/model IDs, V8 sensor limits. |
| `https://www.scyrox.net/sensor.json` | `3a196093618870116babea5cdd14cb9f0f192acef45e55da9e577ad16b27dba0` | PAW3950 uses 50-DPI steps through 30,000 DPI. |

REA evidence records:

- Archive inventory: `ev_da542e57eb35783676a4675db4d3cb49b15102f704bd6370918daef2efbb1aa1`.
- Archive extraction: `ev_988629f48b4e8165de4dccc856da1181628999f853d47a3f2359f0d08f82a484`.
- Managed metadata/resources: `ev_57f71379021a0527bd1692287c39eb4ec3c710513e291ed0acd9b346be3edbf1`.
- Managed native declarations: `ev_307ad625548fbbf7b39b5eb187137eebc2c3bc13bb7f5ccab41625d61b84ed50`.
- Endgame Gear resources: `ev_1760488a35b96da341cabd2b03782a88afcaa3f4772572308f011f860a33ec7c`.

The updater declares Windows HID, SetupAPI, and Kernel32 imports, including `HidD_SetFeature` and `WriteFile`. Import declarations alone do not prove runtime behavior. Firmware bytes were extracted directly from the mapped CLI resource section without loading the assembly.

## Firmware updater reconstruction

ILSpy 9.1 decompiled the supplied executable without executing the updater. The relevant output is ignored under `.local/research/updater-source/UpgradeTool.USB/UsbUpgradeFile.cs`. These are static findings, not observed flashing behavior.

- `UpgradeFileHeader` (line 109) embeds endpoint strings and command templates. The image declares device type `D2` (mouse), company `3E` (62), model `01`, sensor `3950`, and version `0x222`.
- `GetHeaderCRC` (line 291) uses an additive header checksum, not a cryptographic CRC: header checksum plus the sum of bytes 8 through 719 equals `0x55555555`. This check passes for the supplied image.
- The image contains an 8,192-byte prefix followed by 88,080 firmware bytes. The payload SHA-256 is `d5886fda4e710f6e3de877c6b786bd4a0d1a542139ec7ee0088cbec0e7c25f81`. The prefix size is a container offset, not proof of an on-chip bootloader boundary.
- `Upgrade` calls `ResetToBootMode`, `WaitBootMode`, then `DownloadFile`. The embedded reset template uses output report `08`, command `0D`. The device then re-enumerates under bootloader PID `F403`.
- `CreateUpgradePacket` (line 390) places a prepare template first, then copies firmware bytes unchanged into 32-byte chunks. Download packets use command `B1`, continuation flag `C0` or final flag `C1`, a big-endian destination address at bytes 5–8, and data starting at byte 17. The embedded initial destination is zero.
- The supplied prepare/download templates select output reports (`feature=0`), report ID `06`, declared command length 49. The host implementation also supports feature-report templates, but this image does not select them.
- `UpgradeHandler` processes `B0` prepare responses, `B1` acknowledgements, and `5B B5` state messages. It treats state `88` as success. Erase-state labels suggest main/backup handling but do not establish physical flash layout or recovery guarantees.

No host-side payload decryption occurs in the inspected packet-construction path. This does not establish whether the bootloader decrypts, authenticates, or transforms the payload. No bootloader command was sent during this investigation.

## Configuration transport

The website calls `navigator.hid.sendReport(8, payload)`, not a kernel-driver API. Its configuration payload is 16 bytes. Windows/hidapi prepends the report ID, making a 17-byte application buffer:

| Byte | Meaning |
| --- | --- |
| 0 | Report ID `08` |
| 1 | Command |
| 2 | Request zero / response status |
| 3–4 | Big-endian configuration address |
| 5 | Data length, at most 10 for mouse configuration |
| 6–15 | Data, zero-padded |
| 16 | Checksum so the full report sums to `55` modulo 256 |

The PAW3950 lift-off options come from `https://www.scyrox.net/lang/en.json`, under `mouse.sensor.LODOptions.3950`. The generic sensor options omit the V8's 0.7 mm setting.

The website's `Mr`, `Ra`, `Ks`, `Mt`, `Et`, `st`, and `xr` functions establish packet construction, read/write framing, and checksums. `Ss` decodes input reports. Symbol names are specific to this bundle.

Supported discovery PIDs from the vendor configuration: `F5F4`, `F5F7`, `F517`, `F5F6`, `F515`, all under VID `3554`. Only identity company `62`, model `1`, `10`, or `11` enables V8 configuration. Other models are rejected even when they share the VID/PID.

| Command | Meaning |
| --- | --- |
| `01` | Identify device. Eight-byte challenge payload. Response payload bytes 9–11 (Windows buffer bytes 10–12) contain company, model, and connection type. |
| `03` | Mouse online. Response byte 6 is 1 when online. |
| `04` | Battery. Response bytes 6–9 contain level, charging flag, and big-endian voltage. |
| `05` | Enter receiver pairing mode. Vendor packet uses length 2, data bytes `00 00 3E` (CID 62). |
| `06` | Pair result: response byte 6 is 1=pending, 2=failed, 3=success; byte 7 is remaining time. |
| `09` | Clear device settings, not the bootloader reset. Wait up to six seconds for acknowledgement. |
| `0F` | Select profile, one-byte zero-based index 0–3. Query `0E` verifies the switch. |
| `07` | Write a configuration block. This is not firmware flashing. |
| `08` | Read a configuration block. |
| `0E` | Current profile query. |
| `12` | Mouse firmware version. Response byte 6 is decimal major, byte 7 is displayed in hexadecimal. |
| `1D` | Receiver firmware version query. |

Connection types: `0` = wireless 1 kHz, `1` = wireless 4 kHz, `2` = wired 1 kHz, `3` = wired 8 kHz, `4` = wireless 2 kHz, `5` = wireless 8 kHz.

Live HID descriptor inspection identifies configuration usage `FF02:0002`, report ID `08`, 16-byte input and output payloads. The connected receiver is `3554:F5F7`, product `SCYROX 8K Dongle`, interface 1, collection 5. Other vendor collections expose different report IDs and are excluded from discovery.

Configuration traffic uses output/input reports. The updater's reset starts on the normal configuration interface, but firmware transfer occurs after bootloader re-enumeration. The app does not expose bootloader reset `0D` or the bootloader transport. Its factory-reset control uses configuration command `09`.

## Configuration memory

The website reads the first 256 configuration bytes in blocks of at most 10 bytes. Most scalar settings occupy two bytes `[value, 0x55 - value]`, modulo 256. DPI entries and button bindings use four-byte blocks with the same additive checksum.

| Offset | Meaning |
| --- | --- |
| 0 | Polling rate: `08`=125 Hz, `04`=250 Hz, `02`=500 Hz, `01`=1000 Hz, `10`=2000 Hz, `20`=4000 Hz, `40`=8000 Hz |
| 2 | Number of DPI stages |
| 4 | Active DPI stage, zero-based |
| 8 | Scyrox 20K scanning toggle; requires Competitive Mode |
| 10 | Lift-off distance: `03`=0.7 mm, `01`=1 mm, `02`=2 mm for PAW3950 |
| 12 + 4 × stage | PAW3950 X/Y DPI, packed high bits, checksum |
| 44 + 4 × stage | Stage color, not edited by this app |
| 96 + 4 × button | Action type, big-endian parameter, checksum |
| 169 | Debounce |
| 171 | Motion Sync |
| 173 | Sleep time in 10-second units: 1, 3, 6, 12, 18, 30, 60, 180 |
| 175 | Angle snapping |
| 177 | Ripple control |
| 181 | Competitive Mode enable |
| 183 | Performance timeout; the Scyrox sleep setter writes both 173 and 183 |
| 185 | Generic sensor mode, preserved |
| 189 | Signed angle tuning, -30 to +30, encoded modulo 256 |
| 191 | Angle tuning enable |
| 256 + 32 × button | Keyboard/media shortcut record |
| 225–230 | Optional FPS and release/wheel debounce fields, not edited |

For DPI up to 30,000: `encoded = DPI / 50 - 1`. Store X and Y low bytes, then `(high << 2) | (high << 6)`, then checksum. Existing asymmetric X/Y DPI is rejected instead of silently normalizing it.

Mouse buttons use type `01` and parameters `0100`, `0200`, `0400`, `0800`, `1000`. Type `02`, parameter `0100` cycles DPI. Type `07`, parameter `0100` cycles polling rate. Type `00`, parameter zero disables a button. Type `05`, parameter zero, uses the button's 32-byte shortcut slot. The first byte is event count. Three-byte events contain type/press/release flags and a little-endian value. Press flag is `80`, release flag `40`. Types 0/1/2 mean modifier/keyboard/consumer key. Releases reverse the press order. A checksum follows the events, not the end of the slot. The app writes shortcut data before activating its binding. Unknown records and macros remain unchanged unless explicitly replaced; macro storage is never written.

The added controls follow vendor functions `z5` (Competitive Mode), `y5` (Scyrox 20K), `K5` (sleep), `j5` (angle), `m5` (stage count), `Gg` (profiles), `Ug`/`Xv` (pairing), `Kg` (reset), and `Z5`/`X5`/`J5` (keyboard/media/button bindings) in the archived web bundle. Scyrox's visible 20K switch uses offset 8, not the generic sensor FPS field at 225. New DPI records are initialized before increasing the stage count. The active index is lowered before reducing the count. Profile identity and shortcut records are included in stale-setting checks.

## Live read verification

The connected `3554:F5F7` receiver returned company/model/connection identity `[62, 1, 5]`. Configuration reads passed report and setting checksums. The V8 reported firmware `v2.22`, receiver firmware `v2.16`, battery 100%, 800 DPI, one active stage, 2,000 Hz, and lift-off code `03` (0.7 mm). Standard six-button bindings decoded correctly.

This verifies discovery, identity, online status, configuration reads, settings decoding, battery, and version queries on the connected receiver. Wired mode was not tested. Setting writes, persistence after a power cycle, and firmware flashing were not tested on hardware. No setting or firmware write was sent.

## Safety and limits

- Configuration operations never enter the bootloader or flash firmware. Pairing and factory reset require their separate explicit controls. Factory reset requires a second click.
- Startup opens the supported configuration interface and reads settings. It does not write settings. Connect/Reconnect retries the same read-only workflow. Online checks run once per second without reloading drafts.
- Apply validates ranges, checks for external changes, and writes only changed blocks directly to the mouse. No local backup is created or required.
- Each write requires an acknowledgement and matching readback. A failure stops the sequence. Partial completion is possible and is reported.
- There is no automatic retry for writes and no automatic rollback. Reconnect and inspect current settings after a failure.
- Basic button changes retain at least one left-click binding and install its replacement before changing the original.
- Only V8/PAW3950 settings are enabled. Other Scyrox sensor encodings are not assumed compatible.

Static findings do not prove firmware execution or complete bootloader behavior. The nRF52840 string identifies the updater's declared target, not a verified silicon readout. Firmware signing/encryption, flash layout, recovery, and replacement firmware remain unresolved. Replacing firmware is not necessary for the recovered configuration path.
