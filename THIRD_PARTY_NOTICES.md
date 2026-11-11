# Third-party notices

Scycocks source code is MIT licensed. Distributed binaries include the
components below and carry their terms as well.

## wxWidgets

- Crates: `wxdragon` and `wxdragon-sys` (MIT OR Apache-2.0), resolved versions recorded in `Cargo.lock`. They statically link wxWidgets.
- License: [wxWindows Library Licence 3.1](https://www.wxwidgets.org/about/licence/). It is the LGPL with an exception that permits distribution of binary forms under the user's own terms.
- Copyright: Julian Smart, Robert Roebling, and other wxWidgets contributors.

## Other dependencies

`hidapi` and every other crate in `Cargo.lock` keep their own licenses. Run `cargo metadata` or
`cargo deny list` to review the full set before you distribute a binary.

## Vendor material

No Scyrox or Endgame Gear firmware, installer, executable, screenshot, or
artwork is distributed with this repository. The research artifacts those
files informed stay in the ignored `.local/` and `reference/` directories. The
MIT license of this project grants no rights to them.
