# Measure switching latency before changing architecture

## Current boundary — 2026-10-18

The measurement trials below are historical. The user now prohibits further manual round trips. Use existing logs and deterministic tests; the latest user instruction additionally authorizes one Mac restart to enable detailed command logging. Do not perform extra hardware writes or request manual switching for this investigation. Separate command generations from discovery revisions, and publish inventory before blocking write/readback. No server/client rewrite is justified by these findings.

## Original measurement scope

The user observed slow Windows-to-Mac automatic switching and approximately 1–3 seconds Mac-to-Windows. They asked us to investigate implementation mistakes rather than assume PowerShell or a missing server is responsible. Instrument the existing pipeline first; do not change scheduling, debounce, safety revalidation, backend, or two-host architecture in the measurement build.

## Measurement

An explicit local environment path enables a bounded timing log. Record monotonic process-relative timestamps and fixed stage/result labels, never display/USB identities, raw helper errors, input content or credentials. Default operation produces no timing file.

Measure scan start/end and purpose, ownership observation, dispatch, queue wait, pre-write inventory, retry waits, write start/end and completion. Windows helper stages distinguish initialization, USB enumeration, cosmetic/display discovery, physical-monitor opening, DDC write, post-write delay and readback. Preserve helper stdout protocol, failure handling and existing deadlines. Diagnostic errors must not alter switching decisions.

Helper stage records are collected after helper completion: their `t_us` is collection time, not stage onset; use `duration_us` for stage costs. Nested spans (native display enumeration inside open/discovery, and script total) overlap and must not be added together. A killed/timed-out helper currently supplies total failure duration only; its completed stderr stage markers are not retained. If such a failure occurs in the physical trial, collect additional bounded evidence before attributing it.

A private launcher starts a separate Windows measurement build with the existing saved configuration. The user quits the previous tray app, starts the launcher and performs a bounded foreground round trip. Read logs over the already authorized SSH connection; SSH itself does not perform monitor writes. Logs cannot establish exact physical button time or visible picture time; those require user observation and must remain separate from transport/readback measurements.

## Known source-level contributors, not measured attribution

- Poll interval starts after each inventory finishes.
- Policy debounce is 300ms arrival / 700ms established departure.
- Writes require another full inventory, including cosmetic Windows discovery.
- Discovery and writes share one serialized worker; publication waits for command completion.
- Windows waits 1.5 seconds after SetVCPFeature before readback.
- Failed writes can retry with additional delay and full revalidation.

Use the resulting measurements to choose a targeted implementation fix. Preserve exact enrolled identity checks, partial/unknown handling, cancellation, and startup-absence behavior. Verify any later policy or scheduler change with deterministic regressions without requesting another physical trial.

## Evidence-driven follow-up

After collecting desktop timing, remove redundant Service queries already supplied by Get-PnpDevice; preserve topology properties and validation. Next, wake the existing worker on native DBT_DEVNODES_CHANGED hints instead of waiting solely for its periodic timer. Keep fallback polling and all existing policy/revalidation. Microsoft documents this event as a device-list change with no identity payload: https://learn.microsoft.com/en-us/windows/win32/devio/dbt-devnodes-changed. It cannot establish USB ownership. Log receipt for the next comparison.

## Reproduce the offline audit

Run `python3 scripts/analyze-timing.py /absolute/path/to/local-timing.jsonl` and `python3 scripts/test-analyze-timing.py`. The standard-library-only analyzer groups sessions and helper invocations, reports disjoint phase accounting separately from nested spans, and associates command/notification chronology without inferring physical switch time. Raw logs and generated summaries remain outside Git.

## Automatic log rotation — 2026-10-18

Replace stop-at-cap with an active 1MiB JSONL file and five numbered archives (`.1` newest, `.5` oldest). Rotate before a whole record would exceed the limit, including startup with an already-full file. Hold a stable per-family `.lock` OS lock and close Windows log handles before rename. Preserve partial trailing records in an archive, then start a clean active file. Disk failures must not block hardware work; report dropped records when writing recovers. Retention deliberately bounds history, not recording duration. Logging remains explicitly enabled, identity-free and asynchronous.

The offline analyzer reads a captured stable file family oldest-first, associates sessions/helper invocations across boundaries, and reports partial tails, missing session starts and explicit loss markers. The launcher must identify a newly written session across the rotation family rather than use a stale byte offset or a size preflight. Verify startup-full, repeated rotations/retention, restart append, partial tails, write/rename failures and recovery with temporary files, plus native Windows tests. Do not request further manual switching or restart an active app during validation.

## Detailed Mac transport records — 2026-10-18

Log each enrolled set/get with an invocation number, requested/read input, target role, actual spawn and terminal process result. Retain the five-second deadline and existing validation. Exit zero means transport completion, never confirmed visible switching. Readback parse results remain separate from process outcomes. Per-record UTC supports cross-host alignment subject to clock-offset uncertainty; durations remain monotonic. Process duration starts in the runner; readback duration includes validation, so they overlap and must not be summed. The analyzer preserves these records and leaves absent historical UTC values unknown. No extra DDC reads are introduced.

## Latest approved event-driven deployment — 2026-10-18

1. Add Mac IOKit first-match/termination notifications as hints to the existing worker, using the main run loop and explicit cleanup. Keep periodic fallback and all fresh identity/lifecycle checks. Record subscription readiness/failure and notifications without device identifiers.
2. Use native Windows full display interface identity enumeration for prewrite/retry/probe validation, preserving cosmetic WMI names only in UI snapshots. Reduce established complete-departure settling to 300ms; partial/unknown still cancels.
3. Verify synthetic callback/resource cleanup and controller wake/cancellation tests, platform tests, native builds and exact identity parity where the interactive session permits. Deploy both automatically with graceful exit, saved config hashes and new log sessions.
4. Analyze ordinary-use traces; no repeated physical trials. Same-target multi-host requests are still possible, and display-active timestamps remain distinct from visible-picture evidence.
