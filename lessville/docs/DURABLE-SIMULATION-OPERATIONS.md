# Durable simulation operations plan

> **Current status — 2026-11-17:** The corrective rollout passed its final live
> health, continuity and checkpoint gates and retains one `basic` Container. See
> [release qualification](RELEASE-QUALIFICATION.md) for current evidence and costs
> (about $5.22/month extra memory/disk versus `lite`; active CPU additional).
> The older `lite` budgets and delivery evidence below are retained as history,
> not the current deployment estimate. Early capacity headroom is not endurance proof.

**Status:** proposed architecture; not yet deployed or enabled.

This plan answers what is required to move the temporary Reachy Mini, MicroDuck and Security Lab surfaces to durable public services while keeping the existing LessVille town deployment and its cost guardrails honest.

## Executive recommendation

Do **not** move the embodied simulators into the existing `lite` Cloudflare Container. Keep the town on its current Cloudflare Worker/container deployment and run the companion simulator stack as a separate Dockerized service on a persistent host.

The current sandbox process snapshot is not a production benchmark, but the active physical stack collectively used roughly 5 GB RSS and more than two CPU cores across Reachy, MicroDuck, robotd/MuJoCo, swarm and orchestration processes. That makes a single 1/16-vCPU, 256 MiB `lite` instance unsuitable. A practical first durable target is:

- one Docker host with **at least 8 GB RAM and 2 shared CPU units**;
- pinned, reproducible images for Reachy Mini, MicroDuck/robotd, the swarm, the Security Lab and the read-only unified hub;
- a persistent local volume only for the 24-hour operational window, crash spool and deployment state;
- R2 as the durable archive and checkpoint store;
- a reverse proxy with TLS and separate public read-only subdomains or paths;
- restart-on-failure plus semantic health checks; and
- no actuator, robot-control or public mutation route.

This is **not compatible with the current $10/month total infrastructure ceiling**. The smallest credible durable companion host is likely to add about $50/month before storage, egress and monitoring. A strict $10 ceiling requires keeping the companion services temporary/on-demand or reducing them to one very small, non-physical observer service.

## What changes for an always-on production service

### 1. Package the simulations as Docker services

Use Docker Compose or an equivalent orchestrator with immutable image digests. The intended service boundary is:

| Service | Role | Public exposure |
|---|---|---|
| `security-lab` | Six-persona observer-only security/trust simulation | `GET /`, `/health`, `/state.json` only |
| `reachy-sim` | Official Reachy Mini daemon-backed MuJoCo visual | Read-only rendered state and health |
| `microduck-robotd` | Official MicroDuck robotd simulator and camera/body readback | Read-only rendered state and health |
| `microduck-swarm` | Official-model bounded swarm visual | Read-only rendered state and health |
| `embodied-hub` | Shared-coordinate presentation/orchestration layer | Read-only aggregation of the four sources |
| `health-agent` | Internal probe, status store and alert adapter | Never public |

The unified hub must remain an orchestration/projection layer. It must not be described as a merged physics engine, one robot-control loop or a safety interlock.

Each image should have:

- a non-root runtime user;
- pinned base image and dependency versions;
- a read-only root filesystem where practical;
- explicit CPU/memory limits;
- `restart: unless-stopped` or an equivalent supervisor policy;
- a Docker `HEALTHCHECK` that validates the service's own `/health` response;
- a separate semantic probe for `/state.json`; and
- a release marker containing source revision, image digest and model/daemon versions.

### 2. Use a durable front door

Keep `ville.lessdb.dev` as the public LessVille front door. Put the companion services behind separate subdomains such as `security.lessdb.dev` and `embodied.lessdb.dev`, or route them through a Worker that performs no mutation and adds the same security headers.

The public routes should expose only:

- `GET /`;
- `GET /health`;
- `GET /state.json` or an equivalent read-only state endpoint; and
- static rendered frames/media required by the observer UI.

Do not expose local daemon ports, robotd sockets, internal LessDB endpoints, shell access, actuator commands, reconciliation controls or staging backends.

### 3. Add restart and recovery behavior

The host should recover automatically from a process crash, host restart and image replacement:

