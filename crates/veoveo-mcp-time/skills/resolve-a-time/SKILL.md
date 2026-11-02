---
name: resolve-a-time
description: Turn a time a person or system gives, civil, military DTG, GNSS, Unix, TAI, Julian or mission-relative, into a TimeInstant bound to its time-zone and leap-second releases, convert it to other scales and zones, and pass it on to other servers. Use whenever a task names a time.
---

# Resolve a time

Time is the one temporal authority. Never pass a plain time string to another server
or compute offsets yourself: resolve it here, then pass the result.

## Resolve

1. Read `time://authorities/current` for the active TZDB and leap-second releases, and
   `time://clock/quality` when the time is "now" or derives from this host's clock.
2. Call `time__resolve_time` with the expression, tagged by its `format`: RFC 3339 or
   RFC 9557, a civil time (`localDatetime`, an IANA `zoneId` and the `tzdbReleaseId` of
   the active time-zone release), a military date time group, Unix or TAI seconds, GPS
   week and seconds of week, a Julian TAI day, or a time relative to a mission epoch.
   Add `additionalUncertainty` when the source of the time is less exact than its digits.
3. A civil time that falls in a gap or an overlap of its zone is refused unless its
   `disambiguation` is `earlier` or `later`; ask the person which they mean rather than
   guessing.
4. The result's `instant` is a `TimeInstant`: integral TAI seconds since 1970 and a
   nanosecond, its `uncertainty`, and the `authority` binding, the releases that
   interpreted it.

## Convert

Call `time__convert_time` with the `instant` exactly as `time__resolve_time` returned
it, to read it as UTC, in chosen IANA zones, or as TAI, TT, TDB, GPS time or Galileo
time. Conversions are deterministic for the bound releases.

## Pass it on

Pass the `TimeInstant` with its uncertainty and authority binding unchanged to any server
that takes a time, such as a map or an optimizer; never only its UTC text. Report a time
to a person with its zone and, when it is not exact, its uncertainty.

## Windows and clocks

- `time__evaluate_windows` takes the union, intersection or difference of half-open
  windows `[start, end)`.
- `time__assess_clock` checks this host's clock against a policy you pass or the tenant's,
  before you trust "now" for anything that must be exact.
- `time__create_event` schedules an event at a resolved instant; subscribe to its
  `time://events/{event_id}` to be told when it is due, and cancel it with
  `time__cancel_event` and the revision you last read.
