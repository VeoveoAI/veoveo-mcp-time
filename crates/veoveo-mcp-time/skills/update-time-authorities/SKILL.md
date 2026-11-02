---
name: update-time-authorities
description: Acquire a new IANA time-zone or leap-second release from a registered source, check it, and make it the tenant's active authority. Use when an administrator asks to update the time zones or leap seconds Time resolves against.
---

# Update the time authorities

Every resolved instant names the releases that interpreted it, so a release becomes
active only by an explicit activation, and a retired release stays for the instants that
name it.

1. `time__list_sources` lists the tenant's registered HTTPS sources of IANA time-zone and
   leap-second data; register one with `time__create_source` and an idempotency key.
2. `time__start_acquisition` downloads, verifies and compiles a release from a source and
   stages it. It continues after the call returns: follow it with
   `time__get_acquisition` until it is `complete` or `failed`. `time__cancel_acquisition`
   stops a running one and stages nothing.
3. `time__list_active_authorities` shows the active release of each dataset and its
   pointer version.
4. `time__activate_release` makes the staged release active. Pass the release's record
   version and the active pointer version you last read; it fails if either changed, so
   read them again and retry. The previous release is retired and kept.
5. Read `time://authorities/current` to confirm the active releases.

`time__replace_clock_policy` sets the clock quality policy `time__assess_clock` applies
when a call passes none; pass the record version you last read, or none when no policy
exists yet.
