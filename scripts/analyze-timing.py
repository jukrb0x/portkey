#!/usr/bin/env python3
"""Offline PortKey timing summary. Usage: python3 scripts/analyze-timing.py LOG.jsonl.

All durations are microseconds. Child timestamps are ingestion times, not phase
onsets. Exclusive helper partitions exclude nested native_display_enumerate.
Device-event associations describe chronology, never physical switch latency.
"""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import statistics

PHASES = {
    'inventory': ('add_type', 'usb_enumerate', 'names_display_enumerate'),
    'displays': ('add_type', 'names_display_enumerate'),
    'write': ('add_type', 'exact_display_open', 'set_vcp', 'postwrite_wait', 'postwrite_readback'),
    'read': ('add_type', 'exact_display_open', 'read_input'),
}
STAGES = set().union(*PHASES.values(), ('native_display_enumerate', 'script_total', 'native_helper_total'))


def read_records(stream):
    lines = list(stream)
    records = []
    partial_tail = False
    for index, line in enumerate(lines):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            if index == len(lines) - 1 and not line.endswith('\n'):
                partial_tail = True
                break
            raise ValueError(f'Invalid JSON at line {index + 1}') from error
        if not isinstance(record, dict):
            raise ValueError(f'Expected record at line {index + 1}')
        for name in ('session', 'pid', 't_us', 'duration_us', 'value', 'utc_us'):
            value = record.get(name)
            if value is None and name in ('duration_us', 'value', 'utc_us'):
                continue
            if type(value) is not int or value < 0:
                raise ValueError(f'Invalid {name} at line {index + 1}')
        if not all(isinstance(record.get(name), str) for name in ('event', 'role')):
            raise ValueError(f'Invalid event/role at line {index + 1}')
        if record['event'] == 'ddc_command':
            if record.get('value') is None or not all(isinstance(record.get(name), str) for name in ('operation', 'target', 'outcome')):
                raise ValueError(f'Invalid DDC invocation/metadata at line {index + 1}')
            input_code, exit_code = record.get('input'), record.get('exit_code')
            if input_code is not None and (type(input_code) is not int or not 0 <= input_code <= 255):
                raise ValueError(f'Invalid DDC input at line {index + 1}')
            if exit_code is not None and (type(exit_code) is not int or not -(2**31) <= exit_code < 2**31):
                raise ValueError(f'Invalid DDC exit code at line {index + 1}')
        record.setdefault('duration_us', None)
        record.setdefault('value', None)
        records.append(record)
    return records, partial_tail


def read_family(path):
    path = Path(path)
    # A captured family must be stable while read; copy it locally before analysis.
    archives = []
    for candidate in path.parent.iterdir():
        prefix = path.name + '.'
        if candidate.name.startswith(prefix):
            suffix = candidate.name[len(prefix):]
            if suffix.isascii() and suffix.isdigit() and str(int(suffix)) == suffix and int(suffix) > 0:
                archives.append((int(suffix), candidate))
    files = [p for _, p in sorted(archives, reverse=True)] + [path]
    records, partial = [], []
    for source in files:
        with source.open(encoding='utf-8-sig') as stream:
            rows, tail = read_records(stream)
        records.extend(rows)
        if tail:
            partial.append(str(source))
    return records, [str(p) for p in files], partial


def stats(values):
    return {'count': len(values), 'median': statistics.median(values)}


