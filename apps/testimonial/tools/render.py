#!/usr/bin/env python3
"""Renders src/qml/Main.qml to PNG files without Basecamp, with a stand-in for the Logos bridge.

Usage: render.py <qml binary> <output dir> [scenario ...]
Needs QML_IMPORT_PATH (with the Logos design system's qml folder on it) and
QT_PLUGIN_PATH set for a Qt 6 install, and QT_QPA_PLATFORM=offscreen to run
without a display. Nothing is asked of a wallet or a chain: every answer is made up here.
"""
import json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
QML = os.path.join(HERE, "..", "src", "qml")
TEXT = "One wallet for my zone accounts and my Logos Blockchain accounts: I sent on both, bridged between them and never opened a terminal."
KEY = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"

BRIDGE = r"""
    width: 900; height: %(height)s
    property var logos: QtObject {
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function callModuleAsync(module, method, args, callback, timeout) {
            var reply = callModule(module, method, args)
            Qt.callLater(function() { callback(reply) })
        }
        function callModule(module, method, args) {
            var mock = %(mock)s
            var asked = { request: "r-1792000000000", app: "testimonial_ui", kind: "inscribe", state: "pending", created: Date.now(), expires: Date.now() + 600000,
                          chain: "logos", submission: "lp-0022:names-wallet", text: args[0] }
            if (method === "get_config") return ok({ sequencer: "http://127.0.0.1:3040" })
            if (method === "granted_accounts") return ok({ connected: !!mock.connected, accounts: mock.connected ? [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", account_kind: "public" }] : [] })
            if (method === "propose_inscription") return mock.refuse ? JSON.stringify({ success: false, value: null, error: mock.refuse }) : ok(asked)
            if (method === "request_status") return ok(Object.assign(asked, mock.status || {}))
            return ok(null)
        }
    }
    Timer { interval: 300; running: true; onTriggered: { %(scenario)s; shot.start() } }
    Timer { id: shot; interval: %(wait)s; onTriggered: root.grabToImage(function(result) { result.saveToFile("%(out)s"); Qt.quit() }) }
"""

WRITE = 'root.target = "chain"; textArea.text = %s' % json.dumps(TEXT)
SCENARIOS = {
    "lez": "",
    "chain": WRITE,
    "chain-empty": 'root.target = "chain"',
    "chain-waiting": WRITE + "; root.inscribe()",
    "chain-done": WRITE + "; root.inscribe()",
    "chain-failed": WRITE + "; root.inscribe()",
    "chain-rejected": WRITE + "; root.inscribe()",
    "chain-refused": WRITE + "; root.inscribe()",
}
DONE = {"state": "done", "account": KEY, "tx": "5d1f0c9a7be24e3f8a61c0d2b9e47f135a08c6d2e91b4f70a3c5d8e2f1b6a904", "fee": "1086",
        "channel": "9a1c44e0b7d25f3a8c6e1b0d4f7a2953c8e6b1d0f4a7c2e5b8d1f4a7c0e3b6d9", "msg_id": "c0ffee"}
MOCKS = {
    "chain-done": {"status": DONE},
    "chain-failed": {"status": {"state": "failed", "account": KEY, "error": "the inscription may not have been sent"}},
    "chain-rejected": {"status": {"state": "rejected"}},
    "chain-refused": {"refuse": "the Names app is too old for inscriptions: update it"},
}
# The app polls a request every 2 s, so a decided one needs a later shot.
SLOW = ("chain-done", "chain-failed", "chain-rejected")

def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
    source = open(os.path.join(QML, "Main.qml")).read()
    marker = "Item {\n    id: root\n"
    assert marker in source, "Main.qml no longer starts the way the harness expects"
    os.makedirs(out_dir, exist_ok=True)
    for scenario in wanted:
        out = os.path.join(out_dir, scenario + ".png")
        bridge = BRIDGE % {"mock": json.dumps(MOCKS.get(scenario, {})), "scenario": SCENARIOS[scenario], "out": out,
                           "height": 1250, "wait": 2800 if scenario in SLOW else 600}
        # Beside Main.qml, so its relative import (logic.mjs) resolves.
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=QML) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        try:
            result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        except subprocess.TimeoutExpired:
            print(scenario, "FAILED", "  it didn't quit within 60 s", sep="\n")
            continue
        finally:
            os.unlink(handle.name)  # never leave it beside Main.qml: the package build copies qml/ whole
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(scenario, "ok" if os.path.exists(out) else "FAILED", *problems[:8], sep="\n  " if problems else " ")

main()
