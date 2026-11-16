## EGM.md

No drift detected.

## scripts/EGM.md

No drift detected.

## scripts/lessville/EGM.md

No drift detected.

## backend/EGM.md

No drift detected.

## backend/agentd/EGM.md

No drift detected.

## backend/agentd/src/EGM.md

- Nit: named the private predicate `visible_in_town` as a snapshot field; actual
  serialized field is `background[].visible`. Resolved in the contract.
- Nit: omitted `scenarios::fixture`, the shared real-LessDB test helper returning
  `World::fresh(1)` without seeding. Resolved with its source link and behavior.

## backend/agentd/src/sim/EGM.md

- Nit: omitted Maya's owner-break exception to café-open/staff checks. Resolved.
- Nit: omitted meal deadline, evening cutoff, unavailable-resource timeout and
  completion/cancellation retry delays. Resolved in the contract.

## docs/WORLD-SCALE.md

- Nit: said five focal +24 supporting =17; actual town membership is five +12.
  Resolved; all 100 people remain simulated.
- Major: claimed an automated adult-height equivalence assertion that the scale
  gallery does not implement. Resolved by accurately separating its frame/bounds/
  ratio assertions from visual silhouette inspection. No new automated rendered
  silhouette equivalence test is claimed.

## docs/LOCAL-WORLD.md

No drift detected.

## docs/TOWN-ART.md

No drift detected within inspected integration claims. Generation provenance and
historical capture evidence were not independently verified by the source reviewer.

## scripts/lessville/EGM.md

No drift detected.

## backend/agentd/src/EGM.md

No drift detected.

## docs/WORLD-SCALE.md

- Major: described the remaining café-path test failure as head-on. The final
  continuation log and oracle report one same-direction follower-overlap tick
  with a −14.9 px margin. Resolved by naming the observed follower failure;
  the preceding head-on encounter remains diagnostic context in the learning log.
- Nit: described vehicle separation using sprite lengths, but the bicycle reserves
  its height. Measured atlas bounds are 558×563 px: rendered width 99.118 px versus
  reserved length 100.006 px. Resolved to “reserved body lengths”; the current
  bicycle reservation is conservative, so no collision fix was required.

## docs/LOCAL-WORLD.md

No drift detected.

## README.md — final portable-command review

- Major: the inherited Run and verify section advertised `npm run check -- --pack`
  and a shared-runtime export. `tools/verify.sh` now accepts only `--export`, and
  `package.json` builds through `tools/export_web.sh`. Resolved by documenting
  `npm run verify:release` and distinguishing historical template guidance.
- Nit: the quick setup omitted Pillow in `/usr/bin/python3` and Linux Xvfb, which
  the current client checks invoke. Resolved with prerequisites and the local guide.

## docs/LOCAL-WORLD.md — final launcher review

- Nit: the hair-render paragraph still required macOS users to provide earlier
  captures. `test/test_suite.gd::_test_lin_profile_hair` now invokes native Godot
  on macOS and Xvfb only on Linux. Resolved; the environment override is optional.

## docs/RELEASE-QUALIFICATION.md — final release-boundary review

- Nit: the portable-check prerequisites omitted Pillow and Linux Xvfb. Resolved
  with the exact interpreter requirement and a link to local setup.

## vendor/lessdb/README.md — final provenance/build review

No drift detected.

## deploy/cloudflare/README.md — final build-boundary review

No drift detected.

## EGM.md — final release-boundary review

No drift detected.

## backend/EGM.md — final vendored-engine boundary review

No drift detected.

## backend/agentd/EGM.md — final vendored-engine boundary review

No drift detected.

## backend/agentd/src/EGM.md — final release-boundary review

No drift detected.

## scripts/EGM.md — final movement-contract review

No drift detected.

## scripts/lessville/EGM.md — final movement-contract review

No drift detected.

## docs/WORLD-SCALE.md — final movement/clearance review

No drift detected.

## backend/agentd/src/EGM.md — weather and JSON checkpoint contract refresh

- Major: omitted the implemented live-weather precedence and persisted Unix-time
  freshness deadline, including dry-rain cleanup, expiry/disabled/legacy fallback
  and bounded observer rain. Resolved with the current rules and focused test link.
- Major: omitted exact resident floating-point fidelity at the persisted World JSON
  boundary. Candidate `38cd159` reproduced a one-ULP restart defect; correction
  `4bc21af` enables `float_roundtrip` and adds the three-roundtrip, 100-resident
  regression. Focused/full native tests, Linux CI `43943678663`, artifact integrity
  and exact-image qualification passed. The formerly failing minute now preserves
  all 99 values exactly. Corrective deployment and live verification remain pending.

## scripts/lessville/EGM.md — weather precedence contract refresh

- Major: omitted `world_atmosphere.gd` and its authority order: explicit rain flag,
  weather kind, then seeded fallback. Resolved with dry-weather suppression,
  supplied kind/temperature/wind behavior and the targeted regression link.
