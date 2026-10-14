import unittest

from textkit import slugify


class SlugTests(unittest.TestCase):
    def test_basic(self):
        self.assertEqual(slugify("Hello World"), "hello-world")

    def test_accents_become_ascii(self):
        self.assertEqual(slugify("Héllo Wörld"), "hello-world")

    def test_runs_of_separators_collapse(self):
        self.assertEqual(slugify("a  --  b"), "a-b")

    def test_no_leading_or_trailing_dash(self):
        self.assertEqual(slugify("  hi!  "), "hi")

    def test_max_length(self):
        self.assertEqual(slugify("x" * 100, max_length=10), "x" * 10)
        self.assertEqual(slugify("hello wonderful world", max_length=11), "hello-wonde")


if __name__ == "__main__":
    unittest.main()
