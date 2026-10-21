# UI / Tauri / Fixed release notices

The Windows installer entry always runs `collect_ui_licenses.mjs` before installer ownership enumeration and engine invocation, including `-SkipUiBuild` reuse. Missing license metadata, missing notice text, stale installed npm versions, untrusted Fixed evidence, or changed pinned texts fail the release. This is a notice/inventory gate, **not legal approval** or proof of VM/runtime acceptance.

```powershell
node scripts/collect_ui_licenses.mjs --ui ui --bundle build/notice-check --variant System
node --test scripts/tests/collect_ui_licenses.test.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/test_ui_license_release.ps1
```

Use `npm ci` first and fetch locked Cargo packages as part of the build. The collector never downloads licenses and runs `cargo metadata --locked --offline --filter-platform x86_64-pc-windows-msvc` on `ui/src-tauri/Cargo.toml`. It independently starts at `pubg-highlight-trim-ui` (the OCR xtask is deliberately unchanged: its root/features/supplement policy are specific to the OCR app and it permits missing text). Normal dependencies are conservatively `shipped`; proc-macro/build subgraphs are `build-only`; dev-only nodes are excluded. All included nodes require license metadata and actual nonempty text. LICENSE/LICENCE/NOTICE/COPYING/ThirdPartyNotices files are copied recursively, excluding dependency/build directories. Explicit Cargo `license_file` is also copied. Five missing archive texts have pinned supplements; see `ui-license-notices/README.md`.

Npm starts at package.json production and optional dependencies, resolves locked transitive normal/optional/peer dependencies including hoisted/nested packages, verifies installed versions, and copies original notice texts. This conservative production closure can overinclude tree-shaken packages: it never claims every package is a separate file in the binary. Dev-only build tools are not asserted shipped. Unknown workspace links, missing installed optional/peer dependencies, and missing notices fail rather than guessing. Lock hashes, source/integrity metadata and every copied text hash are recorded in `third_party/ui/UI_LICENSE_INVENTORY.json`. Each notice and inventory is in the installer-owned payload. On a reused bundle, the release operator remains responsible for matching executable/assets to these exact locks; rebuilding is the default.

## Fixed: mandatory separately trusted evidence

`build_installers_windows.ps1 -Variant Fixed` additionally requires:

- `-FixedRuntimeLicenseManifest <absolute JSON path>`
- `-FixedRuntimeLicenseManifestSha256 <trusted SHA256 of that JSON>`

This evidence must be outside the actual runtime directory (otherwise a runtime inventory would self-reference the manifest hash). All `notices[].path` values are relative to the JSON directory. The manifest and original terms/notices are copied to the final product. The release operator must obtain the real runtime from Microsoft, review its applicable redistribution terms/third-party notices and obligations, and supply a trusted manifest hash. No license text is invented by this repository. A Microsoft URL is provenance recorded by the reviewer, not a cryptographic proof of authorship; the trusted input hash/review is the trust boundary. Test evidence is deliberately fake and must never be used for release.

Schema (descriptive placeholders below are **not** valid release evidence):

```json
{
  "schema": "pubg-highlight-trim/fixed-runtime-license/v1",
  "version": "<actual complete Microsoft runtime version>",
  "source": "https://<authoritative Microsoft terms/distribution source>",
  "reviewed_by": "<release reviewer>",
  "obligations": "<reviewed applicable redistribution/notice/source requirements and how fulfilled>",
  "runtime_files": [
    {"path": "<every runtime file, relative slash path>", "sha256": "<actual SHA256>"}
  ],
  "notices": [
    {"path": "<original license text file>", "sha256": "<actual SHA256>", "source": "https://<authoritative Microsoft source>"},
    {"path": "<original third-party notices file>", "sha256": "<actual SHA256>", "source": "https://<authoritative Microsoft source>"}
  ]
}
```

Every runtime file must match exactly; extras, omissions, symlinks and modified bytes reject. The collector requires Microsoft HTTPS source domains, version/reviewer/obligations, and at least one original nonempty notice. Review must determine which license/third-party notice/source files the particular distribution actually requires; merely satisfying schema is not legal acceptance. System variant does not distribute WebView2, so no Fixed terms are invented for it.

## Actual local collection evidence

The locked System closure was collected successfully offline on this worktree: 274 components (26 npm production packages, 180 shipped Rust packages, 68 build-only Rust packages), 466 copied notice texts. Regeneration was exercised; the inventory contains no generation timestamps or machine-local source paths. This is source/notice collection evidence only, not a real installer/Fixed runtime or clean-host result.

## Verification boundary

Tests cover missing npm and Tauri transitive notices/metadata, dependency scopes, hashes, deterministic regeneration, explicit license_file, and Fixed missing/untrusted/tampered evidence. PowerShell test invokes a copied real release builder and collector, a real offline Cargo fixture, and a controlled installer engine, then verifies the resulting fixture product's notices/hashes/ownership. It proves the license gate cannot hand a missing-notice layout to the engine; it is not a claim of real NSIS extraction, clean-host installation, runtime validity or legal review. No trusted actual Fixed runtime or VM was available for these tests.
