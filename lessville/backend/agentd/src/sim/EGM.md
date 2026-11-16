# Daily-life component contract

## Overview
[daily_life.rs](daily_life.rs) extends the parent simulation with bounded needs,
meals, conversations and practice. Rates are scenario parameters, not physiology.

## Interface
`Life`, `Meal`, `Conversation` are serialized state. Parent integration uses
`Life::minute` and `Sim::{life_busy,supply_meals,tick_life}`. Parent:
[world service](../EGM.md).

## Invariants
- Sleep reduces fatigue; hunger/fatigue/social need stay within scenario bounds.
- Hunger ≥60 can create a meal intent. Arrival, food and cafe table/staff availability
  precede fifteen minutes of eating. Reserve stock once; completion lowers hunger
  by 55. Cancellation gives no nutrition and releases owned seating.
- Maya's owner break bypasses café-open/staff checks, but still needs stock and seating.
  Pending meals expire after 90 minutes or at the 21:30 cutoff; unavailable food or
  staff cancels after 30 minutes. Completion delays another attempt by 30 minutes;
  cancellation delays it by 60 minutes.
- Pending meals and their timestamps persist; intention is not completed consumption.
- Social contact requires shared place and five minutes. Interrupted contact gives
  no benefit. Plans and role routines defer while meal/conversation state is busy.
- Sixty accumulated work/study minutes generate practice evidence. Evidence does not
  imply a validated measure of human skill or automatic executable policy change.
- External daily supply is explicit; finance and a full food production chain remain open.

## Side Effects
Updates needs and pending activity, schedules travel, reserves stock/tables, records
causal events, private meal memories, social interactions and practice evidence.

## Verification
`cd backend/agentd && cargo +1.96 test --locked sim::daily_life`.
Tests cover scarcity, exactly-once meal completion, restart state, timed coffee,
social interruption, night progression and deterministic supporting-resident replay.
