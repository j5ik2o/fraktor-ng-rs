#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Tests for scripts/pub_api_diff.py.

Usage:
    python3 scripts/pub_api_diff_test.py
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pub_api_diff import CrateDiff, compute_diffs, item_path, render_markdown  # noqa: E402


class ItemPathTest(unittest.TestCase):
    def test_takes_the_first_path_of_the_line(self) -> None:
        cases = {
            "pub fn fraktor_actor::kernel::ActorSystem::new(id: crate::Id) -> Self": "fraktor_actor::kernel::ActorSystem::new",
            "pub unsafe trait fraktor_actor::kernel::sync::RawLock": "fraktor_actor::kernel::sync::RawLock",
            "pub struct fraktor_testkit::test_kit::ClassOf<T>": "fraktor_testkit::test_kit::ClassOf",
            "pub const fraktor_testkit::coroner::DEFAULT: Duration": "fraktor_testkit::coroner::DEFAULT",
            "pub type fraktor_actor::Mdc = alloc::collections::BTreeMap<String, String>": "fraktor_actor::Mdc",
        }
        for line, path in cases.items():
            self.assertEqual(item_path(line), path, line)


class ComputeDiffsTest(unittest.TestCase):
    def test_separates_new_paths_from_changed_signatures(self) -> None:
        base = {
            "fraktor-actor": [
                "pub struct fraktor_actor::kernel::ActorSystem",
                "pub fn fraktor_actor::kernel::ActorSystem::old(&self)",
                "pub fn fraktor_actor::kernel::ActorSystem::mark(&mut self, limit: usize)",
            ]
        }
        head = {
            "fraktor-actor": [
                "pub struct fraktor_actor::kernel::ActorSystem",
                "pub fn fraktor_actor::kernel::ActorSystem::actor_of_named(&self)",
                "pub fn fraktor_actor::kernel::ActorSystem::mark(&mut self, _limit: usize)",
            ],
            "fraktor-testkit": ["pub struct fraktor_testkit::TestKit"],
        }
        self.assertEqual(
            compute_diffs(base, head),
            [
                CrateDiff(
                    crate="fraktor-actor",
                    added=("pub fn fraktor_actor::kernel::ActorSystem::actor_of_named(&self)",),
                    changed=("pub fn fraktor_actor::kernel::ActorSystem::mark(&mut self, _limit: usize)",),
                    removed_count=1,
                ),
                CrateDiff(crate="fraktor-testkit", added=("pub struct fraktor_testkit::TestKit",), changed=(), removed_count=0),
            ],
        )


class RenderMarkdownTest(unittest.TestCase):
    def test_no_additions(self) -> None:
        md = render_markdown([CrateDiff(crate="fraktor-actor", added=(), changed=(), removed_count=0)], "aaa1111", "bbb2222")
        self.assertIn("Newly added `pub` items: **0**", md)
        self.assertIn("No new `pub` items added.", md)
        self.assertNotIn("<details>", md)

    def test_lists_additions_by_crate_and_folds_the_changes(self) -> None:
        diffs = [
            CrateDiff(
                crate="fraktor-actor",
                added=("pub fn fraktor_actor::kernel::ActorSystem::actor_of_named(&self)",),
                changed=("pub fn fraktor_actor::kernel::ActorSystem::mark(&mut self, _limit: usize)",),
                removed_count=0,
            )
        ]
        md = render_markdown(diffs, "aaa1111", "worktree")
        self.assertIn("### `fraktor-actor` (+1)", md)
        self.assertIn("+ pub fn fraktor_actor::kernel::ActorSystem::actor_of_named(&self)", md)
        self.assertIn("signature changed: 1", md)
        details = md.index("<details>")
        self.assertGreater(md.index("_limit"), details)


if __name__ == "__main__":
    unittest.main()
