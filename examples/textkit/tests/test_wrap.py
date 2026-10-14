import unittest

from textkit import wrap


class WrapTests(unittest.TestCase):
    def test_fills_lines_up_to_width(self):
        self.assertEqual(wrap("aaa bbb ccc", 7), ["aaa bbb", "ccc"])

    def test_exact_fit(self):
        self.assertEqual(wrap("ab cd", 5), ["ab cd"])

    def test_never_exceeds_width(self):
        for line in wrap("the quick brown fox jumps over the lazy dog", 10):
            self.assertLessEqual(len(line), 10)

    def test_long_word_gets_its_own_line(self):
        self.assertEqual(wrap("a extraordinarily b", 6), ["a", "extraordinarily", "b"])


if __name__ == "__main__":
    unittest.main()
