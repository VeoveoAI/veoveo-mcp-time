---
name: validate-a-timeline
description: Check that named time points of a plan or mission keep their order and their minimum and maximum separations. Use when a task gives a sequence of events with constraints between them.
---

# Validate a timeline

1. Name each point and give its time as an expression `time__resolve_time` accepts (skill
   `resolve-a-time`), in `points` as `{name, at}`. A time relative to a mission epoch
   names the epoch's version, which `time://epochs/{epoch_id}/versions/{version}` shows.
2. State each constraint in `constraints` as `{predecessor, successor}` by point name,
   with a `minimumSeparation` (zero when omitted) and an optional `maximumSeparation`.
3. Call `time__validate_timeline`. It resolves every point against the active releases and
   runs as an MCP Task: wait for it to finish.
4. Report every violation it names, with its constraint and message. A constraint that
   holds only within the points' uncertainty is not a clean pass: say so.

Publish a mission epoch the points refer to with `time__create_epoch`. The version is its
own idempotency: publishing the same version again with the same content answers the
stored version.