1. Supervisor restarts only the failed service.
2. The service returns `503` until its official simulator/daemon dependency is ready.
3. The semantic health check stays unhealthy until the rendered state and required source are valid.
4. The unified hub reports a partial-degradation state rather than fabricating a healthy source.
5. The archive/checkpoint worker resumes from its last verified manifest.
6. A release rollback is available by image digest, not by rebuilding an unpinned `latest` image.

A single host is still a single failure domain. It can be durable and self-restarting without being highly available. True HA requires at least two hosts or replicas, shared durable state, health-based routing and a tested failover procedure; that would exceed the current budget by a much larger margin.

## Retention and archive policy

The requested policy is feasible, but it must be implemented as an application-owned archive pipeline. R2 lifecycle rules cannot move data out of the local embedded LessDB directory by themselves.

### Data tiers

| Age | Location | Policy |
|---|---|---|
| 0–24 hours | Local embedded LessDB and local spool | Hot operational data for live UI, recovery and recent diagnostics. Keep bounded and compacted. |
| >24 hours to 30 days | R2 Standard | Upload closed, immutable daily partitions or compressed event bundles after hash verification. Delete the local copy only after a successful R2 `HEAD`/checksum verification. |
| 30 days to 12 months | R2 Infrequent Access | Use an R2 lifecycle transition or explicit copy to `STANDARD_IA`. This lowers storage price but has retrieval fees and a 30-day minimum billable duration. |
| >12 months | Logical archive prefix/bucket, compressed and immutable | Keep monthly or quarterly archives with manifests, schema version, time range, row counts and SHA-256. R2 currently has Standard and Infrequent Access classes, not a separate Glacier-like archive class; a true colder tier would require another provider and explicit approval. |

“Archived” after 12 months therefore means **removed from the live query path and retained in a separately indexed, compressed archive**. It does not currently mean a cheaper Cloudflare R2 storage class. Do not delete the >12-month archive unless a separate retention/deletion policy is approved.

### Archive object format

Each archive should contain:

- compressed newline-delimited records or a compatible columnar format;
- a manifest with `schema_version`, `source_revision`, `first_event_at`, `last_event_at`, `row_count`, `byte_count`, `sha256` and `retention_until`;
- separate namespaces for LessVille town state, security evidence, physical observations and operational health events;
- no raw hostile payload in model prompts or alert messages;
- no user identity, credentials, account identifiers or internal keys; and
- an index row or manifest that lets an approved operator locate the object without scanning the entire bucket.

The archive job must be idempotent. A retry of the same partition must produce the same object key and either the same checksum or a clearly versioned replacement. Never delete local data before the remote object and manifest are verified.

### R2 cost implications

Cloudflare R2 currently documents:

- Standard: `$0.015/GB-month`;
- Infrequent Access: `$0.01/GB-month`;
- 10 GB-month of Standard storage free each month;
- no egress charge;
- no free tier for Infrequent Access;
- Infrequent Access retrieval at `$0.01/GB`; and
- a 30-day minimum storage duration for Infrequent Access objects.

For LessVille-scale compressed daily archives, storage is likely to remain small compared with compute, but the actual byte rate must be measured after the archiver runs. Many tiny objects can also create unnecessary Class A operations, so daily or monthly bundles are preferable to one object per event.

R2 lifecycle rules can transition objects to Infrequent Access by age and can expire objects, but they do not replace the local-to-R2 archive worker. Encryption-at-rest configuration must be verified separately before making a security claim; this plan does not claim a new encryption setting has been enabled.

## Cost scenarios

These are planning estimates, not an invoice. They use the published rates available on 2026-10-12 and exclude unverified account usage, taxes, egress, domain changes, alert destinations and any paid support.

### Existing town deployment

The current town deployment remains approximately **$9.50/month as a designed upper-bound estimate before unrelated account usage**, based on the existing `lite` Container, Workers Paid base and current R2/AI guardrails. It is not a verified invoice amount.

### Cloudflare Container alternatives

Cloudflare Containers currently includes 25 GiB-hours of memory, 375 vCPU-minutes and 200 GB-hours of disk in the Workers Paid plan. CPU is charged by active use, while provisioned memory and disk are charged while the instance runs.

