"""CPU snapshots must not imply complete accounting for transient processes."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'release'))
from load_support.census import cpu_delta


def snapshot(at, rows, unavailable=()):
    return {'monotonic_seconds': at, 'processes': rows,
            'unavailable_pids': list(unavailable)}


def process(pid, category, cpu):
    return {'pid': pid, 'category': category, 'cpu_seconds': cpu}


class CpuScope(unittest.TestCase):
    def test_transient_processes_are_explicitly_unmeasured(self):
        before = snapshot(1, [process(1, 'owner', 2), process(2, 'guardian_helper', 1)])
        after = snapshot(3, [process(1, 'owner', 3), process(3, 'guardian_helper', 1)])
        result = cpu_delta(before, after)
        self.assertEqual(result['categories']['owner']['core_percent'], 50)
        self.assertEqual(result['unmatched_before_pids'], [2])
        self.assertEqual(result['unmatched_after_pids'], [3])
        self.assertFalse(result['complete_process_tree_accounting'])
        self.assertIsNone(result['categories']['guardian_helper']['cpu_seconds'])

    def test_degraded_samples_remain_visible_in_interval(self):
        row = process(1, 'owner', 2)
        result = cpu_delta(snapshot(1, [row], [2]), snapshot(3, [row], [3]))
        self.assertEqual(result['unavailable_pids'], [2, 3])
        self.assertFalse(result['complete_process_tree_accounting'])

    def test_even_matching_snapshots_do_not_measure_between_sample_processes(self):
        row = process(1, 'owner', 2)
        result = cpu_delta(snapshot(1, [row]), snapshot(3, [row]))
        self.assertFalse(result['complete_process_tree_accounting'])
        self.assertIn('between', result['measurement_scope'])


if __name__ == '__main__':
    unittest.main()
