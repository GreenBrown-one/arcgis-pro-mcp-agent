# Task 8 Installer Crash-Recovery Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This review is executed inline; do not delegate it.

**Goal:** Make the Windows development installer safe against path aliases, concurrent installers, process crashes, operation races, and interrupted cleanup without touching real user installation directories.

**Architecture:** Keep `Install-Dev.ps1` as the build wrapper and move Windows entity-path and file-identity primitives into a small compiled C# helper loaded by the PowerShell core. Replace the transient journal with one fixed, strict, atomically updated transaction journal whose phase and per-operation state drive startup recovery, rollback, and committed cleanup under an exclusive installer lock.

**Tech Stack:** Windows PowerShell 5.1, embedded C# P/Invoke (`CreateFileW`, `GetFinalPathNameByHandleW`, `GetFileInformationByHandle`), NTFS atomic replace/move, SHA-256, self-contained PowerShell integration tests.

## Global Constraints

- Preserve commits `b967173` and `e28d709`; add a focused follow-up commit.
- Use only unique verified system-temporary roots and, where supported, one unique temporary `SUBST` drive letter; clean exact targets in `finally`.
- Do not run ArcGIS Pro GUI, create a tag, or install to real user directories.
- Fail closed when a Windows entity path or file identity cannot be resolved.
- Keep the task report `PARTIAL / PENDING` until GUI smoke and the foundation tag are genuinely complete.

---

### Task 1: Windows Entity Paths and Exclusive Lock

**Files:**
- Modify: `scripts/Test-InstallDev.ps1`
- Modify: `scripts/Install-Dev.Core.psm1`

**Interfaces:**
- Produces: `Get-WindowsEntityPath(string path, bool allowMissingTail)` and an exclusive, fixed-path installer lock held for the complete recovery/install/cleanup lifecycle.

- [ ] Add failing tests proving `SUBST` aliases cannot bypass source/root containment and, when the volume exposes an 8.3 alias, the short path resolves to the same entity; otherwise print an explicit skip.
- [ ] Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\Test-InstallDev.ps1 -WorkspaceRoot .` and confirm the alias tests fail against lexical paths.
- [ ] Add the minimal C# Win32 helper that opens the deepest existing ancestor with `FILE_FLAG_BACKUP_SEMANTICS`, resolves the normalized final entity path, appends only missing tail segments, normalizes volume-GUID/UNC forms, and fails closed.
- [ ] Switch topology, overlap, containment, source, manifest, journal, backup, stage, and operation boundary checks to entity paths; acquire an exclusive fixed installer lock before recovery and hold it through cleanup.
- [ ] Re-run the focused installer tests and confirm the entity-path tests pass.

### Task 2: Strict Durable Journal and Identity-Bound Operations

**Files:**
- Modify: `scripts/Test-InstallDev.ps1`
- Modify: `scripts/Install-Dev.Core.psm1`

**Interfaces:**
- Produces: fixed `.arcgis-pro-agent-install-journal.json` schema with `phase`, roots, expected manifest bytes, and operations containing `kind`, `target`, `backup`, `temporary`, `applied`, expected old/new hash and length, and recorded temporary file identity.

- [ ] Add failing tests where the pre-move hook creates an unowned New target, an owned target is changed immediately before Replace, and a DLL/EXE target has a second hard link.
- [ ] Confirm current rollback deletes the raced New target and current replacement cannot reliably distinguish the concurrent change.
- [ ] Add strict journal read/write validation, atomic journal state changes, file ID recording/comparison, and per-step revalidation of entity boundary, reparse state, and current old content.
- [ ] Mark New/ManifestNew `applied` only after the recorded sibling temporary identity becomes the target with the expected new hash/length; rollback removes it only when all identity/hash evidence matches.
- [ ] After Replace/Stale/ManifestReplace, verify the backup hash/length equals the expected old content or old manifest bytes; on mismatch atomically restore the backup content and fail without losing user bytes.
- [ ] Re-run the focused race/hardlink and previous installer tests.

### Task 3: Crash Recovery and Committed Cleanup

**Files:**
- Modify: `scripts/Test-InstallDev.ps1`
- Modify: `scripts/Install-Dev.Core.psm1`

**Interfaces:**
- Consumes: strict fixed journal and exclusive lock from Tasks 1-2.
- Produces: startup recovery where `applying` rolls back and `committed-cleanup-pending` only completes cleanup.

- [ ] Add a child-PowerShell test that exits the process immediately after the first successful Replace, then assert the parent sees a durable journal/backup and the next install recovers and succeeds.
- [ ] Add a committed-cleanup failure test that preserves the journal/backup and assert the next install completes cleanup without rolling back committed files.
- [ ] Confirm both tests fail with the current transient journal/finally cleanup behavior.
- [ ] Recover the fixed journal immediately after acquiring the lock: strictly validate schema, owner, roots, all entity paths, reparse state, identities and hashes; rollback incomplete transactions and clean only committed ones.
- [ ] Atomically set `phase=committed-cleanup-pending` before deleting any backup; retain journal and remaining backups on cleanup failure; delete the journal only after cleanup succeeds.
- [ ] Re-run the complete installer test suite and confirm no transaction artifacts remain after successful runs.

### Task 4: Aggregate, Integration, Documentation, and Commit

**Files:**
- Modify: `scripts/Test-Foundation.ps1`
- Modify: `docs/development/foundation.md`
- Modify: `.superpowers/sdd/task-8-report.md` (ignored evidence report)

**Interfaces:**
- Consumes: complete hardened installer suite.

- [ ] Ensure the aggregate invokes the expanded installer suite and document path-entity resolution, exclusive locking, crash recovery, and cleanup-pending behavior.
- [ ] Run script parsing and the complete installer tests; expected exit 0 with explicit SUBST/8.3 pass or supported skip reporting.
- [ ] Run real `Install-Dev.ps1` twice below one unique system-temporary parent; verify strict manifest paths, lengths, SHA-256 values, and exact cleanup.
- [ ] Run `scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro`; expected exit 0 and `Foundation non-GUI verification passed.`
- [ ] Run `git diff --check`, confirm no generated/runtime artifacts are tracked, append evidence while keeping report `PARTIAL / PENDING`, and create one focused commit without rewriting prior commits.
