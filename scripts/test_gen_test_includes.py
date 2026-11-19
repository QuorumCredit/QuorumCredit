#!/usr/bin/env python3
"""Regression tests for scripts/gen_test_includes.py (issue #2097).

A generated-block END marker on the last line of src/tests.rs, without a trailing
newline, must not crash the generator, and content after the block must be kept.
"""

from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from scripts import gen_test_includes as gen


class SpliceBlockTests(unittest.TestCase):
    def setUp(self) -> None:
        self.block = "NEW BLOCK\n"

    def _text(self, tail: str) -> str:
        return f"use x;\n{gen.BEGIN}\nOLD\n{gen.END}{tail}"

    def test_marker_with_trailing_newline_keeps_following_content(self) -> None:
        out = gen.splice_block(self._text("\nfn after() {}\n"), self.block)
        self.assertEqual(out, "use x;\nNEW BLOCK\nfn after() {}\n")

    def test_marker_as_last_line_without_trailing_newline_does_not_raise(self) -> None:
        out = gen.splice_block(self._text(""), self.block)
        self.assertEqual(out, "use x;\nNEW BLOCK\n")

    def test_marker_as_last_line_with_trailing_newline_is_idempotent(self) -> None:
        block = gen.generated_block()
        once = gen.splice_block(self._text("\n"), block)
        twice = gen.splice_block(once, block)
        self.assertEqual(once, twice)
        self.assertEqual(once.count(gen.BEGIN), 1)

    def test_missing_markers_appends_block_and_trailer(self) -> None:
        out = gen.splice_block("use x;\n", self.block)
        self.assertTrue(out.startswith(self.block))
        self.assertIn(gen.TRAILER.strip(), out)


if __name__ == "__main__":
    unittest.main()
