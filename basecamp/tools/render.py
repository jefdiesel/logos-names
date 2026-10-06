#!/usr/bin/env python3
"""Renders Main.qml to PNG files without Basecamp, with a stand-in for the Logos bridge.

Usage: render.py <qml binary> <output dir> [scenario ...]
Needs QML_IMPORT_PATH and QT_PLUGIN_PATH set for a Qt 6 install, and
QT_QPA_PLATFORM=offscreen to run without a display.
"""
import json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ALICE = "2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2"
BOB = "CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r"
CAROL = "9iHYqf77noB5oyGrxhL5GNVLSKcKQZeGBzp294fMi3Mv"
NAMES = [("turing", ALICE), ("hopper", CAROL), ("linus", BOB), ("grace", ALICE), ("ada", CAROL), ("jef", ALICE)]
PAGE = "<!doctype html>\n<meta charset=\"utf-8\">\n<title>jef</title>\n<h1>jef</h1>\n<p>This page is stored on the Logos Execution Zone and resolved by name.</p>\n"

BRIDGE = r"""
    width: 1180; height: %(height)s
    property var logos: QtObject {
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function callModule(module, method, args) {
            var names = %(names)s
            if (module === "lez_core")
                return method === "account_id_from_base58" ? "15145aee" : ""
            if (method === "wallet") {
                if (%(wallet)s)
                    return ok({ accounts: [
                        { account: "%(alice)s", balance: "19999899314697", funded: true, name: "jef" },
                        { account: "%(bob)s", balance: "9999975450630", funded: true, name: null },
                        { account: "%(carol)s", balance: "0", funded: false, name: null }] })
                return JSON.stringify({ success: false, value: null, error: "the LEZ wallet is not running; install LEZ Wallet and open it" })
            }
            if (method === "get_config") return JSON.stringify({ registry: %(registry)s, sequencer: "http://127.0.0.1:3040" })
            if (method === "normalize") return String(args[0]).toLowerCase().replace(/\.lez$/, "")
            if (method === "list") return ok({ total: 214, names: names, more: true })
            if (method === "page") return ok(args[0] === "jef" ? { content_type: "text/html; charset=utf-8", bytes: 148, text: %(page)s } : null)
            if (method === "resolve") {
                var name = String(args[0]).toLowerCase().replace(/\.lez$/, "")
                for (var i = 0; i < names.length; i++) {
                    if (names[i].name !== name) continue
                    var rich = name === "jef"
                    return ok({ name: name, display: name + ".lez", owner: names[i].owner,
                        account: names[i].owner, npk: rich ? "a1e38c92" : null, vpk: rich ? "4865" : null,
                        texts: rich ? { chat: "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", agent: "https://agents.example/jef" } : {},
                        previous: i + 1 < names.length ? names[i + 1].name : null, next: i > 0 ? names[i - 1].name : null })
                }
                return ok(null)
            }
            return ok(null)
        }
    }
    Timer { interval: 300; running: true; onTriggered: { %(scenario)s; shot.start() } }
    Timer { id: shot; interval: 500; onTriggered: root.grabToImage(function(result) { result.saveToFile("%(out)s"); Qt.quit() }) }
"""

SCENARIOS = {
    "start": "",
    "name": 'root.show("jef")',
    "plain": 'root.show("linus")',
    "pay": 'root.show("jef"); root.openPay()',
    "register": 'root.show("satoshi")',
    "nowallet": 'root.show("satoshi")',
    "edit": 'root.show("jef"); root.openEdit()',
    "transfer": 'root.show("jef"); root.tool = "transfer"',
    "delete": 'root.show("jef"); root.tool = "delete"',
    "waiting": 'root.show("jef"); root.tool = "delete"; root.deleteName()',
    "settings": 'root.show("jef"); root.settingsOpen = true',
    "unset": "",
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
            "names": json.dumps([{"name": n, "owner": o} for n, o in NAMES]),
            "alice": ALICE, "bob": BOB, "carol": CAROL, "page": json.dumps(PAGE),
            "wallet": "false" if scenario == "nowallet" else "true",
            "registry": "null" if scenario == "unset" else json.dumps("DbYLkKwd2b13bQhceVj351mpPw7QmRzEjRT2EfU56CKH"),
            "scenario": SCENARIOS[scenario], "out": out, "height": 1240 if scenario == "edit" else 760,
        }
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        os.unlink(handle.name)
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(scenario, "ok" if os.path.exists(out) else "FAILED", *problems[:6], sep="\n  " if problems else " ")

main()
