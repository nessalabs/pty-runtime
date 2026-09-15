"""The default selection must be runnable, or its pass bit means nothing.

`resources-projected-500` cannot pass on any host at the shipped defaults:
`ProjectionLimits` reserves 8 MiB of native memory per projected session against
a 1 GiB `resident_bytes` quota, which admits exactly 128. Selecting it by
default made the documented `load.py --output ...` command record a failed trial
and exit non-zero everywhere, so a real regression and a structurally impossible
case looked identical from outside.
"""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    'release_matrix', Path(__file__).resolve().parents[1] / 'release/load_support/matrix.py')
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


class DefaultSelection(unittest.TestCase):
    def test_the_host_dependent_cases_are_not_selected_by_default(self):
        for smoke in (False, True):
            default = matrix.default_selection(smoke)
            for name in matrix.HOST_DEPENDENT:
                self.assertNotIn(name, default, f'{name} is host-dependent')

    def test_they_are_still_defined_so_they_can_be_run_by_name(self):
        cases = matrix.cases()
        for name in matrix.HOST_DEPENDENT:
            self.assertIn(name, cases)
        self.assertEqual(cases['resources-projected-500']['sessions'], 500)
        self.assertFalse(cases['resources-projected-500']['raw'])

    def test_the_default_selection_is_otherwise_the_whole_matrix(self):
        cases = matrix.cases()
        self.assertEqual(set(cases) - set(matrix.default_selection()),
                         set(matrix.HOST_DEPENDENT))
        self.assertEqual(len(matrix.default_selection()),
                         len(cases) - len(matrix.HOST_DEPENDENT))

    def test_the_bounded_matrix_stops_at_the_documented_128_sessions(self):
        """LOAD.md calls 500-session qualification host-dependent and outside it."""
        for name in matrix.default_selection():
            self.assertLessEqual(matrix.cases()[name]['sessions'], 128, name)


if __name__ == '__main__':
    unittest.main()
