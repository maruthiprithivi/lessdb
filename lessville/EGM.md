# Lessville EGM blueprint

## Overview
Lessville is a server-authoritative research world observed through Godot, with
isolated local execution and an observer-only Cloudflare deployment.
The working population is five focal residents and 95 supporting residents: 17
in the town centre (5 + 12), with 83 in abstract surrounding districts.
EGM (Elephant-and-Goldfish Model) keeps durable component contracts here and uses
fresh-context source scouts to check them. This bootstrap covers the active town
client and simulation backend. Historical experimental tools retain their own limits.

## Interface
- [Client boundary](scripts/EGM.md): snapshots, town/interior rendering, shared scale and seats.
- [World boundary](backend/EGM.md): authoritative clock, behavior, persistence and HTTP.
- [Daily-life boundary](backend/agentd/src/sim/EGM.md): needs, meals, social contact and practice.
- [Run locally](docs/LOCAL-WORLD.md), [scale contract](docs/WORLD-SCALE.md),
  [learning and evidence log](docs/egm/LEARNING-LOG.md).
- [Release qualification](docs/RELEASE-QUALIFICATION.md) records publication scope,
  checkpoint rehearsal and pending rollout evidence. The embedded engine is
  [vendored with provenance](vendor/lessdb/README.md).
- [Original attribute register](docs/REALISM-IMPLEMENTATION-REGISTER.md) preserves
  supplied claims separately from verified implementation.

## Invariants
- Backend state owns time, activity, consumption and memories. Rendering cannot
  grant nutrition, complete a trip, create a relationship or promote a lesson.
- Visual detail and behavioral depth are separate: the 95 supporting residents
  are adult-sized but do not yet share the focal cast's full memory machinery.
- Every claimed improvement names a source change, evidence, limits and remaining work.
- Keep existing human-maintained READMEs; update these contracts when interfaces,
  rules, persistence or scale change. Do not report old baseline assertions as new tests.
- No automatic promotion of agent-generated proposals. No claim of consciousness,
  validated human physiology or a complete economy.
- Preserve the existing Cloudflare Worker, Container identity and R2 checkpoint
  namespace during this release. Publishing source or passing a local build does
  not prove deployed checkpoint compatibility or a completed rollout.

## Side Effects
Local execution writes LessDB state and runtime logs. Recording writes PNG frames,
JSON snapshots and video. Optional model/checkpoint integrations exist but are disabled
by the local launcher. Documentation bootstrap itself performs no deployment or commit.

## Verification
Backend: `cd backend/agentd && cargo +1.96 test --locked -- --test-threads=2`.
Client: Godot `res://test/test_suite.tscn`; see platform-specific render prerequisites
in [LOCAL-WORLD.md](docs/LOCAL-WORLD.md). Scale: native `tools/render_scale.gd`.
Read the latest evidence log before interpreting a successful build as a clean test run.

## Continuing work
1. Read this blueprint and the affected component contract.
2. Inspect current source; a contract is a navigation aid, not proof.
3. Make the bounded change with a behavioral check and native capture for visual work.
4. Append an evidence-backed entry to the learning log and update changed contracts.
5. Use an EGM fresh-context scout/drift review for architectural or contract changes;
   preserve the actual reviewer findings and resolve discrepancies explicitly.
