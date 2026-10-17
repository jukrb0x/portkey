import importlib.util
import io
import pathlib
import unittest
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('timing', pathlib.Path(__file__).with_name('analyze-timing.py'))
timing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(timing)


def row(event, t=0, role='inventory', duration=None, value=None, session=1):
    return dict(event=event, t_us=t, role=role, duration_us=duration, value=value, session=session, pid=7)


class TimingTests(unittest.TestCase):
    def test_native_helper_partition_has_no_invented_compilation_time(self):
        records = [row('helper_start', 0, 'write', value=3)]
        for stage, duration in [('exact_display_open', 20), ('set_vcp', 60), ('postwrite_wait', 1500), ('postwrite_readback', 60), ('native_helper_total', 1650)]:
            records.append(row(stage, 1700, 'write', duration, 3))
        records.append(row('helper_end', 1700, 'write', 1700, 3))
        parts = timing.analyze(records)[0]['helpers']['write']['exclusive_phases_us']
        self.assertEqual(parts['native_other']['median'], 10)
        self.assertEqual(parts['process_overhead']['median'], 50)
        self.assertNotIn('add_type', parts)
        self.assertEqual(sum(v['median'] for v in parts.values()), 1700)
        records.insert(-1, row('script_total', 1700, 'write', 1650, 3))
        self.assertEqual(timing.analyze(records)[0]['helpers']['write']['exclusive_partition_count'], 0)

    def test_sessions_invocations_and_nested_phases(self):
        records = []
        for session, total in [(1, 1000), (2, 2000)]:
            records += [row('helper_start', value=1, session=session)]
            for stage, duration in [('add_type', 100), ('usb_enumerate', 400), ('names_display_enumerate', 200), ('native_display_enumerate', 50), ('script_total', 800)]:
                records.append(row(stage, 900, duration=duration, value=1, session=session))
            records.append(row('helper_end', total, duration=total, value=1, session=session))
        report = timing.analyze(records)
        self.assertEqual(len(report), 2)
        parts = report[0]['helpers']['inventory']['exclusive_phases_us']
        self.assertNotIn('native_display_enumerate', parts)
        self.assertEqual(parts['script_other']['median'], 100)
        self.assertEqual(parts['process_overhead']['median'], 200)
        self.assertEqual(sum(p['median'] for p in parts.values()), 1000)
        self.assertEqual(report[1]['helpers']['inventory']['total_us']['median'], 2000)

    def test_unapplied_scan_is_not_labelled_with_later_ownership(self):
        records = [row('inventory_end_ok', 100, 'idle', 100), row('inventory_end_ok', 300, 'idle', 200), row('ownership_observed', 310, 'absent'), row('ownership_observed', 320, 'owned')]
        report = timing.analyze(records)[0]
        self.assertEqual(report['idle_scans_by_observed_state_us']['absent'], {'count': 1, 'median': 200})
        self.assertNotIn('owned', report['idle_scans_by_observed_state_us'])
        self.assertEqual(report['idle_scans_without_observation'], 1)

    def test_command_and_event_intervals_use_parent_timestamps(self):
        records = [row('ownership_observed', 0, 'owned'), row('device_change', 10, 'native'), row('ownership_observed', 20, 'owned'), row('ownership_observed', 40, 'partial'), row('ownership_observed', 80, 'absent'), row('dispatch_queued', 100, 'automatic', value=3), row('dispatch_dequeued', 110, 'automatic', 10, 3), row('inventory_end_ok', 200, 'prewrite', 90, 41), row('helper_start', 210, 'write', value=9), row('set_vcp', 999, 'write', 30, 9), row('helper_end', 1000, 'write', 790, 9), row('completion_ok', 1010, 'current', 910, 3)]
        report = timing.analyze(records)[0]
        command = report['commands'][0]
        self.assertEqual(command['queue_us'], 10)
        self.assertEqual(command['prewrite_us'], 90)
        self.assertEqual(command['observed_to_queued_us'], 20)
        self.assertEqual(command['write_helpers'][0]['durations_us']['set_vcp'], 30)
        self.assertEqual(command['write_helpers'][0]['start_us'], 210)
        self.assertEqual(report['device_events'][0]['first_changed_observation']['elapsed_us'], 30)
        self.assertEqual(report['device_events'][0]['first_stable_change']['elapsed_us'], 70)
        self.assertEqual(report['device_events'][0]['next_completion']['elapsed_us'], 1000)

    def test_native_usb_scan_durations_are_separate_from_helper_times(self):
        records = [row('usb_inventory_end_ok', 100, 'native', 10), row('usb_inventory_end_ok', 200, 'native', 20), row('usb_inventory_end_error', 300, 'native', 30)]
        report = timing.analyze(records)[0]
        self.assertEqual(report['native_usb_us'], {'successful': {'count': 2, 'median': 15}, 'failed': {'count': 1, 'median': 30}})
        self.assertEqual(report['helpers'], {})

    def test_incomplete_helpers_not_summarized(self):
        report = timing.analyze([row('helper_start', value=1), row('set_vcp', 4, duration=5, value=1)])[0]
        self.assertEqual(report['incomplete_helpers'], 1)
        self.assertEqual(report['helpers'], {})

    def test_impossible_phase_partition_is_not_reported_as_exclusive(self):
        records = [row('helper_start', value=1)]
        for stage in ('add_type', 'usb_enumerate', 'names_display_enumerate'):
            records.append(row(stage, 90, duration=40, value=1))
        records += [row('script_total', 90, duration=100, value=1), row('helper_end', 100, duration=100, value=1)]
        summary = timing.analyze(records)[0]['helpers']['inventory']
        self.assertEqual(summary['exclusive_partition_count'], 0)
        self.assertEqual(summary['exclusive_phases_us'], {})

    def test_trace_drop_markers_report_evidence_gaps(self):
        records = [row('trace_dropped', 1, 'diagnostics', value=3), row('trace_dropped', 2, 'diagnostics', value=2)]
        report = timing.analyze(records)[0]
        self.assertEqual(report['dropped_records'], 5)
        self.assertFalse(report['has_session_start'])

    def test_rotated_family_reassembles_helper_across_files(self):
        import json
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory) / 'trace.jsonl'
            older = base.with_name(base.name + '.2')
            recent = base.with_name(base.name + '.1')
            older.write_text(json.dumps(row('helper_start', 10, value=1)) + '\n')
            recent.write_text(json.dumps(row('usb_enumerate', 100, duration=50, value=1)) + '\n')
            base.write_text(json.dumps(row('helper_end', 110, duration=100, value=1)) + '\n')
            base.with_name(base.name + '.tmp').write_text('not a trace')
            records, sources, partial = timing.read_family(base)
            self.assertEqual(sources, [str(older), str(recent), str(base)])
            self.assertEqual(partial, [])
            self.assertEqual(timing.analyze(records)[0]['helpers']['inventory']['total_us']['median'], 100)

    def test_archived_partial_tail_does_not_corrupt_new_session(self):
        import json
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory) / 'trace.jsonl'
            archive = base.with_name(base.name + '.1')
            archive.write_text(json.dumps(row('session_start')) + '\n{unfinished')
            base.write_text(json.dumps(row('session_start', session=2)) + '\n')
            records, _, partial = timing.read_family(base)
            self.assertEqual(partial, [str(archive)])
            self.assertEqual(len(timing.analyze(records)), 2)

    def test_ddc_invocations_preserve_transport_and_readback_without_summing(self):
        import json
        records = []
        for session in (1, 2):
            for t, outcome, code, duration, value in [(40, 'transport_ok', 0, 30, None), (55, 'readback_ok', None, 55, 5)]:
                record = row('ddc_command', t, 'macos', duration, 0, session)
                record.update(operation='get_input', input=value, target='current', outcome=outcome, exit_code=code, utc_us=1792255680350041 + t)
                records.append(record)
        parsed, _ = timing.read_records(io.StringIO(''.join(json.dumps(r) + '\n' for r in records)))
        reports = timing.analyze(parsed)
        self.assertEqual(len(reports), 2)
        invocation = reports[0]['ddc_commands'][0]
        self.assertEqual(invocation['invocation'], 0)
        self.assertEqual(invocation['role'], 'macos')
        self.assertEqual(invocation['records'], [
            dict(t_us=40, utc_us=1792255680350081, operation='get_input', input=None, target='current', outcome='transport_ok', exit_code=0, duration_us=30),
            dict(t_us=55, utc_us=1792255680350096, operation='get_input', input=5, target='current', outcome='readback_ok', exit_code=None, duration_us=55),
        ])
        self.assertEqual(reports[0]['commands'], [])
        self.assertEqual(reports[0]['helpers'], {})

    def test_ddc_failure_and_old_logs_preserve_unknown_utc(self):
        import json
        record = row('ddc_command', 9, 'macos', 9, 2)
        record.update(operation='set_input', input=7, target='remote', outcome='exit_error', exit_code=-1)
        records, _ = timing.read_records(io.StringIO(json.dumps(record) + '\n'))
        detail = timing.analyze(records)[0]['ddc_commands'][0]['records'][0]
        self.assertIsNone(detail['utc_us'])
        self.assertEqual(detail['exit_code'], -1)
        self.assertEqual(detail['outcome'], 'exit_error')
        self.assertEqual(timing.analyze([row('session_start')])[0]['ddc_commands'], [])
        for field, value in [('utc_us', 1.5), ('exit_code', True), ('input', 256), ('operation', None), ('value', None)]:
            with self.subTest(field=field):
                invalid = dict(record, **{field: value})
                with self.assertRaises(ValueError):
                    timing.read_records(io.StringIO(json.dumps(invalid) + '\n'))

    def test_reader_rejects_corrupt_records_but_reports_partial_tail(self):
        import json
        valid = json.dumps(row('helper_start', value=1)) + '\n'
        records, partial = timing.read_records(io.StringIO(valid + '{partial'))
        self.assertEqual(len(records), 1)
        self.assertTrue(partial)
        with self.assertRaises(ValueError):
            timing.read_records(io.StringIO('{invalid}\n' + valid))
        with self.assertRaises(ValueError):
            timing.read_records(io.StringIO(json.dumps(row('helper_start', duration=-1)) + '\n'))


if __name__ == '__main__':
    unittest.main()