| Instance | Resources | Approx. 30-day total including $5 Workers Paid base* | Suitability |
|---|---:|---:|---|
| `lite` | 1/16 vCPU, 256 MiB, 2 GB | `$9.50` at full allocation | Current town only; not embodied stack |
| `basic` | 1/4 vCPU, 1 GiB, 4 GB | `$24.44` at full CPU | Too small for all physical sources |
| `standard-1` | 1/2 vCPU, 4 GiB, 8 GB | `$57.57` at full CPU | Possible security lab or reduced stack; CPU likely tight |
| `standard-2` | 1 vCPU, 6 GiB, 12 GB | `$97.17` at full CPU | Memory plausible; CPU likely tight for all current sources |
| `standard-3` | 2 vCPU, 8 GiB, 16 GB | `$162.70` at full CPU | More credible single-container physical stack; far above budget |

\* The CPU portion is an upper bound because Cloudflare bills CPU by active usage. Worker, Durable Object, logging, R2 and egress usage are additional. If the companion stack is placed in a second Container while the town remains on `lite`, the costs add; they do not replace the town's cost.

Cloudflare is attractive for the current town front door, but a 24/7 physical simulation stack on a single Cloudflare Container would be expensive and would still be a single failure domain. It also needs careful validation of daemon startup, image size, CPU pressure and container sleep behavior.

### Docker VM alternative

Fly.io currently lists approximately `$13.39/month` for a continuously running shared-CPU 2x machine with 2 GB RAM, plus `$6/GB-month` for additional RAM. An 8 GB shared-CPU 2x machine is therefore approximately `$49.39/month` before volume and egress. Region pricing varies.

A single 8 GB host is the most economical credible first target for the current stack, but the measured sandbox footprint means it still needs a load test and hard memory limits. Keeping the existing town deployment plus this host would put the designed baseline around **$59/month before volume, egress and monitoring**.

A small conventional Docker VPS may be cheaper, but it introduces provider selection, billing, backups, patching, TLS, incident response and credential management. No new provider or budget has been approved by this plan.

### Reserved WebDev host

The available Reserved Hosting reference provides one persistent 1-vCPU/512-MB instance with a full-utilization ceiling of about `$37.50/month` before the included `$10` monthly usage credit, plus egress and volume. That is suitable for a small security observer or health service, not the current Reachy/MicroDuck stack whose observed sandbox processes collectively exceed that memory by a wide margin.

## Automated health checks and alerts

### Probe contract

Every service should expose a small public health response with no sensitive state:

```json
{"ok":true,"service":"microduck-robotd","observer_only":true}
```

The internal semantic probe should additionally validate:

- HTTP status is `200`;
- response is valid JSON where expected;
- service identity matches the expected target;
- `observer_only` is true;
- required simulator/daemon source is connected;
- frame or state sequence is advancing;
- timestamps are not stale;
- the unified hub reports the expected sources, or explicitly reports partial degradation;
- LessVille `/api/health` reports public mode and no restore/checkpoint failure; and
- LessVille `/api/state` contains the authoritative fields expected by the current client, including traffic lights and policy-proposal schema where applicable.

Health probes must store only a bounded result: target, check time, latency, status, failure class, source revision and response hash. Do not store complete hostile payloads or raw response bodies in alert messages.

### Recommended cadence and thresholds

- Probe every **5 minutes** from a durable Worker schedule or the health-agent host.
- Use a 10-second request timeout and two immediate retries.
- Alert after **two consecutive failed probe cycles** for a service.
- Resolve after **two consecutive successful cycles**.
- Treat a dependency that is still booting as `degraded`, not as a false healthy state.
- Run a deeper state/telemetry check every 15 minutes and a full synthetic read-only journey daily.
- Keep 24 hours of probe results locally, then archive daily summaries to R2 under the same retention policy.

This gives a practical detection window of roughly 5–15 minutes without making a 99.99% availability claim from a single host.

### Cloudflare-native options

