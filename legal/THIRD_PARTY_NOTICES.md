# Styrhous Third-Party Notices

Styrhous includes third-party software, fonts, and provider marks. Those
materials remain governed by their own terms. The Styrhous Source-Available
License does not replace or restrict the rights their licenses
grant.

The complete selected license texts for Rust dependencies are distributed in
`THIRD_PARTY_LICENSES.html`. Corresponding source for dependencies whose
licenses require source availability is distributed in
`THIRD_PARTY_SOURCE.tar.gz`.

Linux AppImage builds also bundle native shared libraries needed for keyboard
handling. Their distribution-provided copyright and license notices are in the
packaged `APPIMAGE_NATIVE_LICENSES` directory. The build inventories every
bundled shared library against `legal/appimage-native-libraries.tsv`; an unreviewed
library makes packaging fail.

## Fonts

The bundled fonts are licensed under the SIL Open Font License 1.1, except the
DejaVu fonts, which are under the Bitstream Vera / Arev font licenses. Each
complete license is distributed next to the application and is also available in
the source tree:

- Figtree — copyright 2022 The Figtree Project Authors; `Figtree-OFL-1.1.txt`
  (`crates/components/assets/fonts/figtree/OFL.txt`).
- Bricolage Grotesque (static instances) — copyright 2022 The Bricolage Grotesque
  Project Authors; `BricolageGrotesque-OFL-1.1.txt`
  (`crates/components/assets/fonts/bricolagegrotesque/OFL.txt`).
- IBM Plex Mono — copyright © 2017 IBM Corp., Reserved Font Name "Plex";
  `IBMPlexMono-OFL-1.1.txt` (`crates/components/assets/fonts/ibmplexmono/OFL.txt`).
- IBM Plex Sans Condensed — copyright © 2017 IBM Corp., Reserved Font Name "Plex";
  `IBMPlexSansCondensed-OFL-1.1.txt`
  (`crates/components/assets/fonts/ibmplexsanscondensed/OFL.txt`).
- DejaVu Sans and DejaVu Sans Mono (a subset for ↵ ⌘ ⌫ plus the complete fonts as
  the fallback for other symbols and scripts) — copyright Bitstream, Inc. and
  Tavmjong Bah; `DejaVu-LICENSE.txt` (`crates/components/assets/fonts/dejavu/LICENSE`).

## Heroicons

The generic interface icons in `crates/components/assets/icons/` are based on
Heroicons by Tailwind Labs, Inc. and are used under the MIT License:

> MIT License
>
> Copyright (c) 2020 Refactoring UI Inc.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## Provider marks

- `azure.svg` is the Microsoft Azure social icon obtained from Microsoft's
  official brand media. Microsoft Azure and the Azure logo are trademarks of
  Microsoft Corporation. Their inclusion identifies a supported provider and
  does not imply endorsement.
- `tailscale.svg` is the official Tailscale squircle obtained from Tailscale's
  media kit. Tailscale and its logo are trademarks of Tailscale Inc. Their
  inclusion identifies a supported provider and does not imply endorsement.

No trademark license is granted for either provider mark beyond the permission
given by its owner and applicable law. See
`crates/components/assets/icons/ATTRIBUTION.md` for source details.
