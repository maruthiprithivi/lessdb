# LessDB + LessVille next action plan and roadmap

> **Current status — 2026-11-17:** The corrective rollout passed its final live
> health, continuity and checkpoint gates and retains one `basic` Container. See
> [release qualification](RELEASE-QUALIFICATION.md) for current evidence and costs
> (about $5.22/month extra memory/disk versus `lite`; active CPU additional).
> The older `lite` budgets and delivery evidence below are retained as history,
> not the current deployment estimate. Early capacity headroom is not endurance proof.

**Status date:** 2026-10-12

## 1. Cloudflare cost position

The exact Cloudflare invoice or current-period billed total was **not available through the configured API token**. The account listing and resource APIs work, but the billing profile, billing history and billable-usage endpoints all returned `403 Authentication error`. This means there is no verified current billed amount to report from automation. A token with Cloudflare billing-read permission or a billing-dashboard review is required for that number.

The verified LessVille budget model remains:

| Component | Current guardrail or allowance | Budget meaning |
|---|---:|---|
| Workers Paid base | $5/month minimum | The deployment model assumes the paid plan needed by Containers. Actual subscription state still needs billing-read verification. |
| Lite Container | 1/16 vCPU, 256 MiB, 2 GB | If continuously active for 30 days, the current published-rate model is approximately $2.79 CPU + $1.40 memory + $0.31 disk, or about $4.50. Scale-to-zero should reduce this when idle. |
| Workers AI | Hard cap: 9,500 neurons per UTC day | Cloudflare documents 10,000 neurons/day free allocation, so the configured LessVille cap is designed to remain below the free allocation. This does not prove actual usage or account billing. |
| R2 | Latest checkpoint plus 168 hourly archives | Standard R2 includes 10 GB-month, 1M Class A operations and 10M Class B operations monthly; egress is free. The `lessville-data` bucket exists, but object-byte totals were not readable in this session because the R2 S3 secret variables were unavailable. |
| Worker / Pages / Durable Object | Observer traffic and one global budget object | Standard Workers Paid includes 10M Worker requests and 30M CPU-ms monthly; the budget Durable Object is expected to be small, but exact usage requires billing/analytics access. |

Therefore the honest current statement is:

> **Designed ceiling:** approximately **US$9.50/month before unrelated Cloudflare account usage**, assuming a continuously active lite Container and the $5 Workers Paid base. **Actual incurred cost: not verified yet.**

### Embodied-world milestone — 2026-10-11

Reachy Mini and the four-body MicroDuck visual/daemon paths are now composed by an observer-only shared-coordinate orchestrator. The common frame is `lessville:embodied-district/v1`; source physics and daemon control remain separate. LessDB telemetry exposes bounded frame IDs, sequence windows, explicit/inferred shared-context scopes, and persisted observation/proposal/reconciliation counts through the private `/api/physical/analytics` endpoint.

This is a **shared-world coordination view**, not a merged MuJoCo physics scene and not a sim-to-real safety claim. A temporary public staging URL is used for review; production `ville.lessdb.dev` is unchanged.

The live LessVille health endpoint confirms public observer mode. The Cloudflare API confirms the R2 bucket exists. Neither result is an invoice.

### Latest live guardrail check — 2026-10-11

The public health response currently reports:

| Signal | Observed value | Assessment |
|---|---:|---|
| Public mode | `true` | Observer-only runtime remains enabled. |
| Workers AI accepted calls | 2,173 | Calls are occurring under the configured single-purpose path. |
| Workers AI rejected calls | 348 | Validation rejected outputs; no failed provider calls were reported. |
| Workers AI failed calls | 0 | No provider-call failures reported by the service. |
| Workers AI skipped calls | 1,259 | Rule-based fallback/skip path is active. |
| Neuron budget used | 966.95 / 9,500 | About 10.2% of the configured daily cap at the check time; this is service-reported usage, not an invoice. |
| Checkpoint | enabled; 300 s interval | R2 restore succeeded; latest report was 239,829 bytes restored and 494,280 bytes in the last successful archive. |
| Checkpoint uploads/failures | 488 / 0 | No checkpoint failures reported by the service at the check time. |
| R2 S3 aggregate probe | unavailable (`InvalidArgument`, HTTP 400) | No object-byte total is claimed from this session. |

