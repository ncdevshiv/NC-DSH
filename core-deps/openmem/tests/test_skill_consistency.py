"""
Consistency tests for generated agent skill artifacts.

Guards against copy-paste drift between bin/generate_skills.py and the
committed files under agents/<agent>/skill/:

1. Every agent registered in generate_skills.AGENTS has all three artifact
   files (SKILL.md, learner.py, config.json) present on disk.
2. The committed artifacts are exactly the generator's output (byte-for-byte),
   so manual edits to generated files fail here — change the template instead.
3. All learner.py files share one common body: after replacing the known
   per-agent substitutions (display name, trigger, agent key) the remaining
   text is identical across every registered agent.
"""

import importlib.util
import json
import sys
import unittest
from pathlib import Path

# Add parent to path
sys.path.insert(0, str(Path(__file__).parent.parent))

BASE_DIR = Path(__file__).parent.parent

# bin/ is not a package; load the generator module directly by path.
_SPEC = importlib.util.spec_from_file_location(
    "generate_skills", str(BASE_DIR / "bin" / "generate_skills.py")
)
generate_skills = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(generate_skills)

AGENTS = generate_skills.AGENTS
ARTIFACT_FILENAMES = generate_skills.ARTIFACT_FILENAMES
SKILL_ROOT = BASE_DIR / "agents"


def normalize_learner(text, agent_key):
    """Replace known per-agent substitution tokens with named placeholders."""
    info = AGENTS[agent_key]
    for name, token in [
        ("DISPLAY", info["display"]),
        ("TRIGGER", info["trigger"]),
        ("KEY", agent_key),
    ]:
        text = text.replace(token, "<<%s>>" % name)
    return text


def differing_line_count(text_a, text_b):
    """Count positions where two line-split texts disagree."""
    lines_a = text_a.splitlines()
    lines_b = text_b.splitlines()
    n = max(len(lines_a), len(lines_b))
    diff = 0
    for i in range(n):
        a = lines_a[i] if i < len(lines_a) else None
        b = lines_b[i] if i < len(lines_b) else None
        if a != b:
            diff += 1
    return diff, n


class TestArtifactPresence(unittest.TestCase):
    """Every registered agent must have a complete skill directory."""

    def test_all_registered_agents_have_all_artifacts(self):
        for agent_key in AGENTS:
            for fname in ARTIFACT_FILENAMES:
                path = SKILL_ROOT / agent_key / "skill" / fname
                self.assertTrue(
                    path.is_file(),
                    "Missing artifact: %s" % path,
                )

    def test_registry_covers_generic_fallback(self):
        # agents/generic/adapter.py is the auto-detect fallback (base.py
        # auto_detect_adapter); it must have a registered skill payload too.
        self.assertIn("generic", AGENTS)
        self.assertEqual(AGENTS["generic"]["display"], "Any Agent")

    def test_agent_dirs_match_registry(self):
        for skill_dir in sorted(SKILL_ROOT.glob("*/skill")):
            agent_key = skill_dir.parent.name
            self.assertIn(
                agent_key,
                AGENTS,
                "Skill dir %s has no entry in generate_skills.AGENTS" % skill_dir,
            )


class TestCommittedArtifactsMatchGenerator(unittest.TestCase):
    """Committed artifacts must equal generator output byte-for-byte."""

    def test_committed_files_equal_rendered_bytes(self):
        for agent_key in AGENTS:
            rendered = generate_skills.render_bytes(agent_key)
            self.assertEqual(sorted(rendered.keys()), sorted(ARTIFACT_FILENAMES))
            for fname in ARTIFACT_FILENAMES:
                path = SKILL_ROOT / agent_key / "skill" / fname
                with self.subTest(agent=agent_key, artifact=fname):
                    expected = rendered[fname]
                    actual = path.read_bytes()
                    self.assertEqual(
                        actual,
                        expected,
                        "%s drifted from generator output; "
                        "edit bin/generate_skills.py and regenerate "
                        "instead of hand-editing" % path,
                    )

    def test_generator_output_is_deterministic(self):
        # Two builds in the same process must agree — guards against
        # timestamps or other nondeterminism creeping back into templates.
        for agent_key in AGENTS:
            first = generate_skills.render_bytes(agent_key)
            second = generate_skills.render_bytes(agent_key)
            self.assertEqual(first, second, "Generator output not deterministic for %s" % agent_key)


class TestLearnerBodyConsistency(unittest.TestCase):
    """All learner.py files are clones differing only in substituted tokens."""

    def _learner_text(self, agent_key):
        return (SKILL_ROOT / agent_key / "skill" / "learner.py").read_bytes().decode("utf-8")

    def test_normalized_bodies_identical_across_agents(self):
        normalized = {
            key: normalize_learner(self._learner_text(key), key) for key in AGENTS
        }
        reference_key = sorted(AGENTS.keys())[0]
        reference = normalized[reference_key]
        for agent_key, body in normalized.items():
            self.assertEqual(
                body,
                reference,
                "%s learner.py diverges from %s beyond per-agent substitutions"
                % (agent_key, reference_key),
            )

    def test_max_pairwise_divergence_under_10_percent(self):
        keys = sorted(AGENTS.keys())
        for i, key_a in enumerate(keys):
            for key_b in keys[i + 1:]:
                diff, total = differing_line_count(
                    self._learner_text(key_a), self._learner_text(key_b)
                )
                pct = 100.0 * diff / total
                with self.subTest(pair=(key_a, key_b)):
                    self.assertLess(
                        pct,
                        10.0,
                        "%s ~ %s: %.2f%% differing lines (%d/%d)"
                        % (key_a, key_b, pct, diff, total),
                    )

    def test_substitutions_actually_occur(self):
        # Sanity: the substitution tokens really appear in each learner, so
        # the identity assertion above cannot pass vacuously.
        for agent_key in AGENTS:
            text = self._learner_text(agent_key)
            info = AGENTS[agent_key]
            self.assertIn(info["display"], text)
            self.assertIn(info["trigger"], text)
            self.assertIn(agent_key, text)


class TestSkillConfigSchema(unittest.TestCase):
    """config.json carries the registry values consumers may rely on."""

    def test_config_matches_registry(self):
        for agent_key, info in AGENTS.items():
            path = SKILL_ROOT / agent_key / "skill" / "config.json"
            config = json.loads(path.read_bytes().decode("utf-8"))
            with self.subTest(agent=agent_key):
                self.assertEqual(config.get("agent"), info["display"])
                self.assertEqual(config.get("agent_key"), agent_key)
                self.assertEqual(config.get("trigger"), info["trigger"])
                self.assertEqual(config.get("context_file"), info["context_file"])
                self.assertEqual(
                    config.get("workspace_env"), info.get("workspace_env", "")
                )
                self.assertEqual(config.get("version"), "1.0.0")


if __name__ == "__main__":
    unittest.main()
