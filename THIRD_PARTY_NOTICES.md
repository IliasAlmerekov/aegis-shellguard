# Tree-sitter third-party notices

## Scope — read this first

This file covers **only the Tree-sitter components**: the runtime (including the
ICU subset it vendors), its `tree-sitter-language` ABI shim, and the four
qualified grammar crates. They are
the sole native-C build inputs admitted by the exception in
[ADR-022 §8](https://github.com/IliasAlmerekov/aegis-shellguard/blob/main/docs/adr/adr-022-language-aware-analysis-is-an-additive-isolated-stage.md),
which is why they are attributed by hand and pinned by a contract test.

It is **not** a complete notices set for the release binary. Aegis statically
links roughly a hundred other Rust crates (`clap`, `regex`, `aho-corasick`,
`serde`, `tokio`, `crossterm`, …), essentially all MIT or Apache-2.0, and none of
them are attributed anywhere in this repository yet. Generating the full set —
for example with `cargo-about`, keeping the rows below as the hand-verified
Tree-sitter subset — is open follow-up work. Do not treat this file as a
distribution-complete attribution record until that lands.

`cargo deny check` enforces the dependency **license policy** (allowed SPDX
identifiers) for the whole graph; it does not produce attribution. This file is
attribution for one subset, not a policy check.

## Distribution channels

Two channels carry this notice directly and fail closed if it is missing: it is
published as a GitHub Release asset, and it is packed into the npm tarball.

`cargo install --git … --tag vX.Y.Z aegis` builds from a source tree that contains
this file, so that channel is covered by construction.

The Homebrew formula (`packaging/homebrew/Formula/aegis.rb`) and the
`scripts/install.sh` installer place only the binary on disk; users of those
channels get the attribution from the Release the binary was downloaded from.
Extending them is deliberately deferred rather than done here: both resolve
asset URLs for *already published* tags, none of which carry this asset, so a
fetch would 404 for every existing version — fail-closed would break those
installs and best-effort would add an untested network path. The clean fix is to
add the notice to the formula's install step and the installer's download list in
the same change that cuts the first release containing it.

## Components

Versions below are asserted against `Cargo.lock` by
`tests/l1_qualification_contracts.rs`, which also flags a newly vendored
Tree-sitter crate that has no row here.

The `tree-sitter` runtime is dual-licensed in effect: the crate declares `MIT`,
but it also vendors a small ICU subset under the Unicode license — see the
[Unicode license](#unicode-license-icu-58-and-later) section below. `cargo deny`
only sees the crate-declared `MIT`, which is why that row is hand-maintained and
pinned by the contract test.

| Component | Version | Upstream | SPDX license | Copyright notice |
|---|---:|---|---|---|
| `tree-sitter` | `0.26.11` | <https://github.com/tree-sitter/tree-sitter> | MIT AND Unicode-DFS-2016 | Copyright (c) 2018 Max Brunsfeld |
| `tree-sitter-python` | `0.25.0` | <https://github.com/tree-sitter/tree-sitter-python> | MIT | Copyright (c) 2016 Max Brunsfeld |
| `tree-sitter-javascript` | `0.25.0` | <https://github.com/tree-sitter/tree-sitter-javascript> | MIT | Copyright (c) 2014 Max Brunsfeld |
| `tree-sitter-typescript` | `0.23.2` | <https://github.com/tree-sitter/tree-sitter-typescript> | MIT | Copyright (c) 2017 Max Brunsfeld |
| `tree-sitter-bash` | `0.25.1` | <https://github.com/tree-sitter/tree-sitter-bash> | MIT | Copyright (c) 2017 Max Brunsfeld |
| `tree-sitter-language` | `0.1.7` | <https://github.com/tree-sitter/tree-sitter> | MIT | Copyright (c) 2018 Max Brunsfeld |

## MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Unicode license (ICU 58 and later)

The `tree-sitter` runtime vendors a subset of the Unicode organization's ICU
project under `src/unicode/` (ICU commit
`552b01f61127d30d6589aa4bf99468224979b661`). Three of those headers carry real
ICU code — `utf8.h`, `utf16.h`, `umachine.h`; the remaining `utf.h`, `ptypes.h`,
and `urename.h` are empty stubs. `src/unicode.h` includes them and is compiled
into `lexer.c` and `query.c`, so this code is in every release binary on all four
targets — it is not an optional feature.

**Copyright holders.** The three vendored headers each carry two notices:

```
// © 2016 and later: Unicode, Inc. and others.
// License & terms of use: http://www.unicode.org/copyright.html
*   Copyright (C) 1999-2015, International Business Machines
*   Corporation and others.  All Rights Reserved.
```

(`umachine.h` and `utf8.h` read 1999-2015; `utf16.h` reads 1999-2012.) So both
**Unicode, Inc.** and **International Business Machines Corporation and others**
are copyright holders here, and both are named for that reason.

**Which ICU terms apply.** The headers' own "© 2016 and later … License & terms
of use: unicode.org/copyright.html" marker places them under the post-ICU-58
Unicode license reproduced below — the checkout is ICU commit `552b01f…`, well
after the ICU 58 relicensing — so the earlier `ICU License - ICU 1.8.1 to ICU
57.1` (section 1 of `src/unicode/LICENSE`) governs older ICU releases, not this
code; IBM remains a named copyright holder under the Unicode terms. Sections 2-6
of that file cover ICU's dictionary, time-zone, and double-conversion data, none
of which is vendored here.

The full license as shipped by the crate is `src/unicode/LICENSE` in
`tree-sitter 0.26.11`. Its governing notice is reproduced verbatim below.

**When bumping `tree-sitter`, re-verify this whole section, not just the version
row.** The contract test pins the version, the header filenames, and the two
copyright holders — but the ICU commit, the three-real/three-stub header
inventory, and the per-header year ranges above are all transcribed facts that a
re-vendored ICU subset can invalidate silently. Re-read `src/unicode/ICU_SHA`,
`src/unicode/README.md`, and the header comments in the new crate version.

The runtime also vendors `src/portable/endian.h`, included on the same
`src/unicode.h` path. Its header declares `"License": Public Domain`, which
carries no attribution obligation, so it needs no row above — recorded here so the
scope statement is reproducible rather than looking like an oversight. A full-tree
scan of the crate found no other third-party copyright holders; `src/wasm/` is not
compiled, since `aegis-language` takes the crate's default features and `wasm` is
not among them.

COPYRIGHT AND PERMISSION NOTICE (ICU 58 and later)

Copyright © 1991-2019 Unicode, Inc. All rights reserved.
Distributed under the Terms of Use in https://www.unicode.org/copyright.html.

Permission is hereby granted, free of charge, to any person obtaining
a copy of the Unicode data files and any associated documentation
(the "Data Files") or Unicode software and any associated documentation
(the "Software") to deal in the Data Files or Software
without restriction, including without limitation the rights to use,
copy, modify, merge, publish, distribute, and/or sell copies of
the Data Files or Software, and to permit persons to whom the Data Files
or Software are furnished to do so, provided that either
(a) this copyright and permission notice appear with all copies
of the Data Files or Software, or
(b) this copyright and permission notice appear in associated
Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT OF THIRD PARTY RIGHTS.
IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS
NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL
DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE,
DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THE DATA FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder
shall not be used in advertising or otherwise to promote the sale,
use or other dealings in these Data Files or Software without prior
written authorization of the copyright holder.