The Cloudflare account list endpoint succeeded, but billing profile, history, usage and pay-as-you-go endpoints returned **403 Authentication error** for the configured token. Therefore the exact current invoice remains unverified. The designed ceiling remains an estimate, not incurred cost.

Official pricing references: [Workers](https://developers.cloudflare.com/workers/platform/pricing/), [Containers](https://developers.cloudflare.com/containers/platform/pricing/), [Workers AI](https://developers.cloudflare.com/workers-ai/platform/pricing/), and [R2](https://developers.cloudflare.com/r2/pricing/).

## 2. Immediate action items

### Completed locally — 2026-10-12

- LessVille now has bounded multi-modal traffic and optional hourly Open-Meteo
  weather with deterministic fallback and visible provenance.
- The x4/x8 harness was hardened against occupied ports and stale child services,
  then passed against a freshly built executable. This validates invariants, not
  capacity, frame rate or physical realism.
- Phase 3A versioned spatial-frame types and SC-03 stale/expired observation
  coverage remain the reference contract for the next native-agent work.
- SC-04 now provides an executable zero-day-style scope-isolation regression:
  hostile content remains in its granted scope and is not copied into an
  ungranted swarm scope.
- The authoritative traffic model and Godot renderer now share deterministic
  signal phases; x1/x4/x8 chaos checks passed route, identity, separation and
  red/amber-crossing invariants.
- Reachy Mini and MicroDuck proposal-only injection passed against private
  staging with duplicate-safe observations and denied reconciliation. No robot
  or actuator path was invoked.
- Bounded security observation capture is available for deterministic quadrant,
  trust and event projections. The latest short capture showed a blocked
  compound-path verdict but no event-count increase; it is evidence collection,
  not a claim of autonomous self-learning.
- The incubating `less-agent` contract now includes a versioned
  `lessdb-policy-proposal/v1` candidate with source assessments, reason,
  approval state and an explicit `auto_apply=false` boundary. A fresh optimized
  private HTTP replay produced one candidate after two recurring blocked
  compound-path assessments while leaving every guardrail unchanged.

### Next P1 — Phase 3B agent-native integration

1. Add a versioned `observe`/`recall`/`explain` adapter above `less-agent` so
   spatial-frame validity and shared-scope grants are one typed path rather than
   app-specific JSON in LessVille.
2. Add the agent-native meta-plane: catalog stable primitives, compose bounded
   context recipes and memory lenses, propose logical evolution, simulate it
   against replay evidence, then activate or roll back only through approval.
3. Persist candidate/accepted/omitted recall decisions with frame, scope, policy
   version and reason; keep native row ACL and safety claims explicitly out of
   scope until implemented and tested.
4. Extend SC-04 to assert cross-agent grants, expiry and least-privilege shared
   context, then expose the same evidence in the Mind panel and the embodied-world
   analytics view.
5. Add an offline/replay fixture for delayed spatial observations and transform
   replacement, including deterministic conflict explanations.

### P0 — obtain real cost evidence

1. Grant the existing Cloudflare API token the minimum **Billing Read** permission, or review the Cloudflare billing dashboard manually. Do not grant write, account ownership or security permissions just for reporting.
2. Capture current billing-period totals by service: Workers/Pages, Containers, Durable Objects, Workers AI, R2 and any unrelated account usage.
3. Compare actual usage with the $10 LessVille envelope. If unrelated account usage is included in the same billing account, report it separately from LessVille.
4. Add a monthly cost-evidence record to the delivery notes with the billing period, service totals and retrieval date. Never store tokens or account identifiers in the repository.

### P0 — preserve operational safety

1. Keep `PUBLIC` observer mode enabled; no public pause, reset, nudge or write controls.
2. Keep one lite Container instance, 300-second checkpoints, 168 hourly R2 archives and the 9,500-neuron daily cap.
3. Run one restore drill from the latest R2 checkpoint after a temporary data-directory wipe, then verify the newest world snapshot, lifecycle rows and object-transition provenance.
4. Confirm R2 retention pruning remains bounded and that the latest checkpoint is never pruned with the hourly archive set.

### P1 — finish the conformance harness

1. Implement executable SC-03 through SC-06 fixtures: temporal validity/expiry, cross-agent scoped sharing, provenance explanation and physical observation/proposal flow.
2. Make each fixture assert four views: world state, LessDB rows, context packet/omission reasons and rendered observer state.
3. Run the fixtures in CI or a reproducible local command without connecting to production data.
4. Add a release gate: backend tests, scenario fixtures, Godot checks, public privacy scan, route/link audit and one live health check.

## 3. Product roadmap

### Phase A — operational truth and adoption clarity (now to 2 weeks)

**LessVille:** complete the SC-03–SC-06 scenario runner, add a visible “deterministic policy / model phrasing / persisted provenance” legend, and expose checkpoint age, budget usage and restore status in the observer UI.

**LessDB:** keep `less-agent` as an incubation crate, but make its contract the shared vocabulary for LessVille: `AgentIdentity`, `Situation`, `RecallIntent`, `ContextPacket`, typed provenance and omission reasons. Add a small `observe`, `recall`, `explain` and `remember` adapter surface without claiming native row ACL or automatic awareness.

**Adoption:** publish one click-through scenario for each initial archetype: software factory, family assistant, personal chat assistant, organization/community workspace and physical-AI edge observer.

**Exit gate:** a new user can see what was stored, what was recalled, why an item was omitted, whether a policy was deterministic, and which guarantees belong to the application rather than LessDB core.

### Phase B — native agent interaction primitives (2–6 weeks)

1. Promote the RFC into a versioned agent-session API above storage, not a raw SQL connection wrapper.
2. Define explicit scopes: agent, user, team, organization, community, device and world/object scope.
3. Add temporal semantics: valid-from, valid-until, observed-at, asserted-at, superseded-by and retention class.
4. Add native explainability records for recall decisions: candidate, accepted, rejected, reason, policy version and source provenance.
5. Separate proposal from actuation. LessDB may persist an observation or action proposal; the owning controller must approve and execute physical actions.
6. Define encryption-at-rest responsibilities for local disks, R2 checkpoints, backups and edge caches. Verify provider controls before documenting them as guarantees.

**Exit gate:** the same typed contract works for a single-user assistant and a multi-agent organization without silently widening visibility.

### Phase C — scenario-driven scale and governance (6–12 weeks)

1. Build a conformance matrix for the deployment blueprints already documented: local assistant, family assistant, software factory, organization, community and physical edge.
2. Add deterministic replay from event/provenance logs so a user can reproduce why an agent saw a context packet and why a later decision changed.
3. Stress WAL recovery, concurrent readers/writers, checkpoint restore, bounded recall and long-running temporal retention.
4. Specify native governance candidates only after evidence: scope-aware grants, immutable context-access audit, policy evaluation, quotas and approval envelopes.
5. Add cost-aware retention policies: hot LessDB state, compressed R2 history, archive expiration and explicit deletion/revocation semantics.

**Exit gate:** a workload can be replayed and audited across agent identity, scope, time, storage tier and action proposal without overstating isolation or durability.

### Phase D — physical-AI and ecosystem adapters (12+ weeks)

1. Provide edge adapters for robot/device observations with timestamp, pose/frame, uncertainty, sensor identity and calibration metadata.
2. Support disconnected operation with bounded local queues, signed reconciliation and conflict explanations.
3. Add safety-controller integration contracts, but keep emergency stops, interlocks and motion authorization outside LessDB.
4. Add SDK examples for Python, Node/Rust and embedded/edge clients, with the same context contract and scenario fixtures.
5. Publish reference deployments for home, factory, logistics, field service and community coordination.

**Exit gate:** physical-AI examples demonstrate useful memory and provenance while clearly separating database facts, uncertain observations, proposed actions and safety-critical control.

## 4. Recommended execution order

1. Obtain billing-read evidence and record the real Cloudflare total.
2. Run the R2 restore and retention drill.
3. Finish SC-03–SC-06 and the four-view acceptance oracle.
4. Add the LessVille cost/checkpoint/budget observer panel.
5. Version the LessDB agent-session RFC and integrate the incubator contract.
6. Implement temporal lifecycle and explainable recall primitives.
7. Add governance only where conformance tests demonstrate the need and behavior.
8. Expand physical-AI adapters after replay, uncertainty and proposal-first actuation are stable.

This order keeps the public simulation believable, keeps the infrastructure below the approved envelope by design, and avoids presenting application-layer coordination as a native LessDB guarantee before it has been implemented and tested.

## 5. New staging milestones — 2026-10-12

The physical-simulation gap is now closed at the staging level: Reachy Mini has an official daemon-backed view, while MicroDuck exposes the official four-body model, real `robotd --sim` camera and daemon-driven operator view as distinct sources in one shared-coordinate hub. The hub is an orchestration/projection layer, not a merged physics engine.

A separate Security Lab now runs six named personas through four knowledge quadrants. It persists bounded security events, guardrail verdicts and decisions in embedded LessDB, reloads its versioned scenario snapshot, and visibly distinguishes observed, blocked and dangerous compound paths. The next product step is to promote the security envelope and omission explanations into the incubating `less-agent` contract rather than treating the current API as native authorization.

### Production hardening rollout — 2026-10-12

The traffic-light/world-hardening slice is now deployed to the existing
observer-only `ville.lessdb.dev` runtime from source checkpoint `8640f5d`.
Post-rollout state exposed two authoritative signals, six traffic actors and
eight background residents. The refreshed content-addressed PCK was verified
live from R2, and public control, security-step and physical-replay writes all
remained blocked with HTTP 403. Reachy Mini, MicroDuck and Security Lab remain
temporary private/staging surfaces and were not deployed to production.

### Approval-gated policy proposal rollout — 2026-10-12

The deterministic policy-proposal contract and recurring blocked-pattern
aggregation are now deployed from release commit `d368913` (backend source
checkpoint `81cd481`). Private replay
verified two assessment references, `state=Candidate`,
`requires_approval=true`, `auto_apply=false` and unchanged guardrails. The
public `/api/security/state` read schema includes `policy_proposals`, but the
list is empty in public mode because all security mutation routes remain
blocked. This deploys evidence and schema support, not autonomous learning or
automatic policy change.

### World-coupled developmental-agent research — 2026-10-18

The Alicization-inspired research slice reframes character growth as a world-coupled developmental loop: encounter, attention, memory, interpretation, contestation, commitment, action, consequence, repair and legacy. This is a philosophical and fictional design lens, not a claim that current models are conscious or that LessVille residents are persons. The sourced brief is `docs/ALICIZATION-AND-AGENT-EVOLUTION-RESEARCH.md`.

The first implementation slice is complete locally and is not yet deployed: LessDB's incubating `less-agent` crate has `lessdb-learning-lesson/v1`; LessVille persists idempotent `lv_lessons`; bounded nightly model reflections create candidate lessons only; repeated deterministic bus evidence can create an accepted lesson; a later commute decision cites that lesson; and the Mind panel shows stage, evidence, confidence and the application boundary. The focused LessDB contract suite passed 15 tests, and the LessVille backend suite passed 21 tests with 1 simulator-file integration test ignored by design. Godot parsing and the native project check also passed, while the known visual-coverage metadata remains incomplete.

The next research gap is not a larger score. It is explicit contestation and repair: contradictory lessons, relationship edges with consent and reciprocity, counterfactual replay, identity-thread forks/resets, and teaching grants that never copy context implicitly. Those remain application/incubation candidates until conformance evidence exists; no native ACL, consciousness, formal verification or physical authorization is claimed.

### Consent and contestation hardening — 2026-10-18

The next bounded slice is complete locally and remains undeployed. The incubating `less-agent` vocabulary now includes `lessdb-relationship/v1` and `lessdb-lesson-challenge/v1`, both requiring evidence and rejecting automatic permission or policy changes. LessVille persists `lv_relationships` and `lv_lesson_challenges`, records Lin's café preference grant and later revocation as separate relationship versions, and records a contradicted commute lesson plus an explicitly repaired replacement. Read-only agent detail and the Mind panel expose the evidence, consent state, challenge resolution and boundary language.

The focused typed-contract suite passed 17 tests; the LessVille backend passed 22 tests with one simulator-file integration test ignored by design; the six adoption scenarios pass when run against the rebuilt growth target; and Godot parsing/native project checks pass. This is application/incubation behavior above LessDB storage, not native ACL, a global trust score, consciousness, formal verification or physical authorization. Production still serves the previous committed release until a separate release build, schema migration review and live observer-only verification are completed.

### Teaching and counterfactual replay — 2026-10-18

The next bounded slice is complete locally and remains undeployed. The incubating `less-agent` vocabulary now includes `lessdb-teaching-grant/v1` and `lessdb-counterfactual/v1`. LessVille persists `lv_teaching_grants` and `lv_counterfactuals`: an active grant can present one teacher lesson to one recipient for a bounded purpose and time window, a later revocation suppresses recall without copying or deleting the source lesson, and a replay compares baseline and lesson-aware actions with `not_applied=true`.

The read-only agent detail and Mind panel expose grant purpose, validity, state, baseline/learned branches and evidence. The typed contract suite passed 19 tests; the LessVille backend passed 23 tests with one simulator-file integration test ignored by design; seven adoption scenarios pass; and Godot parsing/native project checks pass. This remains application/incubation behavior above LessDB storage, not native ACL, automatic awareness, policy execution or physical authorization. Production still serves the previous committed release until a separate release build, schema migration review and live observer-only verification are completed.


### Bounded-emergence pressure test — 2026-10-18

The next research phase is implemented locally and remains undeployed. LessVille now uses a persisted world seed to generate bounded, replayable novelty opportunities across social context, weather/route composition, expiring shared context and skill evidence. Each opportunity produces a LessDB event, temporary memory, deterministic decision, frontier gap signal and inactive evolution proposal. The observer DB panel exposes the replay key, capability gap, recurrence and proposal ID.

The executable bounded-emergence scenario proves same-seed replay, different-seed variation, no scope widening and `auto_apply=false` proposal state. The LessVille backend passed 25 tests with one simulator-file integration test ignored by design; traffic chaos passed at x1/x4/x8 with 15 samples; world stability passed at x4/x8; the physical adapter passed; Godot parsing/native checks passed; Python tooling and the reusable security skill validator passed. The first storage blocker—an unsigned replay seed exceeding signed `Int64`—was preserved as a documented gap and fixed with a bounded storage projection plus full replay key.

This is controlled pseudo-random application behavior, not unrestricted self-modification, consciousness or autonomous schema migration. Production still serves the prior release pending a separate release build, schema review and live observer-only verification.


### Agent-session adapter proving ground — 2026-10-18

The first transport adapter is implemented locally and remains undeployed. LessVille exposes `catalog`, `compose`, `explain`, `propose` and `simulate` using the incubating `lessdb-context-recipe/v1`, `lessdb-frontier-signal/v1`, `lessdb-evolution-proposal/v1` and `lessdb-counterfactual/v1` vocabulary. Compose returns bounded context with explicit scope, temporal and budget omissions; propose accepts only evidence-backed candidate declarative changes with migration/rollback references and `auto_apply=false`; simulate returns a non-executing counterfactual. Private HTTP replay passed all five operations; public mode exposes only read-only catalog/explanation behavior and rejects compose/propose/simulate with HTTP 403. Production remains on the previous release.

### Agent-session adapter production proving ground — 2026-10-18

The first proposal-only agent-session adapter is now deployed from source
checkpoint `de3814c` in the existing observer-only `ville.lessdb.dev` runtime.
The live catalog returned `lessdb-agent-session-catalog/v1` over HTTP 200 and
described bounded catalog, compose, explain, propose, simulate and activation
operations. Public compose, propose and simulate writes remained HTTP 403;
activation is not exposed as a public execution path. This is a production
read-only proving ground for the incubating application adapter, not native
LessDB ACL, automatic awareness, schema self-modification, formal verification
or actuator authorization. The deployment used binary SHA-256
`44df04f4545143cfe45b9dc47ee3f83b2a1a34562b704296821113100a66ff1b` and
preserved the existing 9,500-neuron, checkpoint and observer-only guardrails.


### Character competency and reciprocity production proving ground — 2026-10-18

LessVille now records successful café handoffs as reciprocal interaction evidence and accumulates bounded per-domain competency projections from repeated outcomes. The incubating vocabulary is `lessdb-social-interaction/v1` plus `lessdb-competency/v1`; the application ledgers are `lv_interactions` and `lv_competencies`. The read-only agent detail and Mind panel expose action → response, reciprocity, evidence, status, level, policy version and the explicit `auto_apply=0` boundary.

The optimized source checkpoint `084d44a` passed 24 typed LessDB contract tests, 29 LessVille backend tests with one external simulator-file test ignored by design, all ten adoption scenarios, physical adapter regression, traffic chaos, x4/x8 world stability, Godot/project checks, Python tooling and a private HTTP replay through the real service path. The live catalog lists both developmental contracts, the live agent detail exposes `competencies` and `interactions`, state restored from R2, the 9,500-neuron cap is live, and public mutation-like routes remain 403. Production is back to one `lite` instance after a temporary bounded overlap used to rebind the stale Durable Object singleton.

This is a bounded application/incubation projection, not consciousness, a global trust score, native access control, automatic policy change or physical authorization. Next character research pressure tests are identity-thread fork/reset history, competency decay/transfer and multi-step mixed-outcome dialogue.


## Character evolution milestone — identity continuity and competency state

LessVille's next local slice adds `lessdb-identity-thread/v1` and `lessdb-competency-state/v1` incubation contracts. Root/fork/reset records preserve character continuity evidence without implying consciousness; competency support, contradiction and decay events preserve prior/next bounded levels and source references without auto-applying permissions or policies. The read-only Mind/API surfaces and executable replay cover the behavior locally. Transfer, multi-step dialogue and production release remain open follow-up work pending their own gates.


### Identity continuity and competency-state production rollout — 2026-10-19

The identity and competency-state proving ground is now live in the observer-only LessVille runtime. The optimized backend from source checkpoint `f9b1810` is deployed with `lessdb-identity-thread/v1` and `lessdb-competency-state/v1`; the live catalog and agent detail expose the read-only projections, and the selected Maya detail showed one identity-root record at verification time. The refreshed Godot Web PCK containing the continuity and competency-state Mind panels is published at its content-addressed R2 path and referenced by the live root HTML.

The rollout passed 30 release-mode backend tests with one simulator-file test ignored by design, traffic chaos, x4/x8 world stability, physical adapter, Godot/parser/project, Python tooling and live HTTP checks. A temporary two-instance handover was reduced back to one lite instance. This remains an application/incubation proving ground: identity lineage is continuity evidence, competency state is bounded evidence, and neither implies consciousness, native ACL, automatic policy change or physical authorization. Transfer, multi-step dialogue and richer mixed-outcome interactions remain the next research pressure tests.
