"""The test of make_map.py. Run: python3 -m unittest discover -s site -p 'test_*.py'"""

import unittest

from make_map import COLUMNS, render


class MakeMapTest(unittest.TestCase):
    def test_one_square_for_each_record(self):
        marks = ["P"] * COLUMNS + ["F", "P", "F"]
        text = "".join(f"a.test\t{i}\t{mark}\n" for i, mark in enumerate(marks))
        svg, score = render(text + "b.test\t1\tP\n", "abc")
        self.assertEqual(
            score,
            {"file": "a.test, b.test", "passed": COLUMNS + 2, "applicable": COLUMNS + 4, "generated_from_commit": "abc"},
        )
        self.assertIn(f"lekha: {COLUMNS + 2} of {COLUMNS + 4} SQLite test queries answered correctly", svg)
        # The first row is full. The second row has 4 squares, and squares 0 and 2 of it fail.
        self.assertIn('d="M0 0h630v5H0zM0 5h20v5h-20z"', svg)
        self.assertIn('d="M0 5h5v5h-5zM10 5h5v5h-5z"', svg)
        self.assertEqual(render(text + "b.test\t1\tP\n", "abc")[0], svg)

    def test_fails_on_a_line_that_it_does_not_understand(self):
        for text in ["", "a.test\t1\n", "a.test\t1\tX\n", "a.test\t1\tP\textra\n"]:
            with self.assertRaises((ValueError, KeyError), msg=repr(text)):
                render(text, "abc")


if __name__ == "__main__":
    unittest.main()