def helpers_for(records):
    groups = defaultdict(list)
    for record in records:
        if record['event'] in STAGES | {'helper_start', 'helper_end', 'helper_failed'}:
            groups[record['role'], record['value']].append(record)
    helpers = []
    incomplete = 0
    for (mode, invocation), events in groups.items():
        starts = [e for e in events if e['event'] == 'helper_start']
        ends = [e for e in events if e['event'] in ('helper_end', 'helper_failed')]
        counts = defaultdict(int)
        for event in events:
            counts[event['event']] += 1
        if len(starts) != 1 or len(ends) != 1 or any(n > 1 for n in counts.values()):
            incomplete += 1
            continue
        start, end = starts[0], ends[0]
        if start['t_us'] > end['t_us'] or end['duration_us'] is None:
            incomplete += 1
            continue
        durations = {e['event']: e['duration_us'] for e in events if e['event'] in STAGES and e['duration_us'] is not None}
        phases = PHASES.get(mode, ())
        totals = [name for name in ('script_total', 'native_helper_total') if name in durations]
        native = totals == ['native_helper_total']
        if native:
            phases = tuple(p for p in phases if p != 'add_type') if mode in ('read', 'write') else ()
        exclusive = None
        if len(totals) == 1 and phases and all(phase in durations for phase in phases):
            child_total = durations[totals[0]]
            other = child_total - sum(durations[p] for p in phases)
            overhead = end['duration_us'] - child_total
            if other >= 0 and overhead >= 0:
                exclusive = {p: durations[p] for p in phases}
                exclusive['native_other' if native else 'script_other'] = other
                exclusive['process_overhead'] = overhead
        helpers.append(dict(invocation=invocation, mode=mode, start_us=start['t_us'], end_us=end['t_us'], total_us=end['duration_us'], outcome=end['event'], durations_us=durations, exclusive_phases_us=exclusive))
    return helpers, incomplete


def summarize_session(records):
    records = sorted(records, key=lambda r: r['t_us'])
    helpers, incomplete = helpers_for(records)
    helper_summary = {}
    for mode in sorted({h['mode'] for h in helpers}):
        selected = [h for h in helpers if h['mode'] == mode]
        phases = defaultdict(list)
        for helper in selected:
            for name, duration in (helper['exclusive_phases_us'] or {}).items():
                phases[name].append(duration)
        helper_summary[mode] = {
            'total_us': stats([h['total_us'] for h in selected]),
            'failures': sum(h['outcome'] == 'helper_failed' for h in selected),
            'exclusive_partition_count': sum(h['exclusive_phases_us'] is not None for h in selected),
            'exclusive_phases_us': {p: stats(values) for p, values in phases.items()},
        }

    scans = defaultdict(list)
    pending_scan = None
    idle_count = 0
    for record in records:
        if record['event'] in ('inventory_end_ok', 'inventory_end_error'):
            pending_scan = record if record['event'] == 'inventory_end_ok' else None
            if record['event'] == 'inventory_end_ok' and record['role'] == 'idle':
                idle_count += 1
        elif record['event'] == 'ownership_observed':
            if pending_scan and pending_scan['role'] == 'idle' and pending_scan['duration_us'] is not None:
                scans[record['role']].append(pending_scan['duration_us'])
            pending_scan = None

    commands = []
    completions = [r for r in records if r['event'] in ('completion_ok', 'completion_error', 'completion_stale')]
    observations = [r for r in records if r['event'] == 'ownership_observed']
    for completion in completions:
        generation = completion['value']
        queued = [r for r in records if r['event'] == 'dispatch_queued' and r['value'] == generation and r['t_us'] <= completion['t_us']]
        if len(queued) != 1:
            continue
        queued = queued[0]
        interval = [r for r in records if queued['t_us'] <= r['t_us'] <= completion['t_us']]
        dequeued = [r for r in interval if r['event'] == 'dispatch_dequeued' and r['value'] == generation]
        work_start = dequeued[0]['t_us'] if len(dequeued) == 1 else None
        # Inventory revisions and job generations are independent counters.
        prewrite = [r for r in interval if work_start is not None and r['t_us'] >= work_start and r['event'] == 'inventory_end_ok' and r['role'] == 'prewrite']
        previous = [r for r in observations if r['t_us'] <= queued['t_us']]
        prior = previous[-1] if previous else None
        # The queue's worker duration includes prewrite and helper time; do not add it to them.
        commands.append({
            'generation': generation, 'queued_us': queued['t_us'], 'completion_us': completion['t_us'],
            'outcome': completion['event'], 'completion_role': completion['role'],
            'queue_us': dequeued[0]['duration_us'] if len(dequeued) == 1 else None,
            'prewrite_us': prewrite[0]['duration_us'] if len(prewrite) == 1 else None,
            'queued_to_completion_us': completion['t_us'] - queued['t_us'],
            'preceding_observation': prior['role'] if prior else None,
            'observed_to_queued_us': queued['t_us'] - prior['t_us'] if prior else None,
            'write_helpers': [h for h in helpers if work_start is not None and h['mode'] == 'write' and work_start <= h['start_us'] and h['end_us'] <= completion['t_us']],
        })

    device_events = []
    for event in (r for r in records if r['event'] == 'device_change'):
        before = [r for r in observations if r['t_us'] <= event['t_us']]
        state = before[-1]['role'] if before else None
        after = [r for r in observations if r['t_us'] >= event['t_us']]
        changed = [r for r in after if state is not None and r['role'] != state]
        stable = [r for r in changed if r['role'] in ('owned', 'absent')]
        done = [r for r in completions if r['t_us'] >= event['t_us']]
        def associated(candidates):
            if not candidates:
                return None
            candidate = candidates[0]
            return {'at_us': candidate['t_us'], 'role': candidate['role'], 'elapsed_us': candidate['t_us'] - event['t_us']}
        device_events.append({
            'at_us': event['t_us'], 'preceding_state': state,
            'first_observation': associated(after),
            'first_changed_observation': associated(changed),
            'first_stable_change': associated(stable),
            'next_completion': associated(done),
        })
    # An invocation may report both subprocess transport and a later readback.
    # Their durations overlap; retain every record without summing or inferring
    # visible switching. UTC values are explicit wall-clock samples, not derived.
    ddc = defaultdict(list)
    for record in records:
        if record['event'] == 'ddc_command':
            ddc[record['role'], record['value']].append({
                name: record.get(name) for name in
                ('t_us', 'utc_us', 'operation', 'input', 'target', 'outcome', 'exit_code', 'duration_us')
            })
    native_usb = {}
    for outcome, event in [('successful', 'usb_inventory_end_ok'), ('failed', 'usb_inventory_end_error')]:
        durations = [r['duration_us'] for r in records if r['event'] == event and r['duration_us'] is not None]
        if durations:
            native_usb[outcome] = stats(durations)
    return {
        'ddc_commands': [dict(role=role, invocation=invocation, records=events) for (role, invocation), events in ddc.items()],
        'native_usb_us': native_usb,
        'dropped_records': sum(r['value'] or 0 for r in records if r['event'] == 'trace_dropped'),
        'has_session_start': any(r['event'] == 'session_start' for r in records),
        'session': records[0]['session'], 'pid': records[0]['pid'],
        'record_count': len(records), 'incomplete_helpers': incomplete,
        'idle_scans_by_observed_state_us': {state: stats(values) for state, values in scans.items()},
        'idle_scans_without_observation': idle_count - sum(map(len, scans.values())),
        'helpers': helper_summary, 'commands': commands,
        'unpaired_completions': len(completions) - len(commands), 'device_events': device_events,
    }


