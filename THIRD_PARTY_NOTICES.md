# Third-party notices

Scycocks source code is MIT licensed. Distributed binaries include the
components below and carry their terms as well.

## Slint

- Crates: `slint`, resolved version recorded in `Cargo.lock`.
- License chosen: [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).
- Alternatives available from SixtyFPS GmbH: GPLv3, or a commercial license.
- Attribution: the "Built with Slint" badge at the top of the README satisfies section 2(b) of the royalty-free license. It is a link to https://slint.dev and it comes from the `logo/` directory of the upstream Slint repository.
- Copyright: SixtyFPS GmbH, Oranienburger Str. 44, 16540 Hohen Neuendorf, Germany.

Keep the badge and this notice in place when you redistribute the app or a
build of it. Do not ship Slint on its own, and do not publish an app that
exposes Slint's APIs, because the royalty-free license excludes both.
Applications for embedded systems fall outside this license.

## Other dependencies

`hidapi` and every other crate in `Cargo.lock` keep their own licenses. Run `cargo metadata` or
`cargo deny list` to review the full set before you distribute a binary.

## Vendor material

No Scyrox or Endgame Gear firmware, installer, executable, screenshot, or
artwork is distributed with this repository. The research artifacts those
files informed stay in the ignored `.local/` and `reference/` directories. The
MIT license of this project grants no rights to them.
