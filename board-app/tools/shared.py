"""Copies the parts the Board app shares with the Names app (palette, module
calls, buttons, fields, avatars) out of basecamp/Main.qml into
board-app/Shared.qml.part, which board-app/Main.qml.in includes. Run it after
changing those parts in the Names app:  python3 board-app/tools/shared.py
"""
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(HERE, "..", "..", "basecamp", "Main.qml")
TEMPLATE = os.path.join(HERE, "..", "Main.qml.in")
OUT = os.path.join(HERE, "..", "Main.qml")

COMPONENTS = ["Seal", "Action", "IconButton", "Link", "Field", "Area", "Chip", "Tab", "Card", "Hint", "Part", "Avatar", "NameLabel"]
FUNCTIONS = ["parsed", "call", "ask", "askLater", "seedOf", "base64Bytes", "readable", "clock", "dated", "copy", "lastLogLine"]
PROPERTIES = ["slate", "panel", "raised", "hover", "line", "ink", "dim", "faint", "brass", "alarm", "sans", "mono", "bridgeReady"]

text = open(SOURCE).read()
lines = text.split("\n")

def block(start):
    """Lines from `start` to the line closing the brace opened on or after it."""
    depth, opened = 0, False
    for i in range(start, len(lines)):
        depth += lines[i].count("{") - lines[i].count("}")
        opened = opened or "{" in lines[i]
        if opened and depth == 0:
            return lines[start:i + 1]
    raise SystemExit("unbalanced braces from line %d" % (start + 1))

def with_docs(start):
    """Back up over the /// comment lines above `start`."""
    while start > 0 and lines[start - 1].strip().startswith("///"):
        start -= 1
    return start

parts = []
for name in PROPERTIES:
    for i, line in enumerate(lines):
        if re.match(r"\s{4}readonly property \w+ %s:" % name, line):
            parts.append(line)
            break
    else:
        raise SystemExit("no property " + name)
for name in FUNCTIONS:
    for i, line in enumerate(lines):
        if line.startswith("    function %s(" % name):
            start = with_docs(i)
            parts.append("\n".join(lines[start:i] + block(i)))
            break
    else:
        raise SystemExit("no function " + name)
for name in COMPONENTS:
    for i, line in enumerate(lines):
        if line.startswith("    component %s:" % name):
            start = with_docs(i)
            parts.append("\n".join(lines[start:i] + block(i)))
            break
    else:
        raise SystemExit("no component " + name)

shared = "    // ── Shared with the Names app (copied by tools/shared.py; edit there) ──\n\n" + "\n\n".join(parts) + "\n"
template = open(TEMPLATE).read()
assert "//@SHARED@" in template
open(OUT, "w").write(template.replace("    //@SHARED@\n", shared))
print("wrote", os.path.normpath(OUT))
