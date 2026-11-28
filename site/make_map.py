"""Draws the score map from the file that the test harness writes with SLT_MAP.

Usage: python3 site/make_map.py <map file> <commit>

Each line of the map file is <file>, <line> and P or F, with a tab between them.
There is one line for each applicable test record, in the order of the run.
Writes score-map.svg and score.json next to this script.
Python 3, standard library only.
"""

import json
import sys
from pathlib import Path

COLUMNS = 126
PITCH = 5  # The side of one square with its gap. site.css shows the image at this size.
MARGIN = 10
TOP = 36  # The caption and the legend are above the squares.
BACK, TEXT, PASS, FAIL = "#0d1117", "#e6edf3", "#2ea043", "#ffb224"


def square(index, width=1):
    x, y = index % COLUMNS * PITCH, index // COLUMNS * PITCH
    return f"M{x} {y}h{width * PITCH}v{PITCH}h-{width * PITCH}z"


def render(text, commit):
    """Returns the SVG and the score for the text of a map file."""
    files, cells = [], []
    for line in text.splitlines():
        file, _, mark = line.split("\t")
        if file not in files:
            files.append(file)
        cells.append({"P": True, "F": False}[mark])
    if not cells:
        raise ValueError("the map file has no record")
    score = {
        "file": ", ".join(files),
        "passed": sum(cells),
        "applicable": len(cells),
        "generated_from_commit": commit,
    }
    caption = f"lekha: {score['passed']:,} of {len(cells):,} SQLite test queries answered correctly"
    rows = -(-len(cells) // COLUMNS)
    width, height = COLUMNS * PITCH + 2 * MARGIN, TOP + rows * PITCH + MARGIN
    # One green area for all the squares, then the squares that fail, then the gaps as lines on top.
    full = len(cells) // COLUMNS * COLUMNS
    area = f"M0 0h{COLUMNS * PITCH}v{full // COLUMNS * PITCH}H0z"
    if len(cells) > full:
        area += square(full, len(cells) - full)
    fails = "".join(square(i) for i, passed in enumerate(cells) if not passed)
    gaps = "".join(f"M{c * PITCH} 0v{rows * PITCH}" for c in range(COLUMNS + 1))
    gaps += "".join(f"M0 {r * PITCH}h{COLUMNS * PITCH}" for r in range(rows + 1))
    legend = width - MARGIN - 112
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="{caption}">
<title>{caption}</title>
<rect width="{width}" height="{height}" rx="8" fill="{BACK}"/>
<g fill="{TEXT}" font-family="system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif" font-size="12">
<text x="{MARGIN}" y="24" font-size="14" font-weight="600">{caption}</text>
<rect x="{legend}" y="15" width="9" height="9" fill="{PASS}"/>
<text x="{legend + 14}" y="24">passes</text>
<rect x="{legend + 66}" y="15" width="9" height="9" fill="{FAIL}"/>
<text x="{legend + 80}" y="24">fails</text>
</g>
<g transform="translate({MARGIN} {TOP})">
<path fill="{PASS}" d="{area}"/>
<path fill="{FAIL}" d="{fails}"/>
<path stroke="{BACK}" d="{gaps}"/>
</g>
</svg>
"""
    return svg, score


if __name__ == "__main__":
    svg, score = render(Path(sys.argv[1]).read_text(encoding="utf-8"), sys.argv[2])
    site = Path(__file__).parent
    (site / "score-map.svg").write_text(svg, encoding="utf-8")
    (site / "score.json").write_text(json.dumps(score) + "\n", encoding="utf-8")
