# Task 8 Journal Recovery Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan inline. Do not rewrite prior commits.

**Goal:** Close the remaining recovery-validation gaps without changing real installations, GUI state, or tags.

**Architecture:** Strengthen the existing strict journal reader and recovery state machine so no backup or atomic-journal artifact is trusted before stable identity/content/path validation. Make the crash test cleanup derive its exact stage directory only from the case-owned strict journal.

**Tech Stack:** Windows PowerShell 5.1, embedded Win32 file identity/entity path helper, self-contained child-process integration tests.

## Global Constraints

- Preserve commit `dbd3112`; create one focused follow-up commit.
- Use unique verified TEMP cases only; never install to real user directories.
- Preserve every ambiguous or invalid journal/backup/target for manual review.
- Keep status `PARTIAL / PENDING`; do not run GUI smoke or create a tag.

---

### Task 1: Persisted-old Backup Recovery Gate

**Files:** `scripts/Test-InstallDev.ps1`, `scripts/Install-Dev.Core.psm1`

- [x] Add focused child-process RED tests for Replace, ManifestReplace, and Stale exits after the atomic operation but before backup-state persistence; tamper each backup and assert next run fails while target, backup, and journal bytes remain unchanged.
- [x] Require recovery to compare backup SHA-256, length, and file identity to persisted old fields before filling empty backup fields; fail with manual-review guidance on mismatch.
- [x] Run the three focused tests to GREEN.

### Task 2: Atomic Journal Previous Validation and Stage Binding

**Files:** `scripts/Test-InstallDev.ps1`, `scripts/Install-Dev.Core.psm1`

- [x] Add RED tests for invalid previous-only journal, mismatched simultaneous journal/previous files, and cross-transaction or prefixed stage roots with sentinels.
- [x] Strictly parse and stably re-read identity/hash for both journal files before Move/Delete; require matching transaction and permitted phase/state relationship when both exist.
- [x] Bind stageRoot basename exactly to `ArcGISProAgent-install-$transactionId` and require its entity parent to equal the TEMP entity exactly.
- [x] Run focused tests to GREEN.

### Task 3: Case-owned Crash Cleanup

**Files:** `scripts/Test-InstallDev.ps1`

- [x] Replace the global TEMP before/after difference with strict parsing of the current case journal and exact transaction stage derivation.
- [x] Add a concurrent unrelated stage sentinel and prove cleanup preserves it.
- [x] Validate exact basename, TEMP entity parent, and every non-reparse descendant before recursive cleanup.

### Task 4: Verification, Documentation, and Commit

**Files:** `docs/development/foundation.md`, `.superpowers/sdd/task-8-report.md`

- [x] Document the intentionally persistent fixed lock file.
- [x] Run focused RED/GREEN evidence, the complete installer suite, real TEMP main double-install, and final aggregate.
- [x] Run diff/runtime audits and commit a focused follow-up while keeping the report `PARTIAL / PENDING`.
