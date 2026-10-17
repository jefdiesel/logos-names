#!/usr/bin/env python3
"""Renders the Board app's Main.qml to PNG files without Basecamp, with a stand-in
for the names module.

Usage: render.py <qml binary> <output dir> [scenario ...]
Needs QML_IMPORT_PATH and QT_PLUGIN_PATH set for a Qt 6 install, and
QT_QPA_PLATFORM=offscreen to run without a display.
"""
import json, os, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
NOW = int(time.time() * 1000)

POSTS = [
    {"post": "ada/1", "name": "ada", "n": 1, "parent": "jef/0", "time": NOW - 4 * 60_000,
     "text": "Welcome! First reply on the board.", "cids": [], "likes": 2, "hidden": False, "block": 30501},
    {"post": "jef/0", "name": "jef", "n": 0, "parent": None, "time": NOW - 25 * 60_000,
     "text": "First post on the LNS board. Every post here is made privately as a name: the chain sees jef, never my account.",
     "cids": ["zDvZRwzm8K4nJ2a7q9WTqN1"], "likes": 5, "hidden": False, "block": 30420},
    {"post": "hopper/3", "name": "hopper", "n": 3, "parent": None, "time": NOW - 3 * 3_600_000,
     "text": "A ship in port is safe, but that is not what ships are built for.", "cids": [], "likes": 0, "hidden": False, "block": 29800},
]
PEOPLE = {
    "jef": {"display": "Jef", "color": "#96d678"},
    "ada": {"display": "Ada Lovelace", "color": "#c3a6ff"},
    "hopper": {"display": "Grace Hopper", "color": "#7fa8ff"},
}

BRIDGE = r"""
    width: 1180; height: %(height)s
    property var logos: QtObject {
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function callModuleAsync(module, method, args, callback, timeout) {
            var reply = callModule(module, method, args)
            Qt.callLater(function() { callback(reply) })
        }
        function callModule(module, method, args) {
            var posts = %(posts)s
            var people = %(people)s
            if (method === "cli_mine") return ok(%(mine)s)
            if (method === "board_follows") return ok(%(follows)s)
            if (method === "board_feed") return ok(args[0] ? posts.filter(function(p) { return %(follows)s.indexOf(p.name) >= 0 }) : posts)
            if (method === "board_thread") return ok(posts.filter(function(p) { return p.post === args[0] || p.parent === args[0] }).reverse())
            if (method === "board_follow") return ok(args[1] ? [] : [args[0]])
            if (method === "board_post" || method === "board_like" || method === "board_hide")
                return ok({ id: args[0] + "-1792000000000", state: "running", started: Date.now() - 95000 })
            if (method === "job") return ok({ state: "running", log_tail: "" })
            if (method === "resolve") { var who = people[args[0]]; return ok(who ? { name: args[0], texts: who } : null) }
            if (method === "avatar") return ok(null)
            return ok(null)
        }
    }
    Timer { interval: 300; running: true; onTriggered: { %(scenario)s; shot.start() } }
    Timer { id: shot; interval: 600; onTriggered: root.grabToImage(function(result) { result.saveToFile("%(out)s"); Qt.quit() }) }
"""

SCENARIOS = {
    "all": "",
    "following": 'root.show("following")',
    "thread": 'root.show("thread", "jef/0")',
    "reply": 'root.replyTo = "jef/0"',
    "posting": 'composer.text = "Hello board"; root.post()',
    "nonames": "",
    "empty": "",
}


def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
    source = open(os.path.join(HERE, "..", "Main.qml")).read()
    marker = "Item {\n    id: root\n"
    assert marker in source, "Main.qml no longer starts the way the harness expects"
    os.makedirs(out_dir, exist_ok=True)
    for scenario in wanted:
        out = os.path.join(out_dir, scenario + ".png")
        bridge = BRIDGE % {
            "height": 1100,
            "posts": json.dumps([] if scenario == "empty" else POSTS),
            "people": json.dumps(PEOPLE),
            "mine": json.dumps([] if scenario == "nonames" else [{"name": "jef", "private": True}, {"name": "jefdiesel", "private": True}]),
            "follows": json.dumps(["ada"] if scenario == "following" else []),
            "scenario": SCENARIOS[scenario],
            "out": out,
        }
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        os.unlink(handle.name)
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(scenario, "ok" if os.path.exists(out) else "FAILED", *problems[:8], sep="\n  " if problems else " ")


main()
