#!/usr/bin/env python3
"""
Master Test Execution Coordinator.
Runs specified test tiers with unified continuous TAP version 13 output.
"""

import sys
import os
import time

sys.path.insert(0, os.path.abspath(os.path.dirname(__file__)))
from harness.client import TapReporter
from tier1_feature_coverage.test_tier1_features import run_tier1_tests
from tier2_boundary_corner.test_tier2_boundaries import run_tier2_tests
from tier3_cross_feature.test_tier3_pairwise import run_tier3_tests
from tier4_real_world.test_tier4_scenarios import run_tier4_scenarios

TIER_COUNTS = {
    "tier1": 155,
    "tier2": 145,
    "tier3": 29,
    "tier4": 15,
    "all": 344
}

def main():
    target_tier = "all"
    base_url = os.environ.get("BASE_URL", "http://127.0.0.1:8089")

    args = sys.argv[1:]
    for arg in args:
        if arg.startswith("http://") or arg.startswith("https://"):
            base_url = arg
        elif arg.lower() in TIER_COUNTS:
            target_tier = arg.lower()
        elif arg in ["-h", "--help"]:
            print("Usage: python3 run_all.py [tier1|tier2|tier3|tier4|all] [BASE_URL]")
            sys.exit(0)

    total_expected = TIER_COUNTS.get(target_tier, 344)
    reporter = TapReporter(total_expected=total_expected)
    reporter.print_header()

    print(f"# Personal Finance PWA E2E Test Suite - Target Tier: {target_tier.upper()}", flush=True)
    print(f"# Target Server: {base_url}", flush=True)

    if target_tier in ["tier1", "all"]:
        print("# >>> Executing Tier 1: Feature Coverage (155 tests) <<<", flush=True)
        run_tier1_tests(base_url, reporter=reporter)

    if target_tier in ["tier2", "all"]:
        print("# >>> Executing Tier 2: Boundary & Corner Cases (145 tests) <<<", flush=True)
        run_tier2_boundaries_clean(base_url, reporter)

    if target_tier in ["tier3", "all"]:
        print("# >>> Executing Tier 3: Pairwise Cross-Feature (29 tests) <<<", flush=True)
        run_tier3_tests(base_url, reporter=reporter)

    if target_tier in ["tier4", "all"]:
        print("# >>> Executing Tier 4: Real-World Scenarios (15 scenarios) <<<", flush=True)
        run_tier4_scenarios(base_url, reporter=reporter)

    all_passed = reporter.print_summary()
    sys.exit(0 if all_passed else 1)

def run_tier2_boundaries_clean(base_url, reporter):
    run_tier2_tests(base_url, reporter=reporter)

if __name__ == "__main__":
    main()
