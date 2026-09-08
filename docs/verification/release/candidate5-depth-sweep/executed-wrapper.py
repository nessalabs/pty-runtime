#!/usr/bin/env python3
"""Run full five-trial depth sweep using the frozen release driver."""
import sys
from pathlib import Path
root = Path.cwd()
sys.path.insert(0, str(root / 'scripts/release'))
import load
from load_support import matrix
original = matrix.cases
def cases(smoke=False):
    result = original(smoke)
    base = result['capacity-projected']
    for depth in (32, 64, 128):
        result[f'capacity-depth-{depth}'] = {**base, 'staging-slots': depth}
    return result
matrix.cases = cases
if __name__ == '__main__':
    load.main()
