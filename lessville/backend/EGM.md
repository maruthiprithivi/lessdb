# Backend boundary

## Overview
Native authoritative simulation service used locally and by the Cloudflare Container.
The pinned embedded engine lives in `../vendor/lessdb`; it is isolated from the
parent repository's public engine.

## Interface
- [Component contract](agentd/EGM.md) describes source interfaces and ownership.

## Invariants
Keep state ownership, scenario limitations and evidence claims consistent with the
linked child contract. Build artifacts and archived tools are outside this bootstrap.

## Side Effects
See the child contract for runtime I/O. This document adds no execution behavior.

## Verification
`cd agentd && cargo +1.96 test --locked`. Check the root EGM evidence log for known failures and prerequisites.
