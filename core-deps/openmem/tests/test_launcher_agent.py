"""
Launcher agent-detection unification tests (Phase 2 hygiene, H-c).

bin/launcher.py detect_agent() must delegate to
agents.base.resolve_agent_adapter() so the launcher shares one detection
order with the learning loop — on this machine that resolves claude_code
from real on-disk session history instead of the generic fallback.
"""

import unittest
import os
import sys
import tempfile
import shutil
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent.parent))


def _load_launcher():
    """Import bin.launcher (namespace package) fresh for assertions."""
    import bin.launcher as launcher
    return launcher


class _FakeAdapter:
    def __init__(self, agent_name):
        self.AGENT_NAME = agent_name


class TestLauncherAgentDetection(unittest.TestCase):

    def setUp(self):
        self.launcher = _load_launcher()
        # Detection must be deterministic regardless of the host env.
        patcher = mock.patch.dict(os.environ, {
            "OPENMEM_AGENT": "",
            "CLAUDE_CODE_WORKSPACE": "",
            "QWEN_CODE_WORKSPACE": "",
            "CODEX_WORKSPACE": "",
        })
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_delegates_to_resolve_agent_adapter(self):
        with mock.patch(
            "agents.base.resolve_agent_adapter",
            return_value=_FakeAdapter("Claude Code"),
        ) as resolver:
            detected = self.launcher.detect_agent()
        resolver.assert_called_once()
        self.assertEqual(detected, "claude_code")

    def test_canonicalizes_multiword_agent_names(self):
        cases = {
            "Claude Code": "claude_code",
            "Codex CLI": "codex_cli",
            "Antigravity IDE": "antigravity_ide",
            "Generic": "generic",
        }
        for agent_name, expected in cases.items():
            with mock.patch(
                "agents.base.resolve_agent_adapter",
                return_value=_FakeAdapter(agent_name),
            ):
                self.assertEqual(self.launcher.detect_agent(), expected)

    def test_env_var_flows_through_shared_resolver(self):
        """OPENMEM_AGENT is honored by resolve_agent_adapter itself; the
        launcher must not need (or keep) its own private env shortcut."""
        with mock.patch.dict(os.environ, {"OPENMEM_AGENT": "qwen_code"}):
            with mock.patch(
                "agents.base.resolve_agent_adapter",
                return_value=_FakeAdapter("Qwen Code"),
            ) as resolver:
                detected = self.launcher.detect_agent()
        resolver.assert_called_once()
        self.assertEqual(detected, "qwen_code")

    def test_falls_back_to_generic_when_resolution_yields_unknown(self):
        with mock.patch(
            "agents.base.resolve_agent_adapter",
            return_value=_FakeAdapter("unknown"),
        ):
            with mock.patch.object(self.launcher.os, "getcwd",
                                   return_value=tempfile.gettempdir()):
                detected = self.launcher.detect_agent()
        self.assertEqual(detected, "generic")

    def test_legacy_cwd_indicator_used_when_resolution_raises(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        os.makedirs(os.path.join(tmp, ".cursor"))

        with mock.patch("agents.base.resolve_agent_adapter",
                        side_effect=RuntimeError("boom")):
            with mock.patch.object(self.launcher.os, "getcwd",
                                   return_value=tmp):
                detected = self.launcher.detect_agent()
        self.assertEqual(detected, "cursor")


if __name__ == "__main__":
    unittest.main()
