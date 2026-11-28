#!/usr/bin/env python3
"""Renders Main.qml to PNG files without Basecamp, with a stand-in for the Logos bridge.

Usage: render.py <qml binary> <output dir> [scenario ...]
Needs QML_IMPORT_PATH (Qt 6 plus the Logos design system's qml folder) and
QT_PLUGIN_PATH set, and QT_QPA_PLATFORM=offscreen to run without a display.
"""
import base64, json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))

def pixels(w, h, seed):
    out = bytearray()
    for y in range(h):
        for x in range(w):
            out += bytes([(x * 255 // w + seed) % 256, (y * 255 // h) % 256, (seed * 3 + x + y) % 256, 255])
    return {"width": w, "height": h, "pixels": base64.b64encode(bytes(out)).decode()}

PUB = "Public/FaPur3GjxmeJZasHw1UXjVaZpwhCanjznp7KkQcnZsdg"
PRIV = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"
ACCOUNT = {"public": {"account": PUB, "balance": "47542624"}, "private": {"account": PRIV, "balance": "40600000"},
           "receive_code": "lez-nft-recv:" + "3xQ" * 40}
COLLECTIONS = [
    {"id": "ab" * 32, "price": "1000", "payee": PUB, "supply": 100, "minted": 12, "sold_out": False, "mine": PRIV,
     "content": {"name": "Basecamp Badges", "description": "One for everyone who set up a node", "image": "zCidA"}},
    {"id": "cd" * 32, "price": "0", "payee": None, "supply": 5, "minted": 5, "sold_out": True, "mine": None,
     "content": {"name": "First Five", "description": "", "image": "zCidB"}},
    {"id": "ef" * 32, "price": "777", "payee": "private", "supply": 3, "minted": 1, "sold_out": False, "mine": None,
     "content": {"name": "Quiet Money", "description": "Paid privately", "image": "zCidC"}},
]
TOKENS = [
    {"token_id": "11" * 32, "holder": PRIV, "collection": {"id": "ab" * 32, "serial": 7}, "sealed": False, "readable": True,
     "content": {"name": "Basecamp Badges", "image": "zCidA"}},
    {"token_id": "22" * 32, "holder": PRIV, "collection": None, "sealed": True, "readable": True,
     "content": {"name": "For your eyes", "description": "sealed to you", "image": "zCidD", "sealed": True}},
    {"token_id": "33" * 32, "holder": PRIV, "collection": None, "sealed": True, "readable": False, "content": None},
]
JOBS = [
    {"id": "1-1", "args": ["mint"], "writer": True, "state": "running", "log": "signing privately: proving it on this computer (about 5 minutes)"},
    {"id": "1-2", "args": ["portfolio"], "writer": False, "state": "done", "log": "scanning blocks 61940-62011"},
    {"id": "1-3", "args": ["buy"], "writer": True, "state": "failed", "error": "refused: collection is sold out", "log": "minting #6 of 5"},
]
SCENARIOS = {
    "setup": {"tab": "wallet", "ready": False},
    "wallet": {"tab": "wallet"},
    "launch": {"tab": "launch"},
    "mint": {"tab": "mint"},
    "private": {"tab": "private"},
    "portfolio": {"tab": "portfolio"},
    "activity": {"tab": "activity", "busy": True},
}

BRIDGE = r"""
    width: 1180; height: 1500
    property var logos: QtObject {
        signal moduleEventReceived(string moduleName, string eventName, var data)
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function onModuleEvent(m, e) { return true }
        function callModule(module, method, args) {
            if (module !== "lez_nft") return JSON.stringify({ success: true, value: "s1", error: null })
            if (method === "get_config") return ok({ cli: "/x/lez-nft", cli_found: true, wallet: "/x/wallet", wallet_ready: %(ready)s, program: "", rpc: "https://testnet.lez.logos.co", files: "/tmp" })
            if (method === "jobs") return ok(%(jobs)s)
            if (method === "start") return ok({ job: "9-9" })
            if (method === "job") return ok({ id: "9-9", state: "running" })
            if (method === "cid_path") return ok({ path: "/tmp/" + args[0], exists: true })
            if (method === "thumbnail") return ok(%(pixels)s)
            return ok(null)
        }
    }
    Timer {
        interval: 400; running: true
        onTriggered: {
            root.tab = %(tab)s
            if (%(ready)s) root.account = %(account)s
            root.collections = %(collections)s
            root.tokens = %(tokens)s
            root.jobList = %(jobs)s
            root.notice = %(tab)s === "mint" ? "Minting from Basecamp Badges privately: about 5 minutes." : ""
            shot.start()
        }
    }
    Timer { id: shot; interval: 600; onTriggered: root.grabToImage(function(result) { result.saveToFile(%(out)s); Qt.quit() }) }
"""

def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
    source = open(os.path.join(HERE, "..", "Main.qml")).read()
    marker = "Rectangle {\n    id: root\n"
    assert marker in source, "Main.qml no longer starts the way the harness expects"
    os.makedirs(out_dir, exist_ok=True)
    for name in wanted:
        s = SCENARIOS[name]
        out = os.path.join(out_dir, name + ".png")
        jobs = JOBS if s.get("busy") else JOBS[1:]
        bridge = BRIDGE % {"ready": "false" if s.get("ready") is False else "true", "jobs": json.dumps(jobs), "tab": json.dumps(s["tab"]),
                           "account": json.dumps(ACCOUNT), "collections": json.dumps(COLLECTIONS), "tokens": json.dumps(TOKENS),
                           "pixels": json.dumps(pixels(160, 160, 40)), "out": json.dumps(out)}
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=90)
        os.unlink(handle.name)
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(name, "ok" if os.path.exists(out) else "FAILED", *problems[:8], sep="\n  " if problems else " ")

main()
