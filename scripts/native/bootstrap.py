#!/usr/bin/env python3
"""Prepare the pinned native cache using the same verified experiment build path."""
from pathlib import Path
import argparse
import sys
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'experiments'))
import run as experiments


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=ROOT / 'work/experiment-cache')
    parser.add_argument('--jobs', type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1 or args.jobs > 64:
        parser.error('--jobs must be between 1 and 64')
    experiments.build(args.cache.resolve(), 'native', args.jobs)


if __name__ == '__main__':
    main()
