# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for install_windows.py's check that the installer replaced the program."""

import tempfile
import unittest
from pathlib import Path

from install_windows import same_program


class SameProgram(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def file(self, name, data):
        path = Path(self.dir.name) / name
        path.write_bytes(data)
        return path

    def test_the_program_as_the_installer_packed_it_is_the_same(self):
        built = self.file("built.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_UNK build 20261030.143511 more")
        installed = self.file("installed.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_NSS build 20261030.143511 more")
        self.assertTrue(same_program(installed, built))

    def test_an_older_program_left_in_place_is_not(self):
        built = self.file("built.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_UNK build 20261030.143511 more")
        old = self.file("old.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_NSS build 20261029.091502 more")
        self.assertFalse(same_program(old, built))

    def test_a_program_of_another_size_is_not(self):
        built = self.file("built.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_UNK more")
        old = self.file("old.exe", b"MZ code __TAURI_BUNDLE_TYPE_VAR_NSS more and more")
        self.assertFalse(same_program(old, built))


if __name__ == "__main__":
    unittest.main()
