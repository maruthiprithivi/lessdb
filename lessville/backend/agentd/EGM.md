# Agent service boundary

## Overview
Axum service with embedded LessDB, used locally and by the Cloudflare Container.
Cargo path dependencies resolve to `../../vendor/lessdb`; its provenance records
the qualified engine bytes and existing upstream worktree patch.

## Interface
- [Component contract](src/EGM.md) describes source interfaces and ownership.

## Invariants
Keep state ownership, scenario limitations and evidence claims consistent with the
linked child contract. Build artifacts and archived tools are outside this bootstrap.

## Side Effects
See the child contract for runtime I/O. This document adds no execution behavior.

## Verification
`cargo +1.96 test --locked`. Check the root EGM evidence log for known failures and prerequisites.