1. **Cloudflare Health Checks:** appropriate for durable origin hostnames and supports expected status codes, response body matching, retries, consecutive-failure thresholds, regions and analytics. Cloudflare documents Standalone Health Checks as unavailable on Free, with 10 checks on Pro. The current LessVille zone is recorded as Free, so an account/zone plan change would need to be confirmed before using this route. Health Check notifications can send email and webhooks to Slack, Google Chat, Microsoft Teams or an internal/SIEM endpoint; PagerDuty integration is documented for Enterprise.
2. **Existing Worker Cron Trigger:** add a read-only scheduled probe to the existing Worker, persist only summarized results in a Durable Object, and send an alert to a user-selected webhook. Cron triggers run in UTC and the current Worker already has a five-minute scheduled keep-alive. This avoids adding another always-on monitor, but Worker/DO/logging usage must still be watched and a webhook destination must be supplied.
3. **Host-local Docker health checks:** use Docker `HEALTHCHECK` plus the supervisor to restart failed processes. This is the recovery layer, not the independent alert layer; a dead host cannot report its own failure, so at least one external monitor is still required.

### Monitoring layers

- **Recovery:** Docker restart policy and a host supervisor.
- **Detection:** Cloudflare Health Check or external Worker Cron probe.
- **Evidence:** structured JSON logs and bounded status records; use Cloudflare Workers Logs/Observability for the Worker and Prometheus-compatible metrics only if a durable host is provisioned.
- **Alerting:** email or a user-selected webhook; avoid adding ELK/Grafana before the cost and retention budget is approved.
- **Incident response:** documented rollback, host restart, R2 restore and staging-session replacement procedures.

## Phased transition

### Phase A — no new hosting spend

- Implement the 24-hour local spool and verified R2 archive worker against a fresh private store.
- Add semantic `/health` and `/state.json` tests for all current services.
- Add the probe result schema and a local Docker Compose rehearsal.
- Measure compressed bytes/day, peak RSS, CPU and frame latency for 24–72 hours.
- Keep public staging unchanged.

### Phase B — durable security observer

- Move only the Security Lab and health-agent to a persistent Docker host.
- Add a durable hostname and read-only proxy.
- Enable restart, structured logs, bounded R2 archival and alerting after the destination is chosen.
- Do not move Reachy/MicroDuck until the host passes the resource and visual-state soak test.

### Phase C — durable embodied stack

- Deploy official daemon/model dependencies as pinned Docker services.
- Run a 7-day soak test with memory, CPU, frame freshness, source availability and archive verification.
- Expose the unified hub only after all source boundaries and public routes are rechecked.
- Retain one-click rollback to the last known-good image digest.

### Phase D — HA only if justified

- Add a second host or replica, shared archive/checkpoint coordination and health-based routing.
- Test failure of one host, one daemon, R2 unavailability and partial source loss.
- Recalculate the budget; do not call a single-host service “high availability.”

## Decision required before implementation

Two choices materially change the design:

1. **Budget:** preserve the existing `$10/month` total ceiling, or approve a separate durable-host budget of roughly `$50–$70/month` for the first embodied host, before monitoring and egress.
2. **Alert destination:** Cloudflare email, Slack/Teams/Google Chat webhook, or another approved endpoint. A webhook secret must be entered through the provider's secure configuration, never committed to the repository or sent in chat.

Until those choices are made, no durable migration, new provider, billing change or alert trigger should be enabled.

## Sources

- [Cloudflare Containers pricing](https://developers.cloudflare.com/containers/platform/pricing/)
- [Cloudflare Health Checks](https://developers.cloudflare.com/health-checks/)
- [Cloudflare Health Checks API](https://developers.cloudflare.com/api/resources/healthchecks/)
- [Cloudflare Cron Triggers](https://developers.cloudflare.com/workers/configuration/cron-triggers/)
- [Cloudflare Health Check notifications](https://blog.cloudflare.com/get-updates-on-the-health-of-your-origin-where-you-need-them/)
- [Cloudflare R2 pricing](https://developers.cloudflare.com/r2/pricing/)
- [Cloudflare R2 storage classes](https://developers.cloudflare.com/r2/buckets/storage-classes/)
- [Cloudflare R2 object lifecycles](https://developers.cloudflare.com/r2/buckets/object-lifecycles/)
- [Fly.io resource pricing](https://docs.fly.io/about/pricing/)
