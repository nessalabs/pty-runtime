"""CPU snapshots must not imply complete accounting for transient processes."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'release'))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'guardian'))
from load_support.census import cpu_delta
from resources import incarnation_of


def snapshot(at, rows, unavailable=()):
    return {'monotonic_seconds': at, 'processes': rows,
            'unavailable_pids': list(unavailable)}


def process(pid, category, cpu, ticks=None):
    return {'pid': pid, 'category': category, 'cpu_seconds': cpu, 'start_ticks': ticks}


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


class ProcessIdentity(unittest.TestCase):
    """A pid is a number the kernel reuses; a process is a number plus a birth."""

    def test_the_stat_offsets_name_the_parent_and_the_start_time(self):
        # A command containing spaces and a ')' is exactly what the split has to
        # survive: fields 1 and 2 are dropped, so index 1 is ppid (field 4) and
        # index 19 is starttime (field 22).
        fields = ['7', '(od d) ha)', 'S', '4242'] + [str(n) for n in range(5, 23)]
        line = ' '.join(fields)
        ppid, started = incarnation_of(line.rsplit(')', 1)[1].split())
        self.assertEqual(ppid, '4242', 'field 4 is the parent')
        self.assertEqual(started, '22', 'field 22 is the start time')

    @unittest.skipUnless(sys.platform.startswith('linux'), 'needs /proc')
    def test_the_offsets_agree_with_this_very_process(self):
        import os
        from resources import incarnation
        ppid, started = incarnation(Path('/proc') / str(os.getpid()))
        self.assertEqual(int(ppid), os.getppid())
        self.assertGreater(int(started), 0)

    def test_a_recycled_pid_is_not_credited_with_its_predecessors_cpu(self):
        """Subtracting by pid alone produced a delta between two processes."""
        before = snapshot(1, [process(1, 'owner', 2, ticks=100),
                              process(9, 'guardian_helper', 50, ticks=100)])
        after = snapshot(3, [process(1, 'owner', 3, ticks=100),
                             process(9, 'guardian_helper', 1, ticks=777)])
        result = cpu_delta(before, after)
        self.assertEqual(result['categories']['owner']['core_percent'], 50)
        self.assertIsNone(result['categories']['guardian_helper']['cpu_seconds'],
                          'pid 9 is two different processes, so it has no delta')
        self.assertEqual(result['unmatched_before_pids'], [9])
        self.assertEqual(result['unmatched_after_pids'], [9])


if __name__ == '__main__':
    unittest.main()
