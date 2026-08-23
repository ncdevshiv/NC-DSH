#!/usr/bin/env python3
"""
OpenMem Real Test Runner.

Runs all real tests (no mocks/stubs/placeholders) and reports results.

Usage:
    python tests/real/test_runner.py
    python tests/real/test_runner.py --verbose
    python tests/real/test_runner.py --class TestVectorDBReal
"""

import os
import sys
import unittest
from pathlib import Path

# Ensure OpenMem root is on path
OPENMEM_ROOT = Path(__file__).parent.parent
if str(OPENMEM_ROOT) not in sys.path:
    sys.path.insert(0, str(OPENMEM_ROOT))

# Configure logging for tests
import logging
logging.basicConfig(level=logging.WARNING, format="%(name)s %(levelname)s: %(message)s")

# Configure UTF-8 output on Windows
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except AttributeError:
        pass


def run_tests(verbose=False, test_class=None):
    """Run all real tests and report results."""
    # Import test module directly
    test_module_path = Path(__file__).parent / "test_full_suite.py"
    import importlib.util
    spec = importlib.util.spec_from_file_location("test_full_suite", test_module_path)
    test_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(test_module)

    # Get test classes
    all_classes = [
        test_module.TestVectorDBReal,
        test_module.TestMemoryManagerReal,
        test_module.TestUserModelReal,
        test_module.TestPatternRecognizerReal,
        test_module.TestReflectionEngineReal,
        test_module.TestSchedulerReal,
        test_module.TestAgentAdaptersReal,
        test_module.TestLLMModuleReal,
        test_module.TestConfigReal,
        test_module.TestEndToEndReal,
    ]

    if test_class:
        for cls in all_classes:
            if cls.__name__ == test_class:
                all_classes = [cls]
                break
        else:
            print(f"Test class not found: {test_class}")
            return False

    # Create test suite
    loader = unittest.TestLoader()
    suite = unittest.TestSuite()

    for cls in all_classes:
        tests = loader.loadTestsFromTestCase(cls)
        suite.addTests(tests)

    # Run
    print(f"\n{'='*60}")
    print(f"  OpenMem Real Test Suite")
    print(f"  {len(all_classes)} test classes, {suite.countTestCases()} tests")
    print(f"{'='*60}\n")

    runner = unittest.TextTestRunner(
        verbosity=2 if verbose else 1,
        stream=sys.stdout,
    )
    result = runner.run(suite)

    # Summary
    print(f"\n{'='*60}")
    print(f"  Results Summary")
    print(f"{'='*60}")
    print(f"  Tests run:  {result.testsRun}")
    print(f"  Passed:     {result.testsRun - len(result.failures) - len(result.errors)}")
    print(f"  Failed:     {len(result.failures)}")
    print(f"  Errors:     {len(result.errors)}")

    if result.failures:
        print(f"\n  FAILURES:")
        for test, traceback in result.failures:
            print(f"    - {test}: {traceback.split(chr(10))[-2].strip()}")

    if result.errors:
        print(f"\n  ERRORS:")
        for test, traceback in result.errors:
            print(f"    - {test}: {traceback.split(chr(10))[-2].strip()}")

    print(f"{'='*60}\n")

    return result.wasSuccessful()


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="OpenMem Real Test Runner")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--class", dest="test_class", help="Run specific test class")

    args = parser.parse_args()
    success = run_tests(verbose=args.verbose, test_class=args.test_class)
    sys.exit(0 if success else 1)
