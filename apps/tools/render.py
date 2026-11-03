#!/usr/bin/env python3
"""Renders the two mini-apps' Main.qml offscreen with a stand-in Logos bridge, names module
and backend, to check that the QML loads and lays out. Nothing is installed or contacted.

Usage: render.py <qml binary> <apps dir> <fixture.json> <out dir>
(fixture.json: node fixture.mjs <apps>/testimonial/src/qml/logic.mjs fixture.json)
Needs QML_IMPORT_PATH (Qt + Logos design system) and QT_QPA_PLATFORM=offscreen.
"""
import json, os, subprocess, sys, tempfile

qml, apps, fixture_path, out_dir = sys.argv[1:5]
fixture = json.load(open(fixture_path))
PUB = fixture["author"]
PUB2 = "Public/" + "9iHYqf77noB5oyGrxhL5GNVLSKcKQZeGBzp294fMi3Mv"
PRIV = "Private/" + "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"
GRANTED = {"connected": True, "accounts": [{"account": PUB, "account_kind": "public"}, {"account": PUB2, "account_kind": "public"}, {"account": PRIV, "account_kind": "private"}]}
NONE = {"connected": False, "accounts": []}

BRIDGE = r"""
    width: 900; height: %(height)s
    property var logos: QtObject {
        property var fixture: %(fixture)s
        property var mock: %(mock)s
        function ok(v) { return JSON.stringify({ success: true, value: v, error: null }) }
        function callModuleAsync(module, method, args, cb, timeout) {
            var r = answer(method, args)
            Qt.callLater(function() { cb(r) })
        }
        function answer(method, args) {
            if (method === "get_config") return ok({ sequencer: "http://127.0.0.1:3220" })
            if (method === "granted_accounts") return ok(mock.granted)
            if (method === "request_access") return ok({ request: "c0ffee", kind: "connect", state: "pending" })
            if (method === "request_status") return ok(mock.status)
            if (method === "propose_call") return ok({ request: "beef01", kind: "call", state: "pending", program: args[0] })
            if (method === "account_balances") return ok([{ account: "%(pub)s", account_kind: "public", balance: mock.balance || "1500000", updating: false, error: null }])
            if (method === "cancel_request") return ok({ request: args[0], state: "cancelled" })
            return ok(null)
        }
        function module(name) { return root.backendMock }
        function isViewModuleReady(name) { return true }
        function watch(pending, done, failed) { Qt.callLater(function() { done(pending) }) }
    }
    property var backendMock: QtObject {
        function accountView(sequencer, account, program) {
            var st = logos.fixture.states[account]
            if (st === undefined) return JSON.stringify({ ok: true, result: null })
            var states = {}
            states[program] = st
            return JSON.stringify({ ok: true, result: { nonce: 1, data: { actor_states: states } } })
        }
        function requestFunds(url, account) { return JSON.stringify(logos.mock.faucet) }
        function describe(url) { return JSON.stringify({ status: 200, body: { amount: "2000000", per_account: "once a day" }, retry_after: "" }) }
    }
    Timer { interval: 400; running: true; onTriggered: { %(scenario)s; shot.start() } }
    Timer { id: shot; interval: 700; onTriggered: root.grabToImage(function(result) { result.saveToFile("%(out)s"); Qt.quit() }) }
"""

T = "root.program = logos.fixture.program; root.refresh()"
SCENARIOS = {
    "testimonial": {
        "start": ({"granted": NONE}, "", 900),
        "connected": ({"granted": GRANTED}, T, 1700),
        "pending": ({"granted": GRANTED, "status": {"request": "beef01", "kind": "call", "state": "pending"}},
                    T + "; root.body = 'Great wallet'; root.lastPost = { account: root.author, tx: '', node: '' }; root.postDeadline = Date.now() + 540000; root.follow({ request: 'beef01', kind: 'call', state: 'pending' }, 'call')", 1900),
        "done": ({"granted": GRANTED, "status": {"request": "beef01", "kind": "call", "state": "done", "account": PUB, "tx": "9fbe58a895319b88093420471af36f67e34d61a730a8cfc0e64d27b15c01de16"}},
                 T + "; root.lastPost = { account: root.author, tx: '', node: 'X' }; root.follow({ request: 'beef01', kind: 'call', state: 'pending' }, 'call')", 1900),
        "failed": ({"granted": GRANTED, "status": {"request": "beef01", "kind": "call", "state": "failed", "account": PUB2, "error": "the program refused the call"}},
                   T + "; root.lastPost = { account: root.author, tx: '', node: '' }; root.follow({ request: 'beef01', kind: 'call', state: 'pending' }, 'call')", 1900),
        "settings": ({"granted": NONE}, "root.settingsOpen = true; root.program = 'nope'", 1100),
        "post-real": ({"granted": GRANTED, "status": {"request": "beef01", "kind": "call", "state": "pending"}},
                      T + "; textArea.text = 'Names Wallet is great'; root.post()", 1900),
    },
    "faucet": {
        "start": ({"granted": NONE}, "", 900),
        "connected": ({"granted": GRANTED}, "", 1100),
        "sent": ({"granted": GRANTED, "faucet": {"status": 200, "body": {"account": "x", "amount": "2000000", "tx": "ab" * 32}, "retry_after": ""}}, "root.ask()", 1200),
        "limited": ({"granted": GRANTED, "faucet": {"status": 429, "body": {"error": "this account was funded in the last day"}, "retry_after": ""}}, "root.ask()", 1200),
        "busy": ({"granted": GRANTED, "faucet": {"status": 409, "body": {"error": "the faucet is busy sending; try again in a minute"}, "retry_after": ""}}, "root.ask()", 1200),
        "settings": ({"granted": NONE}, "root.settingsOpen = true; root.faucetUrl = 'http://evil.example/'", 1100),
    },
}

os.makedirs(out_dir, exist_ok=True)
for app, scenarios in SCENARIOS.items():
    qml_dir = os.path.join(apps, app, "src", "qml")
    source = open(os.path.join(qml_dir, "Main.qml")).read()
    marker = "Item {\n    id: root\n"
    assert marker in source
    for name, (mock, scenario, height) in scenarios.items():
        out = os.path.join(out_dir, f"{app}-{name}.png")
        if os.path.exists(out):
            os.unlink(out)
        bridge = BRIDGE % {"height": height, "fixture": json.dumps(fixture), "mock": json.dumps(mock), "pub": PUB, "scenario": scenario, "out": out}
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=qml_dir) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        try:
            result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        finally:
            os.unlink(handle.name)
        problems = [l for l in result.stderr.splitlines() if "qt.qpa.fonts" not in l and "propagateSizeHints" not in l and l.strip()]
        print(app, name, "ok" if os.path.exists(out) else "FAILED", *problems[:8], sep="\n  " if problems else " ")
