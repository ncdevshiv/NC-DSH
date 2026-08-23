from __future__ import annotations

import json
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

from moli_benchmark.wpt_cross.audit import (
    audit_matrix,
    load_known_failure_manifest,
    main,
)


class WptCrossAuditTests(unittest.TestCase):
    def test_audit_matrix_separates_known_unexpected_resolved_and_mismatched(self) -> None:
        matrix = [
            {
                "case_path": "known.html",
                "results": {
                    "moli": {
                        "status": "fail",
                        "failures": [{"name": "subtest", "message": "expected 42 got 7"}],
                    }
                },
            },
            {
                "case_path": "new-failure.html",
                "results": {"moli": {"status": "fail", "error": "boom"}},
            },
            {
                "case_path": "resolved.html",
                "results": {"moli": {"status": "pass"}},
            },
            {
                "case_path": "changed.html",
                "results": {
                    "moli": {
                        "status": "fail",
                        "failures": [{"name": "subtest", "message": "different failure"}],
                    }
                },
            },
        ]
        rules = [
            {
                "case_path": "known.html",
                "category": "known",
                "expected_status": "fail",
                "message_contains": "expected 42",
            },
            {
                "case_path": "resolved.html",
                "category": "known",
                "expected_status": "fail",
            },
            {
                "case_path": "changed.html",
                "category": "known",
                "expected_status": "fail",
                "message_contains": "old failure",
            },
            {
                "case_path": "missing.html",
                "category": "known",
                "expected_status": "fail",
            },
        ]

        audit = audit_matrix(matrix, "moli", rules)

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["counts"]["known_failures"], 1)
        self.assertEqual(audit["counts"]["unexpected_failures"], 1)
        self.assertEqual(audit["counts"]["resolved_known_failures"], 1)
        self.assertEqual(audit["counts"]["mismatched_known_failures"], 1)
        self.assertEqual(audit["counts"]["missing_expected_failures"], 1)
        self.assertEqual(audit["unexpected_failures"][0]["case_path"], "new-failure.html")
        self.assertIn("missing expected failure text", audit["mismatched_known_failures"][0]["note"])
        self.assertEqual(audit["category_counts"]["known_failures"], {"known": 1})
        self.assertEqual(audit["category_counts"]["resolved_known_failures"], {"known": 1})
        self.assertEqual(audit["category_counts"]["mismatched_known_failures"], {"known": 1})
        self.assertEqual(audit["category_counts"]["missing_expected_failures"], {"known": 1})
        self.assertEqual(
            audit["category_counts"]["unexpected_failures"],
            {"uncategorized": 1},
        )
        self.assertEqual(
            audit["known_failures"][0]["message_contains"],
            "expected 42",
        )
        self.assertEqual(
            audit["mismatched_known_failures"][0]["message_contains"],
            "old failure",
        )

    def test_cli_writes_audit_and_returns_nonzero_for_unexpected_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            matrix_path = root / "matrix.json"
            rules_path = root / "known.json"
            output_path = root / "audit.json"
            matrix_path.write_text(
                json.dumps(
                    [
                        {
                            "case_path": "unexpected.html",
                            "results": {"moli": {"status": "fail", "error": "boom"}},
                        }
                    ]
                ),
                encoding="utf-8",
            )
            rules_path.write_text(json.dumps({"rules": []}), encoding="utf-8")

            with redirect_stdout(StringIO()):
                code = main(
                    [
                        "--matrix",
                        str(matrix_path),
                        "--engine",
                        "moli",
                        "--known-failures",
                        str(rules_path),
                        "--output",
                        str(output_path),
                    ]
                )

            self.assertEqual(code, 1)
            audit = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(audit["counts"]["unexpected_failures"], 1)

    def test_cli_returns_nonzero_for_resolved_known_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            matrix_path = root / "matrix.json"
            rules_path = root / "known.json"
            matrix_path.write_text(
                json.dumps(
                    [
                        {
                            "case_path": "known.html",
                            "results": {"moli": {"status": "pass"}},
                        }
                    ]
                ),
                encoding="utf-8",
            )
            rules_path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "known.html",
                                "expected_status": "fail",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with redirect_stdout(StringIO()):
                code = main(
                    [
                        "--matrix",
                        str(matrix_path),
                        "--engine",
                        "moli",
                        "--known-failures",
                        str(rules_path),
                    ]
                )

            self.assertEqual(code, 1)

    def test_audit_matrix_rejects_duplicate_matrix_case_paths(self) -> None:
        matrix = [
            {
                "case_path": "known.html",
                "results": {"moli": {"status": "fail", "error": "old"}},
            },
            {
                "case_path": "known.html",
                "results": {"moli": {"status": "pass"}},
            },
        ]

        with self.assertRaisesRegex(ValueError, "duplicate case_path"):
            audit_matrix(matrix, "moli", [])

    def test_audit_matrix_rejects_matrix_rows_without_case_path(self) -> None:
        matrix = [{"results": {"moli": {"status": "fail", "error": "boom"}}}]

        with self.assertRaisesRegex(ValueError, "must have case_path"):
            audit_matrix(matrix, "moli", [])

    def test_manifest_loader_rejects_duplicate_case_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {"case_path": "same.html", "expected_status": "fail"},
                            {"case_path": "same.html", "expected_status": "fail"},
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "duplicate known-failure rule"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_invalid_case_paths(self) -> None:
        invalid_case_paths = [
            "/wasm/known.html",
            "../wasm/known.html",
            "wasm/../known.html",
            "wasm//known.html",
            "wasm/known.html#subtest",
            "https://example.test/wasm/known.html",
            "?moli-wpt-any=window",
            "wasm\\known.html",
        ]
        for case_path in invalid_case_paths:
            with self.subTest(case_path=case_path):
                with tempfile.TemporaryDirectory() as temp_dir:
                    path = Path(temp_dir) / "known.json"
                    path.write_text(
                        json.dumps(
                            {
                                "rules": [
                                    {
                                        "case_path": case_path,
                                        "expected_status": "fail",
                                    }
                                ]
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, "relative WPT case path"):
                        load_known_failure_manifest(path)

    def test_manifest_loader_accepts_query_variant_case_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": (
                                    "wasm/jsapi/idlharness.any.js"
                                    "?moli-wpt-any=window"
                                ),
                                "expected_status": "fail",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            manifest = load_known_failure_manifest(path)

        self.assertEqual(len(manifest["rules"]), 1)

    def test_manifest_loader_validates_rule_shapes_up_front(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "bad.html",
                                "expected_status": 1,
                                "message_contains": {},
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "invalid expected_status"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_empty_expected_status(self) -> None:
        cases = ["", "   ", [], ["fail", ""], ["fail", "   "]]
        for expected_status in cases:
            with self.subTest(expected_status=expected_status):
                with tempfile.TemporaryDirectory() as temp_dir:
                    path = Path(temp_dir) / "known.json"
                    path.write_text(
                        json.dumps(
                            {
                                "rules": [
                                    {
                                        "case_path": "bad.html",
                                        "expected_status": expected_status,
                                    }
                                ]
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, "invalid expected_status"):
                        load_known_failure_manifest(path)

    def test_manifest_loader_rejects_unknown_or_passing_expected_status(self) -> None:
        cases = ["pass", "typo", ["fail", "pass"], ["timeout", "typo"]]
        for expected_status in cases:
            with self.subTest(expected_status=expected_status):
                with tempfile.TemporaryDirectory() as temp_dir:
                    path = Path(temp_dir) / "known.json"
                    path.write_text(
                        json.dumps(
                            {
                                "rules": [
                                    {
                                        "case_path": "bad.html",
                                        "expected_status": expected_status,
                                    }
                                ]
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, "invalid expected_status"):
                        load_known_failure_manifest(path)

    def test_manifest_loader_accepts_non_pass_expected_statuses(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "bad.html",
                                "expected_statuses": [
                                    "fail",
                                    "timeout",
                                    "crash",
                                    "harness-stalled",
                                    "error",
                                    "missing",
                                ],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            manifest = load_known_failure_manifest(path)

        self.assertEqual(len(manifest["rules"]), 1)

    def test_manifest_loader_rejects_empty_message_contains(self) -> None:
        cases = ["", "   ", ["expected", ""], ["expected", "   "]]
        for message_contains in cases:
            with self.subTest(message_contains=message_contains):
                with tempfile.TemporaryDirectory() as temp_dir:
                    path = Path(temp_dir) / "known.json"
                    path.write_text(
                        json.dumps(
                            {
                                "rules": [
                                    {
                                        "case_path": "bad.html",
                                        "expected_status": "fail",
                                        "message_contains": message_contains,
                                    }
                                ]
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, "invalid message_contains"):
                        load_known_failure_manifest(path)

    def test_manifest_loader_validates_declared_categories(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text(
                "# local\n\n## 7. webapi shape\n",
                encoding="utf-8",
            )
            wasm = root / "wpt" / "wasm"
            wasm.mkdir(parents=True)
            (wasm / "known.html").write_text("<!doctype html>", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md#local",
                                        "note": "local fixture evidence",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wpt/wasm/known.html",
                                        "note": "rule source fixture",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            manifest = load_known_failure_manifest(path)

            self.assertEqual(
                manifest["categories"]["known"]["tracking_doc"],
                "docs/wasm-current.md",
            )
            self.assertEqual(
                manifest["categories"]["known"]["evidence"][0]["path"],
                "docs/wasm-current.md#local",
            )
            self.assertEqual(
                manifest["categories"]["known"]["evidence"][0]["kind"],
                "doc",
            )

    def test_manifest_loader_accepts_numbered_doc_evidence_anchors(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text(
                "# wasm\n\n## 7. webapi shape\n",
                encoding="utf-8",
            )
            wasm = root / "wpt" / "wasm"
            wasm.mkdir(parents=True)
            (wasm / "known.html").write_text("<!doctype html>", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md#7",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md#7",
                                        "note": "numbered section evidence",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wpt/wasm/known.html",
                                        "note": "rule source fixture",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            manifest = load_known_failure_manifest(path)

            self.assertEqual(
                manifest["categories"]["known"]["evidence"][0]["path"],
                "docs/wasm-current.md#7",
            )

    def test_manifest_loader_rejects_missing_doc_evidence_anchor(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md#missing",
                                        "note": "stale section evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "document anchor does not exist"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_categorized_rule_without_matching_wpt_evidence(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "category overview",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "must have matching wpt evidence"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_wpt_evidence_outside_wpt_tree(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            wasm = root / "wasm"
            wasm.mkdir()
            (wasm / "known.html").write_text("<!doctype html>", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "category overview",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wasm/known.html",
                                        "note": "local file is not a WPT source path",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "must have matching wpt evidence"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_categorized_rules_without_evidence(self) -> None:
        cases = [
            (
                {
                    "case_path": "known.html",
                    "category": "known",
                    "expected_status": "fail",
                    "message_contains": "expected failure",
                },
                "must have reason",
            ),
            (
                {
                    "case_path": "known.html",
                    "category": "known",
                    "message_contains": "expected failure",
                    "reason": "documented known failure",
                },
                "must have expected_status",
            ),
            (
                {
                    "case_path": "known.html",
                    "category": "known",
                    "expected_status": "fail",
                    "reason": "documented known failure",
                },
                "must have message_contains",
            ),
        ]
        for rule, expected_error in cases:
            with self.subTest(expected_error=expected_error):
                with tempfile.TemporaryDirectory() as temp_dir:
                    root = Path(temp_dir)
                    docs = root / "docs"
                    docs.mkdir()
                    (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
                    path = root / "known.json"
                    path.write_text(
                        json.dumps(
                            {
                                "categories": {
                                    "known": {
                                        "tracking_doc": "docs/wasm-current.md",
                                        "scope": "test category",
                                        "evidence": [
                                            {
                                                "kind": "doc",
                                                "path": "docs/wasm-current.md",
                                                "note": "local fixture evidence",
                                            }
                                        ],
                                    }
                                },
                                "rules": [rule],
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, expected_error):
                        load_known_failure_manifest(path)

    def test_manifest_loader_rejects_unknown_declared_category(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "unknown.html",
                                "category": "unknown",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "unknown category"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_unused_declared_category(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            wpt = root / "wpt" / "wasm"
            wpt.mkdir(parents=True)
            (wpt / "known.html").write_text("<!doctype html>", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "tracked failure",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "category overview",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wpt/wasm/known.html",
                                        "note": "source case",
                                    },
                                ],
                            },
                            "stale": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "stale failure category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "stale category overview",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wpt/wasm/known.html",
                                        "note": "source case",
                                    },
                                ],
                            },
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "known",
                                "expected_status": "fail",
                                "message_contains": "expected failure",
                                "reason": "documented known failure",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "not referenced by any rule"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_category_without_tracking_doc(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {"known": {"scope": "test category"}},
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "tracking_doc must be a string"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_missing_category_tracking_doc(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/missing.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "tracking_doc does not exist"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_absolute_category_tracking_doc(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            tracking_doc = docs / "wasm-current.md"
            tracking_doc.write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": str(tracking_doc),
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "tracking_doc must be relative"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_category_without_scope(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {"tracking_doc": "docs/wasm-current.md"}
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "scope must be a string"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_category_without_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence must be"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_category_with_unresolved_evidence_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/missing.md#section",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence #1 path does not exist"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_absolute_evidence_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            evidence = docs / "wasm-current.md"
            evidence.write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "path": str(evidence),
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence #1 path must be relative"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_anchor_only_evidence_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "path": "#section",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence #1 path does not exist"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_directory_evidence_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": [
                                    {
                                        "path": "docs",
                                        "note": "local fixture evidence",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence #1 path does not exist"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_unstructured_category_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            path = root / "known.json"
            path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "known": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "test category",
                                "evidence": ["local fixture evidence"],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "known.html",
                                "category": "known",
                                "expected_status": "fail",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "evidence #1 must be an object"):
                load_known_failure_manifest(path)

    def test_manifest_loader_rejects_category_evidence_without_known_kind(self) -> None:
        cases = [
            ({}, "kind must be one of"),
            ({"kind": "note"}, "kind must be one of"),
        ]
        for extra, expected_error in cases:
            with self.subTest(extra=extra):
                with tempfile.TemporaryDirectory() as temp_dir:
                    root = Path(temp_dir)
                    docs = root / "docs"
                    docs.mkdir()
                    (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
                    path = root / "known.json"
                    evidence = {
                        "path": "docs/wasm-current.md",
                        "note": "local fixture evidence",
                        **extra,
                    }
                    path.write_text(
                        json.dumps(
                            {
                                "categories": {
                                    "known": {
                                        "tracking_doc": "docs/wasm-current.md",
                                        "scope": "test category",
                                        "evidence": [evidence],
                                    }
                                },
                                "rules": [
                                    {
                                        "case_path": "known.html",
                                        "category": "known",
                                        "expected_status": "fail",
                                    }
                                ],
                            }
                        ),
                        encoding="utf-8",
                    )

                    with self.assertRaisesRegex(ValueError, expected_error):
                        load_known_failure_manifest(path)

    def test_cli_returns_zero_for_known_failure_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            docs = root / "docs"
            docs.mkdir()
            (docs / "wasm-current.md").write_text("# wasm\n", encoding="utf-8")
            wasm = root / "wpt" / "wasm"
            wasm.mkdir(parents=True)
            (wasm / "known.html").write_text("<!doctype html>", encoding="utf-8")
            matrix_path = root / "matrix.json"
            rules_path = root / "known.json"
            output_path = root / "audit.json"
            matrix_path.write_text(
                json.dumps(
                    [
                        {
                            "case_path": "wasm/known.html",
                            "results": {
                                "moli": {
                                    "status": "fail",
                                    "failures": [{"message": "expected 555 but got 100"}],
                                }
                            },
                        }
                    ]
                ),
                encoding="utf-8",
            )
            rules_path.write_text(
                json.dumps(
                    {
                        "categories": {
                            "wasm": {
                                "tracking_doc": "docs/wasm-current.md",
                                "scope": "focused wasm failure",
                                "evidence": [
                                    {
                                        "kind": "doc",
                                        "path": "docs/wasm-current.md",
                                        "note": "local fixture evidence",
                                    },
                                    {
                                        "kind": "wpt",
                                        "path": "wpt/wasm/known.html",
                                        "note": "rule source fixture",
                                    }
                                ],
                            }
                        },
                        "rules": [
                            {
                                "case_path": "wasm/known.html",
                                "category": "wasm",
                                "expected_status": "fail",
                                "message_contains": "expected 555",
                                "reason": "tracked focused wasm failure",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with redirect_stdout(StringIO()):
                code = main(
                    [
                        "--matrix",
                        str(matrix_path),
                        "--engine",
                        "moli",
                        "--known-failures",
                        str(rules_path),
                        "--output",
                        str(output_path),
                    ]
                )

            self.assertEqual(code, 0)
            audit = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(
                audit["categories"]["wasm"]["tracking_doc"],
                "docs/wasm-current.md",
            )
            self.assertEqual(audit["categories"]["wasm"]["scope"], "focused wasm failure")

    def test_audit_matrix_can_skip_unrun_known_failures_for_focused_run(self) -> None:
        matrix = [
            {
                "case_path": "known.html",
                "results": {
                    "moli": {
                        "status": "fail",
                        "failures": [{"message": "expected 555 but got 100"}],
                    }
                },
            }
        ]
        rules = [
            {
                "case_path": "known.html",
                "category": "wasm",
                "expected_status": "fail",
                "message_contains": "expected 555",
            },
            {
                "case_path": "not-run.html",
                "category": "wasm",
                "expected_status": "fail",
            },
        ]

        audit = audit_matrix(
            matrix,
            "moli",
            rules,
            categories={
                "wasm": {
                    "tracking_doc": "docs/wasm-current.md",
                    "scope": "focused wasm failure",
                    "evidence": [
                        {
                            "kind": "doc",
                            "path": "docs/wasm-current.md",
                            "note": "local fixture evidence",
                        }
                    ],
                }
            },
            allow_missing_known_failures=True,
        )

        self.assertTrue(audit["ok"])
        self.assertEqual(audit["categories"]["wasm"]["scope"], "focused wasm failure")
        self.assertEqual(audit["counts"]["known_failures"], 1)
        self.assertEqual(audit["counts"]["missing_expected_failures"], 0)
        self.assertEqual(audit["counts"]["skipped_known_failures"], 1)
        self.assertEqual(audit["skipped_known_failures"][0]["case_path"], "not-run.html")
        self.assertEqual(audit["category_counts"]["skipped_known_failures"], {"wasm": 1})

    def test_cli_allow_missing_known_failures_returns_zero_for_focused_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            matrix_path = root / "matrix.json"
            rules_path = root / "known.json"
            output_path = root / "audit.json"
            matrix_path.write_text(
                json.dumps(
                    [
                        {
                            "case_path": "known.html",
                            "results": {
                                "moli": {
                                    "status": "fail",
                                    "failures": [{"message": "expected 555 but got 100"}],
                                }
                            },
                        }
                    ]
                ),
                encoding="utf-8",
            )
            rules_path.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "known.html",
                                "expected_status": "fail",
                                "message_contains": "expected 555",
                            },
                            {
                                "case_path": "not-run.html",
                                "expected_status": "fail",
                            },
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with redirect_stdout(StringIO()):
                code = main(
                    [
                        "--matrix",
                        str(matrix_path),
                        "--engine",
                        "moli",
                        "--known-failures",
                        str(rules_path),
                        "--output",
                        str(output_path),
                        "--allow-missing-known-failures",
                    ]
                )

            self.assertEqual(code, 0)
            audit = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(audit["counts"]["known_failures"], 1)
            self.assertEqual(audit["counts"]["missing_expected_failures"], 0)
            self.assertEqual(audit["counts"]["skipped_known_failures"], 1)

    def test_known_failure_message_can_match_harness_message(self) -> None:
        matrix = [
            {
                "case_path": "cycle.html",
                "results": {
                    "moli": {
                        "status": "fail",
                        "harness_status_name": "ERROR",
                        "harness_message": "Unhandled rejection: cyclic wasm dependency",
                    }
                },
            }
        ]
        rules = [
            {
                "case_path": "cycle.html",
                "expected_status": "fail",
                "message_contains": "cyclic wasm dependency",
            }
        ]

        audit = audit_matrix(matrix, "moli", rules)

        self.assertTrue(audit["ok"])
        self.assertEqual(audit["counts"]["known_failures"], 1)
        self.assertEqual(
            audit["known_failures"][0]["harness_message"],
            "Unhandled rejection: cyclic wasm dependency",
        )
        self.assertEqual(
            audit["known_failures"][0]["message_contains"],
            "cyclic wasm dependency",
        )


if __name__ == "__main__":
    unittest.main()
