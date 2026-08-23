#!/usr/bin/env python3
"""Setuptools shim for OpenMem. All metadata lives in pyproject.toml.

Historical note: this file previously contained an unrelated OpenClaw
integration module (no setup() call), which broke legacy builds that execute
setup.py directly. That content now lives in skills/openclaw/legacy_setup_module.py.
"""

from setuptools import setup

setup()
