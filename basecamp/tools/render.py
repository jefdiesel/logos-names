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
# jef.lez as it is on the chain: a pixel-art avatar, a colour and a theme, a
# deposit of 0 and a private holder. hopper.lez is a fuller profile.
JEF_AVATAR = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAMAAADXqc3KAAAAOVBMVEURERAIBw1NdEI6WDKW1ngSEBoAAAB9omkjNR54eHdeclNzc3b/AACBin5XcUkoGwnGxsbiWyZCUDorO/AgAAAAmElEQVQoz7WRwRLCIAxEWSBJrUrV//9YE2g1UDy6w+Swb5YwSwj/EIC5HWOcILNN+OGPBCnlppQw+NTUE6jPInYoLx5kkkOUcfEBkXUVGxq59uCmN+k4AwvocMB8t4PyHQ4UM5knoLRAoe+Dd1DFVSPYNn6YnrwDslcpeB2J4IG0u3QDfToB1Uq4lrjg1JXZXYX2rRUA7tvf2UsHKzvMA6UAAAAASUVORK5CYII="
# JEF_AVATAR decoded, as the module's `avatar` answers it: 24x24 RGBA8 in
# base64 (from avatar.rs's jef test, run with PRINT_JEF=1).
JEF_PIXELS = {"width": 24, "height": 24, "pixels": "EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf8IBw3/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/CAcN/010Qv9NdEL/TXRC/wgHDf8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf9NdEL/TXRC/010Qv9NdEL/TXRC/010Qv9NdEL/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/CAcN/010Qv9NdEL/TXRC/010Qv9NdEL/TXRC/010Qv9NdEL/TXRC/wgHDf8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/zpYMv+W1nj/ltZ4/5bWeP+W1nj/ltZ4/5bWeP+W1nj/Olgy/zpYMv8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf86WDL/Olgy/5bWeP8SEBr/EhAa/xIQGv8SEBr/EhAa/xIQGv8SEBr/ltZ4/zpYMv86WDL/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf86WDL/ltZ4/xIQGv8AAAD/faJp/32iaf8AAAD/faJp/32iaf8AAAD/EhAa/5bWeP8jNR7/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf+W1nj/EhAa/32iaf99omn/faJp/32iaf99omn/faJp/32iaf99omn/faJp/xIQGv+W1nj/CAcN/3h4d/8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/CAcN/zpYMv+W1nj/EhAa/32iaf99omn/XnJT/15yU/99omn/faJp/32iaf9eclP/XnJT/xIQGv+W1nj/IzUe/3Nzdv8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/CAcN/zpYMv+W1nj/EhAa/32iaf99omn//wAA/wAAAP99omn/faJp/32iaf//AAD/AAAA/xIQGv+W1nj/IzUe/3Nzdv8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/CAcN/zpYMv+W1nj/EhAa/32iaf99omn/XnJT/32iaf99omn/faJp/32iaf9eclP/faJp/xIQGv+W1nj/IzUe/3Nzdv8RERD/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/5bWeP8SEBr/EhAa/32iaf99omn/faJp/32iaf99omn/faJp/32iaf99omn/faJp/xIQGv8SEBr/ltZ4/4GKfv8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/5bWeP8SEBr/EhAa/1dxSf99omn/faJp/32iaf99omn/AAAA/wAAAP99omn/faJp/xIQGv8SEBr/ltZ4/4GKfv8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/5bWeP8SEBr/EhAa/1dxSf9XcUn/faJp/32iaf99omn/faJp/32iaf99omn/V3FJ/xIQGv8SEBr/ltZ4/yM1Hv8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/5bWeP8SEBr/EhAa/1dxSf9XcUn/V3FJ/1dxSf9XcUn/V3FJ/1dxSf8AAAD/AAAA/wAAAP8AAAD/AAAA/wAAAP8IBw3/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/Olgy/5bWeP8SEBr/EhAa/1dxSf9XcUn/V3FJ/1dxSf8oGwn/KBsJ/wAAAP/Gxsb/xsbG/8bGxv/Gxsb/xsbG/+JbJv8AAAD/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/EhAa/zpYMv+W1nj/EhAa/32iaf9XcUn/V3FJ/1dxSf9CUDr/V3FJ/1dxSf8AAAD/AAAA/wAAAP8AAAD/AAAA/wAAAP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8IBw3/EhAa/zpYMv+W1nj/EhAa/32iaf99omn/V3FJ/1dxSf9XcUn/V3FJ/1dxSf9XcUn/EhAa/5bWeP8jNR7/EhAa/wgHDf8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/CAcN/xIQGv+W1nj/EhAa/32iaf99omn/faJp/wAAAP8SEBr/EhAa/xIQGv8SEBr/ltZ4/yM1Hv8IBw3/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf+W1nj/EhAa/32iaf99omn/faJp/xIQGv8SEBr/EhAa/5bWeP+W1nj/IzUe/wgHDf8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/EREQ/wgHDf8IBw3/EhAa/32iaf99omn/faJp/xIQGv8IBw3/CAcN/wgHDf8IBw3/CAcN/xEREP8RERD/EREQ/xEREP8RERD/EREQ/xEREP8RERD/"}
RECORDS = {
    "jef": {"deposit": "0", "account": None, "npk": "a1e38c92", "vpk": "4865",
            "owner_tag": "1f7a805e9c3d41b2a07e55c1d2f3e4a5b6c7d8e9f0a1b2c3d4e5f60718ab992",
            "texts": {"avatar": JEF_AVATAR, "color": "#96d678", "theme": "dark"}},
    "hopper": {"deposit": "100000000", "account": CAROL, "npk": "b2f49d03", "vpk": "5976",
               "texts": {"display": "Grace Hopper", "bio": "Compilers, COBOL and nanoseconds.\nAsk me about the moth.",
                         "color": "#4a6cf7", "font": "serif",
                         "chat": "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                         "agent": "https://agents.example/hopper", "inbox": "/logos/1/hopper/proto"}},
}
# jef.lez's page is jefdiesel.html, the real page of jefdiesel.lez, and
# JEF_PAGE_HTML is what the module's page_view makes of it (page_view.rs's
# jefdiesel test, run with PRINT_JEF=1).
PAGE = open(os.path.join(HERE, "..", "..", "basecamp-module", "rust-lib", "testdata", "jefdiesel.html")).read()
JEF_PAGE_HTML = "<div>\n      <h1>Jef</h1>\n      <p>jefdiesel.lez</p>\n    </div>\n  \n  <p>Building LNS, private names for the Logos Execution Zone. No relays, no servers, nothing between you and the chain.</p>\n  <ul>\n    <li><span>pay me</span><code>names pay jefdiesel.lez &lt;amount&gt;</code></li>\n    <li><span>also</span><code>jef.lez</code></li>\n    <li><span>code</span><a href=\"https://github.com/jefdiesel\">github.com/jefdiesel</a></li>\n  </ul>\n  Stored on the Logos Execution Zone, opened on your own machine."
# hopper.lez publishes plain text; linus.lez a page long enough to fold.
PAGES = {
    "jef": {"content_type": "text/html; charset=utf-8", "bytes": len(PAGE.encode()), "kind": "html", "title": "jefdiesel.lez",
            "html": JEF_PAGE_HTML, "text": PAGE, "truncated": False},
    "hopper": {"content_type": "text/plain; charset=utf-8", "bytes": 120, "kind": "text", "title": None, "html": None, "truncated": False,
               "text": "Office hours: Tuesdays.\n\nA ship in port is safe, but that is not what ships are built for.\n<b>not bold</b>"},
    "linus": {"content_type": "text/html", "bytes": 5400, "kind": "html", "title": "Changelog", "truncated": False, "text": "<title>Changelog</title>...",
              "html": "<h2>Changelog</h2><table><thead><tr><th>Version</th><th>Date</th></tr></thead><tbody><tr><td>v0.40</td><td>2026-10-16</td></tr><tr><td>v0.39</td><td>2026-10-01</td></tr></tbody></table><pre>" + "names pay linus.lez 100 --memo " + "x" * 160 + "</pre><blockquote>Quoted <i>words</i>.</blockquote><ol><li>one</li><li>two</li></ol><hr>" + "".join("<p><b>v0.%d</b> fixed <code>thing %d</code>, see <a href=\"https://example.org/%d\">notes</a>.</p>" % (i, i, i) for i in range(40, 0, -1))},
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
            var names = %(names)s
            if (module === "lez_core")
                return method === "account_id_from_base58" ? "15145aee" : ""
            if (method === "wallet") {
                if (%(wallet)s)
                    return ok({ accounts: [
                        { account: "%(alice)s", balance: "19999899314697", funded: true, name: "jef" },
                        { account: "%(bob)s", balance: "9999975450630", funded: true, name: null },
                        { account: "%(carol)s", balance: "0", funded: false, name: null }],
                        private_accounts: [{ account: "%(carol)s", balance: "2999000", funded: true }] })
                return JSON.stringify({ success: false, value: null, error: "the LEZ wallet is not running; install LEZ Wallet and open it" })
            }
            if (method === "get_config") return JSON.stringify({ registry: %(registry)s, sequencer: "http://127.0.0.1:3040", wallet_config: "/Users/you/wallet/wallet_config.json", wallet_storage: "/Users/you/wallet/storage.json", cli: "/Users/you/.local/bin/names", cli_wallet: "/Users/you/.lee/wallet" })
            if (method === "normalize") return String(args[0]).toLowerCase().replace(/\.lez$/, "")
            if (method === "mine") return ok({ names: names.filter(function(entry) { return entry.owner === "%(alice)s" }).map(function(entry) {
                return { name: entry.name, display: entry.name + ".lez", owner: entry.owner, expires: null, never: true, lapsed: false } }), pending: [] })
            if (method === "keep") return ok(String(args[0]))
            // hopper.lez and ada.lez are held privately by the names CLI's wallet.
            if (method === "cli_mine") return ok(["hopper", "ada"].map(function(name) {
                return { name: name, display: name + ".lez", signer: "cli", state: "held by a deposit (no expiry)", lapsed: false } }))
            if (method === "cli_set_text" || method === "cli_profile" || method === "cli_set_page")
                return ok({ id: args[0] + "-1792000000000", name: args[0], kind: method.substr(4), state: "running", started: Date.now() - 125000 })
            if (method === "job") return ok(%(job)s)
            if (method === "set_cli" || method === "set_cli_wallet") return true
            if (method === "settings") return ok({ mode: "deposit", deposits: { normal: "100000000", three: "2000000000" }, admin: "%(bob)s", treasury: "%(bob)s", prices: null })
            if (method === "deposit") {
                var typed = String(args[0]).toLowerCase().replace(/\.lez$/, "")
                var tier = typed.length <= 2 ? "locked" : typed.length === 3 ? "three" : "normal"
                return ok({ name: typed, display: typed + ".lez", tier: tier, deposit: tier === "locked" ? null : tier === "three" ? "2000000000" : "100000000", mode: "deposit" })
            }
            if (method === "avatar") return ok(args[0] === "jef" ? %(avatar)s : null)
            if (method === "page_preview") { var drafts = %(pages)s; return ok(drafts["jef"]) }
            if (method === "page_view" || method === "page") { var pages = %(pages)s; return ok(pages[args[0]] || null) }
            if (method === "resolve") {
                var name = String(args[0]).toLowerCase().replace(/\.lez$/, "")
                for (var i = 0; i < names.length; i++) {
                    if (names[i].name !== name) continue
                    var records = %(records)s
                    var rich = records[name] || {}
                    return ok({ name: name, display: name + ".lez", owner_tag: rich.owner_tag || ("c0ffee" + names[i].owner.substr(0, 10)),
                        expires: null, never: true, lapsed: false, deposit: rich.deposit !== undefined ? rich.deposit : "100000000",
                        account: rich.account !== undefined ? rich.account : names[i].owner, npk: rich.npk || null, vpk: rich.vpk || null,
                        texts: rich.texts || {} })
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
    "register": 'root.show("satoshi")',
    "three": 'root.show("qkz")',
    "locked": 'root.show("xz")',
    "nowallet": 'root.show("jef")',
    "nowallet-register": 'root.show("satoshi")',
    "profile": 'root.show("hopper")',
    "source": 'root.show("jef"); root.pageSourceOpen = true',
    "page-link": 'root.show("jef"); root.pageLink = "https://github.com/jefdiesel"',
    "page-text": 'root.show("hopper")',
    "page-long": 'root.show("linus")',
    "page-all": 'root.show("linus"); root.pageAll = true',
    "settings": 'root.manage("jef")',
    "release": 'root.manage("grace"); root.confirmingRelease = true',
    "lapsed": 'root.manage("grace")',
    "waiting": 'root.manage("grace"); root.releaseName()',
    "register-settings": 'root.show("jef"); root.settingsOpen = true',
    # hopper.lez is held by the names CLI: the same settings, minus what only the LEZ wallet does.
    "cli-settings": 'root.manage("hopper")',
    "cli-saving": 'root.manage("hopper"); root.saveText("inbox", "/logos/1/hopper/new")',
    "cli-failed": 'root.manage("hopper"); root.saveText("inbox", "/logos/1/hopper/new"); Qt.callLater(function() { Qt.callLater(function() { root.checkCliJob() }) })',
    "cli-nowallet": 'root.manage("hopper")',
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
            "alice": ALICE, "bob": BOB, "carol": CAROL, "pages": json.dumps(PAGES), "avatar": json.dumps(JEF_PIXELS),
            "wallet": "false" if scenario.startswith("nowallet") or scenario == "cli-nowallet" else "true",
            "job": json.dumps({"id": "hopper-1792000000000", "name": "hopper", "kind": "set_text", "started": 1792000000000, "finished": 1792000190000, "exit": 1, "state": "failed",
                               "log_tail": "proving the change on this machine...\nError: the transaction was refused: out of gas"} if scenario == "cli-failed"
                              else {"id": "hopper-1792000000000", "name": "hopper", "kind": "set_text", "started": 1792000000000, "finished": None, "exit": None, "state": "running", "log_tail": "proving the change on this machine..."}), "records": json.dumps(RECORDS),
            "registry": "null" if scenario == "unset" else json.dumps("DbYLkKwd2b13bQhceVj351mpPw7QmRzEjRT2EfU56CKH"),
            "scenario": SCENARIOS[scenario], "out": out, "height": 2700 if scenario in ("settings", "release", "page-all", "cli-settings", "cli-saving", "cli-failed", "cli-nowallet") else 1300 if scenario in ("name", "profile", "source", "page-link", "page-text", "page-long", "nowallet") else 760,
        }
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
        result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        os.unlink(handle.name)
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(scenario, "ok" if os.path.exists(out) else "FAILED", *problems[:6], sep="\n  " if problems else " ")

main()