def analyze(records):
    groups = defaultdict(list)
    for record in records:
        groups[record['session'], record['pid']].append(record)
    return [summarize_session(rows) for _, rows in sorted(groups.items())]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('log', help='Local timing JSONL file; numbered rotation archives are included automatically')
    args = parser.parse_args()
    try:
        records, sources, partial_files = read_family(args.log)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(json.dumps({
        'units': 'microseconds', 'ignored_partial_tail': bool(partial_files),
        'sources': sources, 'partial_tail_files': partial_files,
        'notes': [
            'Rotation archives are read oldest first. Retention can remove earlier records; incomplete helpers are reported, not reconstructed.',
            'Child stage timestamps are ingestion times; only duration_us measures each stage.',
            'Exclusive phases exclude nested native display enumeration. Phase medians need not sum to the median total.',
            'process_overhead is parent helper duration minus script_total; it includes startup and teardown, not pure startup.',
            'Idle scans are labelled only by their subsequent observation, with no intervening completed scan.',
            'Device-event associations are chronological, not causal or physical switch latency. No visual switch timing is available.',
            'DDC invocation records retain exact transport/readback outcomes and explicit UTC timestamps; overlapping durations are not summed and transport success does not prove a visible switch.',
            'Queued-to-completion includes queue, prewrite and write helpers; do not sum those overlapping totals.',
        ],
        'sessions': analyze(records),
    }, indent=2))


if __name__ == '__main__':
    main()
