# Godot client boundary

## Overview
Observer UI and town/interior rendering, shared by the native and Web clients.

## Interface
- [Component contract](lessville/EGM.md) describes source interfaces and ownership.

## Invariants
Keep state ownership, scenario limitations and evidence claims consistent with the
linked child contract. Build artifacts and archived tools are outside this bootstrap.

## Side Effects
See the child contract for runtime I/O. This document adds no execution behavior.

## Verification
`Godot test/test_suite.tscn`. Check the root EGM evidence log for known failures and prerequisites.
