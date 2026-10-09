# Third-party notices for the combined Ghostty archive

The static archive shipped at both `Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a`
and `Vendor/GhosttyVt/macos-arm64/lib/libghostty-vt.a` includes the components below.
The two paths contain the same native image. Versions and source identities are pinned in
`ThirdPartyNotices/inventory.json`; the full license texts copied from those
exact source trees are under `ThirdPartyNotices/licenses/`.

This notice set must accompany redistribution of binaries that incorporate this archive.
It supplements, and does not replace, the package license in `LICENSE` and Ghostty's license
in `LICENSE-GHOSTTY`.

The Linux VT artifacts under `Vendor/GhosttyVt/linux-x86_64` contain a smaller
dependency closure recorded independently in
`ThirdPartyNotices/linux-vt-inventory.json`. They reuse the applicable notice
texts below but do not contain the macOS combined image's libintl, z2d, fonts,
or application/runtime dependencies.

## Bundled software and data

- FreeType 2.13.2 — FreeType Project License 2006, with separately licensed BDF/PCF,
  HarfBuzz-derived, and zlib-derived portions preserved in the notice bundle.
- libpng 1.6.43 — libpng license.
- zlib 1.3.1 — zlib license.
- Oniguruma 6.9.9 — BSD-2-Clause.
- simdutf 9.0.0 — MIT, with PyTorch-derived ISA detection under BSD-3-Clause
  and a separately traced Fuchsia-derived validation routine under BSD-3-Clause.
- Highway 1.2.0 / commit `66486a10623fa0d72fe91260f96c892e41aceb06` —
  Apache-2.0 and BSD-3-Clause portions.
- GNU gettext/libintl 0.24 — LGPL-2.1-or-later.
- stb_image 2.28 and stb_image_resize 0.97 — MIT.
- Wuffs `0.4.0-alpha.10+3966.20260623` — MIT (also offered under Apache-2.0).
- libxev commit `9ce8e8e6ff89e583258a7f8e7adeeeaeae8611bf` — MIT.
- vaxis 0.6.0 / commit `1dbbe575dff4586fe51e3217aa5c3fecdcbb6089` — MIT.
- z2d 0.12.1 / commit `7dbae85c81784dba9988320bf9543ed9a81350c8` — MPL-2.0,
  including the additional upstream notices preserved in
  `ThirdPartyNotices/licenses/z2d-NOTICE.txt`.
- zf 0.11.0 / commit `c35c421f84895193246db06c40683c1a30e616ef` — MIT.
- uucode 0.2.0 / commit `9d55524551411b493cca41ca06363625d90aff1e` — MIT,
  including Bjoern Hoehrmann's MIT-licensed UTF-8 decoder and Unicode data under the
  Unicode License v3.
- zig-objc commit `c8de82ff80281215ad92900866dab7103a8efa8b` — MIT.
- JetBrains Mono 2.304 variable regular and italic fonts — SIL Open Font License 1.1.
- Nerd Fonts Symbols Only 3.4.0 — MIT.
- Zig 0.16.0 compiler runtime: Zig-authored MIT portions, LLVM-derived portions under
  Apache-2.0 WITH LLVM-exception and legacy permissive LLVM terms, and musl-derived
  math portions under musl's permissive notices.
- Chromium-derived DOM keycode table data — BSD-3-Clause.
- foot 1.15.3-derived legacy and Kitty keyboard mapping tables — MIT.
- freetype-gl-derived texture-atlas packing code, with RectangleBinPack public-domain lineage — BSD-2-Clause and public-domain notice.
- Oniguruma generated Unicode 15.1 property/folding tables — BSD-2-Clause and Unicode data terms.
- Kitty row/column diacritics selected from Unicode 6.0.0 data and transformed by Ghostty; source-era Unicode terms and the transformation statement are preserved. Kitty's generator/repository license is recorded as provenance-only because generator code is not shipped.
- X.Org `app/rgb` color-name data imported by Ghostty at commit `cf8763561d69c58a1d1b49ab3e7c1a1d731443bb` — MIT/X11; exact data SHA-256 `f8e3a7bea17acc0b91e6285c5d32001db27d84c4461c655afdbce9dbeb4fb6f0`.
Ghostty itself and the local Wuffs Zig adapter are covered by `LICENSE-GHOSTTY`.

## GNU libintl static-linking notice

This archive contains a statically linked copy of GNU libintl from gettext 0.24 under the
GNU Lesser General Public License, version 2.1 or (at your option) any later version. The
full license is in
`ThirdPartyNotices/licenses/gettext-libintl-LGPL-2.1-or-later.txt`.

This repository supplies the exact compiled libintl corresponding-source subset,
build configuration, headers, and LGPL text in
`ThirdPartyNotices/corresponding-source/libintl-0.24.tar`. The file manifest and
hashes are in
`ThirdPartyNotices/corresponding-source/libintl-0.24/MANIFEST.json`. The
combined archive retains its individual object members;
`Script/relink-libintl.sh` removes the exact 30 libintl members and recombines
the remaining objects with a caller-supplied compatible replacement
`libintl.a`. Distributors of final executables remain responsible for supplying
these materials and the required relinking means with their distribution.

## MPL source availability

z2d is incorporated in executable form under MPL-2.0. Its exact corresponding source is
retained at
`ThirdPartyNotices/corresponding-source/z2d-7dbae85c81784dba9988320bf9543ed9a81350c8.tar.gz`.
`ThirdPartyNotices/inventory.json` records the upstream and mirror URLs, acquisition method
and date, exact commit, byte size, and SHA-256. Binary distributors must make that source,
including any modifications to MPL-covered files, available as required by MPL-2.0 section
3.2 and tell recipients how to obtain it.
