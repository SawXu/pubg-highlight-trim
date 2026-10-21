# Windows installer implementation status

Entry: `build_installer.ps1 -Layout <absolute local layout> -OutputDirectory <dir> -Variant System|Fixed [-MakeNsis <makensis.exe>]`.
Output: `pubg-highlight-trim-system-setup.exe` / `pubg-highlight-trim-fixed-setup.exe`.

The input schema is shared with `scripts/build_installers_windows.ps1`:

* `distribution.json`: schema `pubg-highlight-trim/distribution/v1`, numeric SemVer `version`, lowercase `variant`, `minimum_webview2_major:120`.
* `installer-manifest.json`: schema `pubg-highlight-trim/installer-ownership/v1`, matching `version` and lowercase `variant`, `files:[{path,owner:'application',action:'replace-or-remove'}]`.
* Installer control files `.installer/*` and `Uninstall.exe` are reserved. Payload paths cannot enter `data/`.

NSIS embeds the payload and transaction engine, requests admin, always shows directory selection, rejects silent mode, uses native 64-bit PowerShell, does not launch the application, and creates a regular shortcut. Both variants use the same 64-bit HKLM uninstall identifier. Upgrade rejects a directory different from the registered location and confirms variant switches. Cancel before transaction leaves existing files untouched.

The PowerShell engine stages all new files first; checks exclusive access to owned files; moves the old/new manifest union to a same-volume recovery directory; journals newly placed files, ACLs and registry values; and restores on caught exceptions. Uninstall uses the installed manifest, not a directory wildcard. Unknown files are not overwritten or deleted. Recovery files are retained if rollback fails. No reboot-delayed removal is used.

Code ACLs give SYSTEM/Administrators full control and the intended user RX. Data ACLs give only those identities Modify or full control. Fixed runtime owned files/directories additionally grant AppContainer package SIDs RX. Intended user is the unique Explorer desktop owner in the session, independently of the administrator credentials used for UAC elevation. Ambiguous or absent desktop owners fail closed. A common Start Menu shortcut avoids writing the elevated administrator's profile; another user cannot access the single user's data.

## Important limitations / not release acceptance

This work is **not proof that issue #104 is complete**. Actual elevated installation, offline Fixed startup, runtime loading, upgrade/switch, lock/failure rollback, ACL and uninstall tests have not been run. Do not publish it as lifecycle-validated.

* Caught-exception rollback is implemented; power loss/process termination mid-transaction is not automatically recovered. A failed rollback reports the recovery directory; do not delete it.
* System preflight currently requires same-account UAC elevation. With alternate administrator credentials System install/switch is refused before modifying the existing copy; use Fixed instead. Correct target-user runtime/policy probing for System remains incomplete.
* Before touching old owned files, preflight runs the payload UI `--check-webview2-runtime system|fixed` native loader probe (no UI/data/elevation rejection); Fixed also checks key runtime files. This depends on the UI implementing that exact argument contract. Actual offline rendering and resource loading still require clean-machine lifecycle verification.
* Full arbitrary-data deletion cannot safely distinguish business files from generated browser/cache files. The explicit deletion option removes only exact `data/settings.json`, `settings.tmp`, `run.log`; browser/cache trees and unknown files are always retained. Complete UI-data deletion acceptance is not satisfied.
* User-owned and writable local parent directories are supported without changing their owner or ACL. Directory handles exclude write/delete sharing during the transaction, rejecting rename and junction changes; new workspace/intermediate/code directories are created atomically with administrator-only ACLs. An existing nonempty code root must already be administrator-owned with a protected read-only-for-users ACL; an unsafe existing nonempty directory is not taken over. An empty user-created root is deleted non-recursively and replaced atomically with a fresh protected object, never re-ACL'd in place. Select a new application subdirectory instead. This distinction avoids stale write/WRITE_DAC handles surviving an unsafe in-place takeover. Native directory/reparse tests pass, but elevated adversarial lifecycle tests are still required.
* HKLM binds a one-way SHA-256 digest of the installation root's native volume/file identity as well as the ownership digest; raw file/volume identifiers are not persisted. A replaced root or a registered installation missing its identity fails closed before file replacement; older unshipped groundwork builds without this identity require explicit recovery, not automatic adoption.
* A writable ancestor can still be renamed or replaced **outside** a transaction. Root identity checks protect an already-running trusted installer from adopting the replacement; they cannot authenticate a substituted EXE that the user separately starts and approves through UAC. Obtain manual upgrade/uninstall installers from a trusted source and check publisher/hash. No privileged service or automatic elevation of code at that path is introduced.
* Installed ownership is accepted only with the registered location and matching administrator-protected SHA-256. Existing data ACLs are validated, never recursively repaired; unknown files are not re-owned. Reinstall after uninstall with retained data must use the same intended user.
* The engine does not provide an automatic migration from unrelated legacy portable layouts, and fails on an unowned file conflict instead of overwriting it.

## Verification

`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tests/test_installer_engine.ps1 -MakeNsis <path>` checks the public build boundary (variant rejection, actual NSIS compilation with Chinese and space paths, engine parse) without elevation or install side effects. The test was observed red for the original invalid NSIS uninstall page (`confirm`) then green after switching to `uninstConfirm`. It is **not** a substitute for real Windows lifecycle tests. No elevated destructive test should be run on the development host.
