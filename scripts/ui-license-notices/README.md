# Pinned UI Rust notice supplements

These are verbatim source texts, not generated SPDX license templates. `SOURCES.json` records each exact crate version, `.cargo_vcs_info.json` revision, original HTTPS source and SHA256. Collection is offline and rejects changed text/revision. They supplement missing crate-archive notices only; newly missing texts fail closed.

- alloc-stdlib 0.3.0: the repository root LICENSE at its packaged Git revision, copyright Dropbox, BSD terms.
- webview2-com 0.39.1 / webview2-com-sys 0.39.1 and webview2-com-macros 0.8.1: repository root MIT LICENSE at each packaged revision. The two revisions currently have identical text; their provenance remains separate.
- selectors 0.38.0: packaged `lib.rs` and other source files explicitly identify MPL 2.0 and link `https://mozilla.org/MPL/2.0/`. Its upstream revision has no root license file. The supplement is the official Mozilla MPL 2.0 plaintext from `https://www.mozilla.org/media/MPL/2.0/index.txt`, bound by SHA256 (the license version, not a fabricated upstream LICENSE). The recorded revision binds the crate's license declaration, not Mozilla's website revision. Preserve MPL source availability obligations: corresponding exact crate source is available through crates.io (`https://crates.io/api/v1/crates/selectors/0.38.0/download`) and the recorded upstream revision; distribute corresponding project source/build changes under the project's source-distribution policy.

No WebView2 Fixed runtime license terms are supplied here. Rust WebView2 binding MIT terms do **not** license Microsoft's runtime binaries.
