---
name: expand-a-calendar
description: Expand a versioned operational calendar into its half-open time windows over a horizon, or publish a new calendar version. Use when a task asks when operations, shifts or windows happen.
---

# Expand an operational calendar

1. Find the calendar: `time://calendars` lists the tenant's calendar versions; read the
   one you need at `time://calendars/{calendar_id}/versions/{version}`. Use the version
   the task names; a published version never changes.
2. Resolve the horizon's start and end with `time__resolve_time` (skill
   `resolve-a-time`); the horizon is `{start, end}`, two `TimeInstant`s.
3. Call `time__expand_schedule` with that calendar document as `calendar` and the
   `horizon`, and `maximumOccurrences` when 10000 is not the bound you want. It runs as
   an MCP Task: wait for the Task to finish and read its result.
4. Every window is half-open, `[start, end)`: a window that ends when the next starts
   does not overlap it. Keep each window's instants with their authority binding when you
   pass them on.

To change a calendar, publish a new version with `time__create_calendar`: its
`calendarId`, a new `version`, a `name`, an IANA `zoneId`, its `windows` and any
`excludedDates`. The version is its own idempotency: publishing the same version again
with the same content answers the stored version, and different content is refused.
