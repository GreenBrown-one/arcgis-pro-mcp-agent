# Task 8 Authorized Root Initialization Plan

**Goal:** Allow a first development install to create the missing tail of the explicitly supplied InstallRoot and AddInRoot without weakening any transaction/component/stage boundary.

**Constraints:** Use only unique system-TEMP fixtures. Do not retry a real default installation, use GUI state, or create a tag. Keep Task 8 `PARTIAL / PENDING`.

## TDD sequence

- [x] Add a RED test whose existing TEMP base has multi-segment missing InstallRoot/AddInRoot tails; require a successful double install.
- [x] Add a RED race test that swaps a newly created intermediate tail segment for a junction; require fail-closed behavior and sentinel preservation.
- [x] Add a dedicated authorized-root initializer that resolves the deepest existing ancestor and creates only the explicit root's missing tail, revalidating entity prediction, GIS/reparse state, and each segment before and after creation.
- [x] Call the initializer for InstallRoot/AddInRoot before lock/recovery; leave ordinary component, stale, stage, and sibling paths on their existing boundaries.
- [x] Run focused GREEN, the complete installer suite, default-like TEMP double installation, and the aggregate non-GUI gate.
- [x] Update the ignored local report as `PARTIAL / PENDING` and create one focused commit.
