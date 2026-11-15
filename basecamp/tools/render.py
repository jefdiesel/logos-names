#!/usr/bin/env python3
"""Renders qml/Main.qml to PNG files without Basecamp, with a stand-in for the Logos bridge.

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
    property string mockNet: ""
    property var mockCalls: ({})
    property var logos: QtObject {
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function callModuleAsync(module, method, args, callback, timeout) {
            var reply = callModule(module, method, args)
            Qt.callLater(function() { callback(reply) })
        }
        function callModule(module, method, args) {
            // Basecamp's package manager: what is installed (the mock's `packages`, else this app and
            // the LEZ Bridge pair Names ships, already in).
            if (module === "package_manager" && method === "getInstalledPackages") {
                var me = { name: "names_ui", version: "0.17.0", installDir: "/Users/you/Library/Application Support/Logos/LogosBasecamp/plugins/names_ui" }
                var pk = (%(mock)s).packages
                return JSON.stringify(pk ? [me].concat(pk) : [me, { name: "lez_bridge", version: "0.1.0", hashes: { root: "ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7" } }, { name: "lez_bridge_ui", version: "0.1.0", hashes: { root: "ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9" } }])
            }
            if (module === "names" && method === "file_sha256")
                return ok(String(args[0]).indexOf("lez_bridge_ui") >= 0 ? "0d7e317c1fbef3e9c9d4155c10746256af8e9ed2b5a8156413823fbc568ea270" : "55f474d88f43effd0c6f9cc122fe1af37fac52b441a11de4922cb6bd17503561")
            if (module === "lez_bridge") {
                var lb = (%(mock)s).bridge
                if (!lb) return JSON.stringify({ error: "Method not found" })
                var answer = lb[method]
                // A list of answers given one per call, in order (the last one repeats).
                if (answer && answer.__seq) {
                    var n = root.mockCalls[method] || 0
                    root.mockCalls[method] = n + 1
                    answer = answer.__seq[Math.min(n, answer.__seq.length - 1)]
                }
                if (answer === undefined && method !== "config" && method !== "relayed_claims" && method !== "relayed_claim_forget" && method !== "receives") return JSON.stringify({ error: "Method not found" })
                if (answer === undefined) answer = method === "config" ? { build: { commit: "00e6f3a" }, deployment: { name: "bridge-test-network-v3", bridge_program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" }, features: ["relayed_claim", "relayed_claim_check_with", "relayed_claim_build", "relayed_claim_submit", "account_hint", "receive", "deposit_code", "public_receive", "receive_status", "receives", "relayed_claim_check_deposit"], notices: { not_private: { title: "Not private", text: "A public claim publishes the claim secret and links the Sepolia deposit to this account, for good. A private account keeps them apart (its claim needs a proof)." } } } : method === "relayed_claims" ? [] : method === "receives" ? { receives: [] } : null
                return JSON.stringify(JSON.stringify({ success: true, value: answer, error: null }))
            }
            if (module === "chat_module") {
                if (method === "status") return JSON.stringify({ convo_count: 2, delivery_state: "online", detail: "", delivery_adopted: true })
                if (method === "get_address") return JSON.stringify("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
                if (method === "list_conversations") return JSON.stringify([{ convo_id: "g1", message_count: 4, last_activity_ms: Date.now() - 60000, kind: "group_v1", name: "LNS builders", preview: "ada: shipping the board today" }, { convo_id: "c1", message_count: 3, last_activity_ms: Date.now() - 120000, kind: "private_v1", history_only: false }, { convo_id: "c2", message_count: 1, last_activity_ms: Date.now() - 7200000, kind: "private_v1", history_only: false }])
                if (method === "list_group_members") return JSON.stringify([{ address: "aaa1", pending: false }, { address: "bbb2", pending: true }])
                if (method === "get_messages" && args[0] === "g1") return JSON.stringify([{ from_self: false, content: "who's building on the board?", timestamp_ms: 1, sender: "aaa1" }, { from_self: true, content: "me, private posts work now", timestamp_ms: 2 }, { from_self: false, content: "shipping the board today", timestamp_ms: 3, sender: "aaa1" }])
                if (method === "get_messages") return JSON.stringify([{ from_self: false, content: "hey, saw your post on the board", timestamp_ms: 1 }, { from_self: true, content: "thanks! it's all made privately as a name", timestamp_ms: 2 }, { from_self: false, content: "nice. how long did the post take?", timestamp_ms: 3 }])
                return JSON.stringify(null)
            }
            var names = %(names)s
            // A module still busy with another call: the host answers with something that isn't the module's.
            if (((%(mock)s).busy || []).indexOf(method) >= 0) return "Timed out waiting for the module"
            if (module === "lez_core")
                return method === "account_id_from_base58" ? "15145aee" : ""
            // Settings → Backup (the mock's `backup`: status, make, open, restore).
            if (method === "backup_status") return ok(((%(mock)s).backup || {}).status || { last: null, in_use: "lez-0.4", folder: "/Users/you/Names Wallet Backups", min_password: 10,
                networks: [{ id: "lez-0.3", has_wallet: true, accounts: [{ account: "Public/9vXe", kind: "public", imported: false }], uncovered: 0 },
                           { id: "lez-0.4", has_wallet: true, accounts: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", kind: "public", imported: true }], uncovered: 1 }] })
            if (method === "backup_make" || method === "backup_open" || method === "backup_restore") {
                var b = ((%(mock)s).backup || {})[method]
                return b && b.error ? JSON.stringify({ success: false, value: null, error: b.error }) : ok(b || null)
            }
            if (method === "wallet") {
                if (%(wallet)s)
                    return ok({ accounts: [
                        { account: "%(alice)s", balance: "19999899314697", funded: true, name: "jef" },
                        { account: "%(bob)s", balance: "9999975450630", funded: true, name: null },
                        { account: "%(carol)s", balance: "0", funded: false, name: null }],
                        private_accounts: [{ account: "%(carol)s", balance: "2999000", funded: true }] })
                return JSON.stringify({ success: false, value: null, error: "the LEZ wallet is not running; install LEZ Wallet and open it" })
            }
            // The names network: set_names_network moves it, get_config says it.
            if (method === "set_names_network") { root.mockNet = String(args[0]); method = "get_config"; var wrapNet = true }
            if (method === "get_config") return (function(c) { return wrapNet ? ok(c) : JSON.stringify(c) })({ features: ["reveal_name", "request_access_with", "file_sha256", "grant_add_account", "app_note"], bridge_programs: ["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr"], names_network: { id: root.mockNet || "lez-0.3" }, zone: (%(mock)s).zone || { id: "home", name: "Home", channel: "%(home_channel)s", home: true }, names_sequencer: "http://127.0.0.1:3040", registry: %(registry)s, sequencer: (%(mock)s).zone ? "http://127.0.0.1:3041" : "http://127.0.0.1:3040", wallet_config: "/Users/you/wallet/wallet_config.json", wallet_storage: "/Users/you/wallet/storage.json", cli: "/Users/you/.local/bin/names", cli_wallet: "/Users/you/.lee/wallet", bridge_key: "%(bridge_key)s", bridge_node: null, module_version: root.neededModule })
            if (method === "normalize") return String(args[0]).toLowerCase().replace(/\.lez$/, "")
            if (method === "mine") return ok({ names: names.filter(function(entry) { return !(%(mock)s).noNames && entry.owner === "%(alice)s" }).map(function(entry) {
                return { name: entry.name, display: entry.name + ".lez", owner: entry.owner, expires: null, never: true, lapsed: false } }), pending: ((%(mock)s).pending || []).map(function(p) {
                    return Object.assign({}, p, { ready_at: Date.now() + p.ready_in * 60000, reveal_by: Date.now() + p.by_in * 60000 }) }) })
            if (method === "keep") return ok(String(args[0]))
            // hopper.lez and ada.lez are held privately by the names CLI's wallet.
            if (method === "cli_mine" && (%(mock)s).noNames) return ok([])
            if (method === "cli_mine") return ok(["hopper", "ada"].map(function(name) {
                return { name: name, display: name + ".lez", signer: "cli", state: "held by a deposit (no expiry)", lapsed: false, registering: false, failed: false, private: true } }).concat(
                [{ name: "knuth", display: "knuth.lez", signer: "cli", state: "registering, the last step starts in about 9 minutes", lapsed: false, registering: true, failed: false }]))
            if (method === "cli_set_text" || method === "cli_profile" || method === "cli_set_page" || method === "cli_register")
                return ok({ id: args[0] + "-1792000000000", name: args[0], kind: method.substr(4), state: "running", started: Date.now() - 125000 })
            var mock = %(mock)s
            // Zones (LP-0022): stand-ins, nothing is asked of a sequencer.
            if (/^zone_/.test(method)) {
                if (!mock.zones) return ok(null)
                var zonesView = { zones: mock.zones, current: mock.zone ? mock.zone.id : "home", max: 16 }
                if (method === "zone_sequencer" && mock.otherChain)
                    return ok(Object.assign({ changed: false, other_chain: true, channel: "0505050505050505050505050505050505050505050505050505050505050505", why: "that sequencer serves another chain" }, zonesView))
                return ok(zonesView)
            }
            if (method === "cli_balance") {
                if (mock.balance === null) return JSON.stringify({ success: false, value: null, error: "there is no wallet in /Users/you/.lee/wallet yet" })
                return ok({ account: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", balance: mock.private !== undefined ? mock.private : root.missing === "pauper" ? "5000" : "250000000",
                    public: { account: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", balance: mock.public !== undefined ? mock.public : "1250000" },
                    waiting: mock.waiting || null })
            }
            // The bridge test network (LEZ 0.4), as bridge_net_balances answers from its last read.
            if (method === "bridge_net_balances") return ok(mock.bridgeNet || %(bridge_net)s)
            if (method === "bridge_net_watch") return ok(args[1] ? [String(args[0])] : [])
            if (method === "activity") return ok(mock.activity || [
                { id: "a1", at: Date.now() - 2 * 3600000, kind: "bridge", chain: "logos", from: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "1000000", state: "in_block" },
                { id: "a2", at: Date.now() - 2 * 3600000 - 120000, kind: "split", chain: "logos", from: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", amount: "1000000", fee: "2018", state: "in_block" },
                { id: "a3", at: Date.now() - 26 * 3600000, kind: "pay", chain: "lez", name: "ada", amount: "250000", fee: "4088", state: "done", job: "ada-1" },
                { id: "a4", at: Date.now() - 30 * 3600000, kind: "arrived", chain: "lez", to: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", amount: "5000000", state: "arrived" },
                { id: "a5", at: Date.now() - 50 * 3600000, kind: "pay_account", chain: "lez", to: "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg", amount: "1000", state: "failed" }])
            if (method === "cli_transfer" || method === "cli_receive_privately") return ok({ id: args[0] + "-1792000000010", name: args[0], kind: method.substr(4), state: "running", started: Date.now() - 30000 })
            if (method === "cli_receive") return ok({ id: "-wallet-1792000000009", name: "-wallet", kind: "receive", state: "running", started: Date.now() - 60000 })
            // Getting started. Stand-ins only: nothing is installed, no wallet is made.
            if (method === "cli_status") return ok(Object.assign({ cli: "/Users/you/.config/logos-names/bin/names", prover: true, installing: null, wallet_dir: "/Users/you/.lee/wallet",
                wallet: true, public: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", private: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", busy: null }, mock.status || {}))
            if (method === "cli_install") return ok({ installed: false, path: "/Users/you/.config/logos-names/bin/names", version: null, prover: false,
                job: { id: "-install-1792000000000", name: "-install", kind: "install", state: "running", started: Date.now() - 20000 } })
            if (method === "cli_create_wallet") return ok({ public: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", private: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z",
                phrase: "abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act action actor actress actual" })
            if (method === "cli_faucet") return mock.faucetError ? JSON.stringify({ success: false, value: null, error: mock.faucetError })
                : ok({ account: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "2000000", tx: "9fbe58a895319b88093420471af36f67e34d61a730a8cfc0e64d27b15c01de16" })
            if (method === "cli_shield") return ok({ id: "-wallet-1792000000001", name: "-wallet", kind: "shield", state: "running", started: Date.now() - 130000 })
            if (method === "cli_pay_account") return ok({ id: "-wallet-1792000000002", name: "-wallet", kind: "pay_account", state: "running", started: Date.now() - 10000 })
            if (method === "job" && /^-install/.test(args[0])) return ok(mock.installJob || { id: args[0], name: "-install", kind: "install", state: "running", started: Date.now() - 20000,
                log_tail: "Downloading names for macos-arm64...\nInstalled /Users/you/.config/logos-names/bin/names (names 0.1.0)\nDownloading RISC Zero's prover (r0vm 3.0.5, about 70 MB) for private changes..." })
            if (method === "job" && /^-wallet/.test(args[0])) return ok(mock.walletJob || { id: args[0], name: "-wallet", kind: "shield", state: "running", started: Date.now() - 130000,
                log_tail: "moving 5000000 to the private account; generating the proof (a few minutes)" })
            // A stand-in only: the harness never runs the names CLI, let alone with --yes.
            if (method === "cli_bridge") {
                if (args[0] === "999999999") return JSON.stringify({ success: false, value: null, error: "That takes more than 30 notes, the most one deposit can use. Bridge in smaller steps." })
                if (args[3]) return ok({ sent: true, lines: [], hash: "5d1f0c9a7be24e3f8a61c0d2b9e47f135a08c6d2e91b4f70a3c5d8e2f1b6a904", total: "5000400", notes: "2", fee: "100000", to: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" })
                return ok({ sent: false, lines: [], hash: null, total: "5000400", notes: "2", fee: "100000", to: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" })
            }
            // Accounts and fees: stand-ins for the names CLI's wallet.
            // Tokens: a program on a chain of one's own, one token held and one added by address.
            if (method === "cli_token_holdings") return ok({ token_program: "Tok3nProgram1111111111111111111111111111111", updating: false, error: null,
                holdings: [{ account: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", account_kind: "private", definition: "GoLDdef1nition11111111111111111111111111111", name: "Gold", balance: "4200" }] })
            if (method === "cli_token_pending") return ok({ token_program: "Tok3nProgram1111111111111111111111111111111", pending: mock.tokenPending || [], updating: false, error: null })
            if (method === "token_source_holdings") return ok(mock.bridged || [])
            if (method === "watched_tokens") return ok([{ definition: "S1LVERdef1nition1111111111111111111111111111", name: "Silver" }])
            if (method === "watch_token") return ok([{ definition: "S1LVERdef1nition1111111111111111111111111111", name: "Silver" }, { definition: args[0], name: "Copper" }])
            if (method === "chain_status") return mock.nodeDown ? JSON.stringify({ success: false, value: null, error: "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app." })
                : ok({ height: 66467, slot: 2286493, lib_slot: 2247925, mode: "Online", epoch: 7 })
            if (method === "chain_accounts") return mock.nodeDown ? JSON.stringify({ success: false, value: null, error: "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app." }) : ok([
                { key: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", balance: "1191220464", label: "Main", ready: true },
                { key: "73058ad5edb0e72b66c3e05ae90471382c3bb4d61314db995ef40e674dbf591e", balance: "25000000" },
                { key: "5f9f8fd1696c6e6d85bb52fc4b61810a3f9c75422612b72a566ff3ef6a68ad12", balance: "0" }].concat(mock.notReady || String(root.chainAccountNote).indexOf("Made") === 0 ? [
                { key: "e613268c7dd432c972e872d03fff5f79d6a3cbcc8103f34afc2b33fef998bf1d", balance: null, title: "names-1", label: "Savings", ready: false }] : []))
            if (method === "chain_account_new") {
                return ok({ key: "e613268c7dd432c972e872d03fff5f79d6a3cbcc8103f34afc2b33fef998bf1d", title: "names-1", label: args[0], ready: false,
                    note: "Your node uses this account from its next start: stop and start it in Basecamp's Blockchain app. Until then it can't send, and don't have anything sent to it: what reaches it before your node uses it isn't seen." }) }
            // An app's inscription request (LP-0022): its sheet, the accounts offered, the node's fee.
            if (method === "pending_inscriptions") return ok(!mock.inscription ? [] : [{ id: "i1", kind: "inscribe", state: "pending", app: "testimonial_ui",
                created: Date.now(), expires: Date.now() + 600000, chain: "logos", says: "inscribes on the Logos blockchain", submission: "lp-0022:names-wallet",
                text: mock.inscription.text || "One wallet for my zone accounts and my Logos Blockchain accounts: I sent on both, bridged between them and never opened a terminal.",
                publisher: mock.inscription.publisher || { status: "verified", publisher: "jef", module: "testimonial_ui", version: "0.2.0" },
                choices: mock.inscription.node ? [] : [
                    { key: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", balance: "1191220464", label: "Main" },
                    { key: "73058ad5edb0e72b66c3e05ae90471382c3bb4d61314db995ef40e674dbf591e", balance: "25000000" },
                    { key: "e613268c7dd432c972e872d03fff5f79d6a3cbcc8103f34afc2b33fef998bf1d", balance: "0", title: "names-1", label: "Savings" }] }]
                .map(function(r) { if (mock.inscription.node) r.node = mock.inscription.node; return r }))
            if (method === "chain_inscribe") return mock.inscription && mock.inscription.problem ? JSON.stringify({ success: false, value: null, error: mock.inscription.problem })
                : ok({ from: args[0], balance: "1191220464", text: args[1], bytes: 181, first: true, fee_estimate: mock.inscription && mock.inscription.noFigure ? null : "1086" })
            if (method === "chain_stake") return ok({ staked: "34002373886", notes: 4538, keys: ["66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"], epoch: 7,
                next_epoch_in_s: 113623, claimable: "26280", vouchers: 2, reward_per_voucher: "13140" })
            if (method === "chain_bridge") { var asked = String(args[2]).replace(/,/g, "")
                return ok({ sent: !!args[3], hash: args[3] ? "ab".repeat(32) : null, to: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: asked,
                    total: mock.exact ? asked : null, notes: mock.exact ? "1" : null, fee: "100000", note: mock.exact ? "e8ac" : null, needs_split: !mock.exact,
                    channel: "01".repeat(32), channel_known: !mock.noChannel, fee_short: !!mock.feeShort, others: mock.feeShort ? "0" : "130294000", lines: [] }) }
            if (method === "chain_split") return ok({ from: args[0], amount: String(args[1]).replace(/,/g, ""), balance: "1191220464", exact: false,
                hash: args[2] ? "cd".repeat(32) : null, fee: args[2] ? "2018" : null })
            if (method === "chain_send") return ok({ from: args[0], to: "8e3b0f4a2c71d95e6a0b4c8d2f1e7a3954c6b0d8e2f4a6c8b0d2e4f6a8c0b2d4", name: args[1] === "hopper" ? "hopper" : null,
                forwarded_from: null, amount: String(args[2]).replace(/,/g, ""), balance: "1191220464", hash: args[3] ? "cd".repeat(32) : null })
            if (method === "cli_accounts" && mock.accountsError) return JSON.stringify({ success: false, value: null, error: mock.accountsError })
            if (method === "cli_accounts") return ok(mock.accounts || [
                { account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", kind: "public", label: null, balance: "1250000", "default": true, public_key: "ab".repeat(32) },
                { account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", kind: "public", label: "Shop", balance: "0", "default": false, public_key: "cd".repeat(32) },
                { account: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", kind: "private", label: "Everyday", balance: "250000000", "default": true, public_key: null },
                { account: "Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2", kind: "private", label: "Savings", balance: "40000000", "default": false, public_key: null },
                { account: "Private/HqRvXf9Lw3jQm5nYc2KpT8sDzV6bR4aE1uGhJ7kNoP2x", kind: "private", label: null, balance: "1000000", "default": false, public_key: null, npk: "b2f49d03" },
                { account: "Private/5tUiY8oPa2sDf4gHj6kLz9xCv3bNm1qWe7rTyUiOpAs4", kind: "private", label: null, balance: "0", "default": false, public_key: null, npk: "7e".repeat(32) }])
            if (method === "cli_fee") return ok({ height: 31012, base_fee_exec: 8, base_fee_stor: 8,
                public_transfer: { about: "8000", at_most: "18000" }, private_transfer: { about: "1601600", at_most: "2002000" } })
            if (method === "cli_account_use" || method === "cli_account_new" || method === "cli_account_label")
                return JSON.stringify({ success: false, value: null, error: "a stand-in: nothing changes here" })
            if (method === "board_follows") return ok(["ada"])
            if (method === "anon_name") return ok(root.anonMock || "")
            if (method === "board_likes") return ok(["jef/0"])
            if (method === "dm_names") return ok({ c1: "ada" })
            if (method === "chat_names") return ok({ aaa1: "ada", bbb2: "hopper" })
            if (method === "spell_check") return ok(/teh|recieve/.test(args[0]) ? [{ word: "recieve", suggestions: ["receive"] }, { word: "teh", suggestions: ["the", "ten", "tea"] }] : [])
            if (method === "set_main_name") return ok(args[0])
            if (method === "board_follow") return ok(args[1] ? [] : ["ada", args[0]])
            if (method === "board_posts_of") return ok({ updating: false, error: null, posts: { total: 14, posts: [
                { id: "L13", parent_id: null, post: "linus/13", name: "linus", n: 13, parent: null, time: Date.now() - 600000, text: "Talk is cheap. Show me the code.", cids: [], likes: 9, hidden: false, block: 0 },
                { id: "L12", parent_id: "Jef0x", post: "linus/12", name: "linus", n: 12, parent: "jef/0", time: Date.now() - 3600000, text: "Nice board. Ship it.", cids: [], likes: 1, hidden: false, block: 0 }] } })
            if (method === "board_feed" || method === "board_thread") return ok({ updating: false, error: null, posts: [
                { id: "Ada1x", parent_id: "Jef0x", post: "ada/1", name: "ada", n: 1, parent: "jef/0", time: Date.now() - 240000, text: "Welcome! First reply on the board.", cids: [], likes: 2, hidden: false, block: 30601 },
                { id: "Jef0x", parent_id: null, post: "jef/0", name: "jef", n: 0, parent: null, time: Date.now() - 1500000, text: "First post on the LNS board. Made privately as a name: the chain sees jef, never my account.", cids: ["zDvZRwzm8K4nJ2a7q9WTqN1"], likes: 5, hidden: false, block: 30564 },
                { id: "Hop3x", parent_id: null, post: "hopper/3", name: "hopper", n: 3, parent: null, time: Date.now() - 10800000, text: "A ship in port is safe, but that is not what ships are built for.", cids: [], likes: 0, hidden: false, block: 29800 }] })
            if (method === "board_post" || method === "board_like" || method === "board_hide")
                return ok({ id: args[0] + "-1792000000000", state: "running", started: Date.now() - 95000 })
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

HOME_CHANNEL = "01" * 32
# The bridge test network (LEZ 0.4) as bridge_net_balances answers it, from the live chain's
# replies of 2026-11-13 (basecamp-module/rust-lib/testdata/bridge-net): the owner's account holds
# 0.1 wrapped ETH, the second public account was never seen there.
NET04_TOKENS = [{"definition": "9ANdXnWBPygeh3mJEDqAtk2AHG6uzZATeyUNyfvZBRVJ", "eth_token": "0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e", "symbol": "dUSDC", "decimals": 6, "name": "Wrapped dUSDC (demo)"},
                {"definition": "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw", "eth_token": "0x0000000000000000000000000000000000000000", "symbol": "wETH", "decimals": 18, "name": "Wrapped Sepolia ETH (demo)"}]
def net04_held(dusdc, weth):
    return [dict(NET04_TOKENS[0], amount=dusdc), dict(NET04_TOKENS[1], amount=weth)]
NET04_READ = {"channel": "b4a9c0de162a2a1b0b6ee9840a90ae90e64bdb49414236679a03e1d86bcc99be", "height": 572, "read_at": 1792000000, "bridge": True,
              "tokens": NET04_TOKENS, "token_problem": None, "reset_at": None,
              "accounts": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "found": True, "native": "23786858", "tokens": net04_held("0", "100000000000000000"), "other_tokens": 0},
                           {"account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "found": False, "native": "0", "tokens": net04_held("0", "0"), "other_tokens": 0}]}
NET04 = {"network": "bridge test network", "version": "LEZ 0.4", "rpc": "https://lez.nameswallet.app/", "read": NET04_READ, "updating": False, "error": None, "private_accounts": 4}
ZONE_B = "03" * 32
# Two zones as zone_list answers them: Home (the testnet) and a second chain with a wallet of its own.
def zones(current, b_wallet=True):
    return [{"id": "home", "name": "Home", "channel": HOME_CHANNEL, "sequencer": "http://127.0.0.1:3040", "home": True, "current": current == "home",
             "wallet_dir": "/Users/you/.lee/wallet", "has_wallet": True, "token_program": "Tok3nProgram1111111111111111111111111111111"},
            {"id": ZONE_B, "name": "Games zone", "channel": ZONE_B, "sequencer": "http://127.0.0.1:3041", "home": False, "current": current == ZONE_B,
             "wallet_dir": "/Users/you/.config/logos-names/zones/" + ZONE_B + "/wallet", "has_wallet": b_wallet, "token_program": None}]
ON_B = {"id": ZONE_B, "name": "Games zone", "channel": ZONE_B, "home": False}
# Another app's payment as pending_requests answers it, asked for the second zone.
def zone_request(in_use, removed=False):
    return ('{ id: "r1", kind: "transfer", app: "forumer_ui", reason: "a tip for the author", created: Date.now(), expires: Date.now() + 600000, asset: "LEZ", asset_kind: "native", amount: "250000", '
            'destination: "ada", destination_kind: "name", zone: "%s", zone_channel: "%s", zone_name: %s, zone_in_use: %s, zone_removed: %s, '
            'choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: true }], '
            'publisher: { status: "verified", publisher: "forumer", module: "forumer_ui", version: "1.2.0" } }'
            % (ZONE_B, ZONE_B, "null" if removed else '"Games zone"', "true" if in_use else "false", "true" if removed else "false"))

SCENARIOS = {
    'card-update-ready': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() {  }) })',
    'receive-update-ready': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
    'receive-restart-needed': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
    'card-restart-needed': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() {  }) })',
    'receive-choose': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
    'receive-ready-to-make': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true }) })',
    'receive-needs': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true; root.againAfter(300, function() { root.makeReceive() }) }) })',
    'receive-code': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true; root.againAfter(300, function() { root.makeReceive() }) }) })',
    'receive-code-details': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true; root.againAfter(300, function() { root.makeReceive() }); root.receiveDetails = true }) })',
    'card-receives': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.receivesLoaded = true; root.receives = [{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }, { id: "dep-2", symbol: "dUSDC", amount: "100", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }, { id: "dep-3", symbol: "ETH", amount: "0.01", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }]; root.receiveStatuses = { "dep-1": { stage: "claimable" }, "dep-2": { stage: "deposited", confirmations: 12, final_in_min: 9 }, "dep-3": { stage: "requested" } } }) })',
    'card-receives-old': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.receivesLoaded = true; root.bridgeConfig = { deployment: {} }; root.receives = [{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }] }) })',
    'claim-arrived': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.claimReceive("dep-1") }) })',
    'claim-arrived-secret': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.claimReceive("dep-1") }) })',
    'backup-state': 'root.section = "settings"; root.settingsOpen = true; ',
    'backup-make': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("make")',
    'backup-made': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("make"); root.againAfter(200, function() { root.makeBackup("correct horse battery") })',
    'backup-made-details': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("make"); root.againAfter(200, function() { root.makeBackup("correct horse battery"); root.backupDetails = true })',
    'backup-restore': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("restore"); root.restoreFile = "/Users/you/Names Wallet Backups/names-wallet-backup-20261114-2350.lnsbackup"',
    'backup-restore-open': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("restore"); root.restoreFile = "/Users/you/Names Wallet Backups/names-wallet-backup-20261114-2350.lnsbackup"; root.againAfter(200, function() { root.openRestoreFile("correct horse battery") })',
    'backup-wrong-password': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("restore"); root.restoreFile = "/Users/you/b.lnsbackup"; root.againAfter(200, function() { root.openRestoreFile("nope") })',
    'backup-restored': 'root.section = "settings"; root.settingsOpen = true; root.openBackup("restore"); root.restoreFile = "/Users/you/b.lnsbackup"; root.againAfter(200, function() { root.restoreFromBackup("x", "lez-0.4") })',
    'claim-approve-sheet': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json"); root.againAfter(400, function() { root.appRequests = [{"id": "r1", "kind": "relayed_call", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", "program": "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "check": "ready", "message_len": 6595, "app_summary": "Claim 100 dUSDC from Sepolia", "sheet": {"lines": ["lez_bridge asks your wallet to sign a call that another account pays for.", "Network: LEZ 0.4 \\u00b7 bridge test network", "Program: JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7 (the Program Registry isn\\\'t installed, so it has no label)", "Signs: Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak, your account", "Fee: paid by Public/1HHhGGKfPayerRelayPayerAccount1MgNT, not by you: at most 9,300,135 (gas limit 200,000, tip 0).", "Reaches Public/HugSoWm7oKZ4rjuqDwy1Yxs7aWvN5AqsqgSbQwQUqKf3 (data for JECKBk\\u2026nUx7).", "Reaches Public/FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ (data for 4VScJ7\\u2026eSex).", "Creates the program\\\'s account Public/END4S7ZPj9bQyB7NYeB4ka8RWpBpWzmiFZ5jRA7cXFu.", "Exactly these bytes are signed: 5f1c0e77c2a1b6f0a8d3e94b7c2d10f3e6a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5 (the message is 6,595 bytes, SHA-256 9b1d\\u2026)."], "warnings": ["The Program Registry isn\\\'t installed, so who wrote this program and whether its source is verified is unknown."], "privacy": "A public claim publishes the claim secret and links the Sepolia deposit to this account.", "after": "Your wallet signs and gives the transaction back to the app; the other account sends it and pays the fee. It can\\\'t see what happens to it after that. The payer can only submit exactly these bytes, and only once on this chain: changing the program, an account, the fee or one byte of the message breaks your signature. After a reset of this test network, or on another LEZ 0.4 chain where your account\\\'s nonce comes back to what it is now, the payer could submit it again.", "app_says": "Claim 100 dUSDC from Sepolia", "hash": "5f1c", "refused": false, "why": []}, "choices": [{"account": "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", "account_kind": "public", "pays": false, "label": "the account the app asked for", "granted": true}], "publisher": null}]; root.appChosen = ["Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak"]; root.depositClaimJob = { id: "rc-job-1", state: "running", phase: "approve", can_cancel: false } }) }) })',
    'claim-notfinal': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json");  }) })',
    'claim-checkagain': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json"); root.againAfter(400, function() { root.checkAgain() }) }) })',
    'claim-relaycap': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json");  }) })',
    'claim-details': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json"); root.claimDetails = true }) })',
    'claim-install': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim();  }) })',
    'claim-install-asking': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.bridgeInstall = { phase: "asking", line: "Basecamp asks you to confirm the install…" } }) }) }) })',
    'claim-installing': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.bridgeInstall = { phase: "installing", line: "Installing the LEZ Bridge module…" } }) }) }) })',
    'claim-install-slow': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.bridgeInstall = { phase: "slow", line: "Still installing the LEZ Bridge app…" } }) }) }) })',
    'claim-install-failed': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.bridgeInstall = { phase: "failed", line: "Basecamp hasn\'t learnt yet that Names may ask this (it does at its next start), so nothing was installed. Or install it yourself: Package Manager (the cube icon) → Install Local Package, the file /Users/you/Library/Application Support/Logos/LogosBasecamp/plugins/names_ui/qml/bundled/lez_bridge.lgx, then /Users/you/Library/Application Support/Logos/LogosBasecamp/plugins/names_ui/qml/bundled/lez_bridge_ui.lgx." } }) }) }) })',
    'claim-idle': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim();  }) })',
    'claim-starting': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.bridgeInstall = { phase: "starting", line: "Starting the LEZ Bridge: Basecamp opens its app for a moment, then Names comes back." } }) }) }) })',
    'claim-older': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim();  }) })',
    'claim-updated': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); Qt.callLater(function() { Qt.callLater(function() { root.claimCheckWith = false; root.bridgeInstall = { phase: "updated", line: "Updated. To use it now: Settings (the gear) → Module Inspector → LEZ Bridge → Unload, then Start here." } }) }) }) })',
    'approve-checking-publisher': 'root.appRequests = [{ id: "r7", kind: "connect", app: "lez_bridge", reason: "To claim bridge deposits into your public account", created: Date.now(), expires: Date.now() + 600000, checking: true, checking_what: "who publishes lez_bridge", choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "checking" } }]; root.publisherSince = { r7: Date.now() - 4000 }; root.publisherNow = Date.now()',
    'wallet-starting': 'root.openWallet("home"); root.sayRead(root.moduleNoAnswer)',
    'setup-card-stuck': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.installedList = [{ name: "names_ui", installDir: "/x" }, { name: "lez_bridge", hashes: { root: "ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7" } }]; root.bridgeAnswers = false; root.bridgeFails = 2 }) })',
    'setup-receive-unload': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.againAfter(300, function() { root.bridgeInstall = { phase: "unload" } }) }) })',
    'setup-claim-stale': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.againAfter(300, function() { root.bridgeAnswers = false; root.bridgeStale = true; root.claimMissing = true }) }) })',
    'setup-receive-done': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.againAfter(300, function() { root.bridgeDoneView = "ethreceive"; root.bridgeDone = "LEZ Bridge updated." }) }) })',
    'setup-receive-install': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
    'add-account-sheet': 'root.appRequests = [{ id: "r8", kind: "connect", adding: true, app: "lez_bridge", reason: "to receive into this account", account_hint: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", created: Date.now(), expires: Date.now() + 600000, granted: ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"], choices: [{ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", account_kind: "public", label: "Names wallet, public account", granted: false }], publisher: { status: "unavailable" } }]; root.appChosen = Logic.preTicked(root.appRequests[0], false, "")',
    'add-account-sheet-app': 'root.appRequests = [{ id: "r8", kind: "connect", adding: true, app: "tips_ui", reason: "to show this balance too", account_hint: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", created: Date.now(), expires: Date.now() + 600000, granted: ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"], choices: [{ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", account_kind: "public", label: "Names wallet, public account", granted: false }], publisher: { status: "unavailable" } }]; root.appChosen = Logic.preTicked(root.appRequests[0], false, "")',
    'claim-card': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    'claim-missing': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim();  }) })',
    'claim-start': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim();  }) })',
    'claim-checking': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimChecking = true }) })',
    'claim-ready': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json") }) })',
    'claim-elsewhere': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json") }) })',
    'claim-wrong-bridge': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.bridgePrograms = ["4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr"]; root.checkClaimFile("/Users/you/Downloads/claim.json") }) })',
    'claim-refused': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.checkClaimFile("/Users/you/Downloads/claim.json") }) })',
    'claim-needs': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; root.claimNeeds = { needs: "connect", request: { request: "r9", state: "pending" } }; root.claimNeedsState = "pending" }) })',
    'claim-sheet': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; root.claimNeeds = { needs: "connect", request: { request: "r9", state: "pending" } }; root.claimNeedsState = "pending"; root.appRequests = [{ id: "r9", kind: "connect", app: "lez_bridge", reason: "To claim bridge deposits into your public account", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", label: "Shop", granted: false }, { account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "unavailable" } }]; root.appChosen = Logic.preTicked(root.appRequests[0], false, root.claimAccount) }) })',
    'claim-uncertain': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; root.claimUncertain = { needs: "uncertain", uncertain: "propose:rc-0123456789abcdef", message: "Your Names Wallet didn\'t answer the claim. If this app shows a claim for this deposit, reject it there, then press Continue." } }) })',
    'claim-busy': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; var f = Logic.claimFailure("Your Names Wallet is busy; nothing was sent; try again."); root.claimError = f.line; root.claimAgain = f.again }) })',
    'claim-noanswer': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; var f = Logic.claimFailure("Your Names Wallet didn\'t answer in time (60 s). Nothing was sent; try again."); root.claimError = f.line; root.claimAgain = f.again }) })',
    'claim-running': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." } }) })',
    'claim-done': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; root.depositClaimJob = { id: "rc-job-1", state: "done", phase: "done", can_cancel: false, account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", block: 5249, note: "Finished.",     result: { balance_text: "0.1 ETH", native: "5688340" }, error: null } }) })',
    'claim-failed': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openClaim(); root.claimCheck = { ok: true, id: "rc-0123456789abcdef", network: "LEZ 0.4 · bridge test network",     wording: "The wallet signs the claim; the bridge\'s relay sends it and pays the fee.",     privacy: "A public claim publishes the claim secret and links the Sepolia deposit to this account.",     deposit: { amount_text: "0.1 ETH", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", leaf_index: 7, leaves: 9 },     relay: { payer: "Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq", max_fee: "2500000" },     call: { program: "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr" },     checks: [{ key: "file", ok: true, text: "The claim file reads, and its secret matches its commitment" },              { key: "bridge", ok: true, text: "It is for this bridge and its vault" },              { key: "deposit", ok: true, text: "The deposit is in the vault, final on Sepolia" },              { key: "root", ok: true, text: "Its root is recorded on the bridge test network" },              { key: "unclaimed", ok: true, text: "It hasn\'t been claimed" },              { key: "relay", ok: true, text: "The relay pays the fee" }],     wallet: { needs: null }, approval: "This app shows the call your wallet built and asks you to approve it." }; root.depositClaimJob = { id: "rc-job-1", state: "failed", phase: "relay", can_cancel: false, account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", block: 5249, note: "",     result: { balance_text: "0.1 ETH", native: "5688340" }, error: { code: "relay_refused", message: "the relay refused it: its nonce moved" } } }) })',
    "start": "",
    "wallet-names": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletView = "names"',
    "wallet-totals": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.walletSetAll(false) })',
    "wallet-row": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletRowOpen = "chain-66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"',
    "wallet-row-lez": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletRowOpen = "lez-Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"',
    "wallet-hidden": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletHidden = true',
    "wallet-bridge-split": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); bridgeAmountField.text = "1000000"; root.bridgeRun(false) })',
    "wallet-bridge-exact": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); bridgeAmountField.text = "1000000"; root.bridgeRun(false) })',
    "wallet-bridge-nochannel": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); bridgeAmountField.text = "1000000"; root.bridgeRun(false) })',
    "wallet-bridge-feeshort": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); bridgeAmountField.text = "1000000"; root.bridgeRun(false) })',
    "wallet-bridge-confirm": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); bridgeAmountField.text = "1000000"; root.bridgeRun(false); root.bridgeConfirm = true })',
    "wallet": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "wallet-chain-account-new": 'root.openWallet("home"); root.chainAccountNew("Savings")',
    "wallet-chain-notready-page": 'root.openWallet("home"); Qt.callLater(function() { var g = root.walletGroups.filter(function(x) { return x.id === "chain" })[0]; root.openAccount(g.rows[g.rows.length - 1]) })',
    "wallet-chain-ready-page": 'root.openWallet("home"); Qt.callLater(function() { var g = root.walletGroups.filter(function(x) { return x.id === "chain" })[0]; root.openAccount(g.rows[0]) })',
    "wallet-claim": 'root.openWallet("home")',
    "wallet-account-low": 'root.openWallet("home"); Qt.callLater(function() { root.openAccount(root.walletGroups[0].rows[1]) })',
    "wallet-account-main": 'root.openWallet("home"); Qt.callLater(function() { root.openAccount(root.walletGroups[0].rows[0]) })',
    "wallet-account-chain": 'root.openWallet("home"); Qt.callLater(function() { var g = root.walletGroups.filter(function(x) { return x.id === "chain" || (x.rows[0] && x.rows[0].kind === "chain") })[0]; root.openAccount(g.rows[0]) })',
    "wallet-activity-page": 'root.go("activity"); root.loadActivity()',
    "wallet-names-transfer": 'root.openWallet("names"); root.loadCliMine(); Qt.callLater(function() { root.openTransfer("hopper"); transferToField.text = "9uwkVJuXScDwLKVtQMcHFU9Hvb3uZjiEDVvDv4PYjCfM"; root.transferConfirm = true })',
    "wallet-send-address": 'root.openWallet("home"); Qt.callLater(function() { root.openSend(root.walletGroups[0].rows.filter(function(r) { return r.canSend && !r.fromPrivate })[0], ""); sendTo.text = "9uwkVJuXScDwLKVtQMcHFU9Hvb3uZjiEDVvDv4PYjCfM"; sendAmount.text = "69420000" })',
    "wallet-row-history": 'root.openWallet("home"); root.walletRowOpen = "chain-66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"',
    "wallet-bridge-private": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeToPrivate = true; bridgeAmountField.text = "1000000" })',
    "wallet-bridge-from-lez": 'root.openWallet("home"); Qt.callLater(function() { root.openBridgeFromLez("Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk") })',
    "wallet-bridge-plain": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens() })',
    "wallet-bridge-open": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeFromOpen = true })',
    "wallet-bridge-open-to": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeToOpen = true })',
    "settings-network": 'root.section = "settings"; root.settingsOpen = true; registryField.text = "9W9huNeAds11pdGpMijrdhTkSQ8upNudRnrZJbNNWfoW"; root.networkConfirm = true',
    # Zones (LP-0022): the list with one row open, a new zone, Home's sequencer naming another chain,
    # the wallet and the names on a second zone, and another app's request asked for a zone.
    "settings-zones": 'root.section = "settings"; root.zoneOpen = "%(zone_b)s"; root.zoneNote = "Sequencer changed. The wallet, its accounts and its history are as they were."',
    "settings-zones-add": 'root.section = "settings"; root.zoneAdding = true',
    "settings-zones-remove": 'root.section = "settings"; root.zoneOpen = "%(zone_b)s"; root.zoneRemoving = true',
    "settings-zones-other-chain": 'root.section = "settings"; root.zoneOpen = "home"; root.zoneOtherChain = { url: "http://127.0.0.1:3042", channel: "0505050505050505050505050505050505050505050505050505050505050505" }',
    "wallet-zone": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "wallet-zone-nowallet": 'root.openWallet("home")',
    "wallet-zone-names": 'root.openWallet("home"); root.walletView = "names"',
    "wallet-zone-activity": 'root.go("activity"); root.loadActivity()',
    "wallet-zone-review": 'root.openWallet("home"); root.openSend({ id: "lez-public", kind: "lez", title: "LEZ", definition: "", fromPrivate: false }, "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"); sendAmount.text = "1000"; root.sendReview()',
    "approve-zone": 'root.appRequests = [%(zone_request)s]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "approve-other-zone": 'root.appRequests = [%(zone_request)s]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "approve-removed-zone": 'root.appRequests = [%(zone_request_gone)s]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "wallet-send": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openSend({ id: "lez-private", kind: "lez", title: "LEZ", definition: "", fromPrivate: true }, "ada"); sendAmount.text = "1000"',
    "wallet-review": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openSend({ id: "lez-private", kind: "lez", title: "LEZ", definition: "", fromPrivate: true }, "ada"); sendAmount.text = "1000"; root.sendReview()',
    "wallet-review-public": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openSend({ id: "lez-public", kind: "lez", title: "LEZ", definition: "", fromPrivate: false }, "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"); sendAmount.text = "1000"; root.sendReview()',
    "wallet-send-bad": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openSend({ id: "lez-public", kind: "lez", title: "LEZ", definition: "", fromPrivate: false }, "Public/7wx3U7qMRWop"); sendAmount.text = "1000"',
    "wallet-sending": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openSend({ id: "lez-public", kind: "lez", title: "LEZ", definition: "", fromPrivate: false }, "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"); sendAmount.text = "1000"; root.sendConfirm()',
    "wallet-chain-review": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.chainFrom = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"; root.openSend({ id: "chain", kind: "chain", title: "Logos Blockchain", definition: "", fromPrivate: false }, "hopper"); sendAmount.text = "1,000,000"; root.sendReview() })',
    "wallet-receive": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openReceive({ id: "lez-private", kind: "lez", title: "LEZ", definition: "", fromPrivate: true })',
    "wallet-receive-chain": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.openReceive({ id: "chain", kind: "chain", title: "Logos Blockchain", definition: "", fromPrivate: false })',
    "wallet-stake": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletView = "stake"',
    "wallet-add": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); root.walletView = "add"; addTokenField.text = "CoPPERdef1nition111111111111111111111111111"',
    "wallet-add-noprogram": 'root.openWallet("add")',
    "wallet-bridge": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home"); Qt.callLater(function() { root.openGetTokens() })',
    "wallet-down": 'root.openWallet("home")',
    "wallet-net03": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "wallet-net03-light": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "wallet-net04": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-light": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-down": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-never": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-reset": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-many": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "wallet-net04-many-open": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); root.bridgeNetShowEmpty = true })',
    "wallet-net04-watch": 'root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); bridgeNetWatchField.text = "Public/0OIl"; root.bridgeNetWatch(bridgeNetWatchField.text, true) })',
    "wallet-net04-activity": 'root.go("activity"); root.loadActivity()',
    "wallet-net04-send": 'root.openWallet("home"); root.openSend({ id: "lez-public", kind: "lez", title: "LEZ", definition: "", fromPrivate: false }, "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"); sendAmount.text = "1000"',
    "wallet-waiting": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "home-reply": 'root.boardReply("Jef0x")',
    "anon-off": 'root.anonOn = true',
    "poster": 'root.show("linus")',
    "messages": 'root.section = "messages"; root.chatLoad(); root.chatOpen("c1")',
    "group": 'root.section = "messages"; root.chatLoad(); Qt.callLater(function() { root.chatOpen("g1") })',
    "spelling": 'boardComposer.text = "I recieve teh posts"; root.boardSpellCheck()',
    "profile-tab": 'root.openProfile()',
    "settings-tab": 'root.section = "settings"',
    "search-tab": 'root.section = "search"',
    "welcome": 'root.cliPath = ""',
    # 0.17 final: a wallet with no name on LEZ 0.4 opens to the Wallet (a "No name yet" card), not Welcome.
    "noname-net04-wallet": 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    # No wallet on LEZ 0.4: Welcome, with the network switch and "Restore from my recovery phrase".
    "nowallet-net04-welcome": 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    # An app's connect request opens over both.
    "noname-net04-request": 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); root.appRequests = [{ id: "r9", kind: "connect", app: "lez_bridge", reason: "to claim your bridge deposits", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], granted: [], publisher: { status: "unavailable" }, checking: false }] })',
    "nowallet-net04-request": 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); root.appRequests = [{ id: "r9", kind: "connect", app: "lez_bridge", reason: "to claim your bridge deposits", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], granted: [], publisher: { status: "unavailable" }, checking: false }] })',
    # The October CLI on LEZ 0.3 (no `module` command), no wallet yet: the outdated line, and the switch.
    "welcome-outdated-03": 'root.cliNames = []; root.cliPending = []',
    # LEZ 0.4 with no names tool set: no Install offered, the path is asked for in Settings.
    "welcome-net04-nocli": 'root.cliNames = []; root.cliPending = []; Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { Qt.callLater(function() { root.cliPath = "" }) }) })',
    # LEZ 0.4, a wallet with nothing in it: no faucet, no Logos node bridge, a plain line.
    "welcome-net04-funding": 'root.cliNames = []; root.cliPending = []; Qt.callLater(function() { root.setWalletNet("0.4") })',
    # Register after a check: a free name opens its page on Search (the switch is there), a taken
    # one stays on the form (its own switch).
    "register-free": 'root.openRegister(); root.checkRegister("penisface")',
    "register-taken": 'root.openRegister(); root.checkRegister("hopper")',
    # Search with a result shown: the switch stays.
    "search-result-switch": 'root.section = "search"; root.show("hopper")',
    "welcome-register": 'root.cliNames = []; root.openRegister(); root.checkRegister("penisface")',
    "register-check": 'root.openRegister(); root.checkRegister("penisface")',
    "home-thread": 'root.boardShow("thread", "jef/0")',
    "name": 'root.show("jef")',
    "plain": 'root.show("linus")',
    "register": 'root.show("satoshi")',
    # The names network on LEZ 0.4: register from a public account only; the Names view and Send say so.
    "register-net04": 'root.setWalletNet("0.4"); Qt.callLater(function() { Qt.callLater(function() { root.show("satoshi") }) })',
    "wallet-names-net04": 'root.openWallet("names"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    # A public registration under way, with the module busy on two polls: the countdown shows,
    # and nothing says the module is missing.
    "names-reveal-countdown": 'root.openWallet("names"); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "register-net04-rules": 'root.openRegister(); Qt.callLater(function() { root.setWalletNet("0.4") })',
    "send-net04": 'root.setWalletNet("0.4"); Qt.callLater(function() { root.payTo("ada") })',
    "register-pending": 'root.show("knuth")',
    "register-nocli": 'root.show("satoshi"); root.cliPath = ""',
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
    "release": 'root.manage("grace"); root.editTab = "giveup"; root.confirmingRelease = true',
    "edit-page": 'root.manage("jef"); root.editTab = "page"',
    "edit-giveup": 'root.manage("grace"); root.editTab = "giveup"',
    "register-public": 'root.show("satoshi"); root.publicRegisterOpen = true',
    "register-new": 'root.openRegister()',
    "register-taken": 'root.openRegister(); root.checkRegister("jef")',
    "register-step1": 'root.show("satoshi"); root.registerPrivately()',
    "register-poor": 'root.show("pauper")',
    "lapsed": 'root.manage("grace")',
    "waiting": 'root.manage("grace"); root.releaseName()',
    "register-settings": 'root.show("jef"); root.settingsOpen = true',
    # hopper.lez is held by the names CLI: the same settings, minus what only the LEZ wallet does.
    "cli-settings": 'root.manage("hopper")',
    "cli-saving": 'root.manage("hopper"); root.saveText("inbox", "/logos/1/hopper/new")',
    "cli-failed": 'root.manage("hopper"); root.saveText("inbox", "/logos/1/hopper/new"); Qt.callLater(function() { Qt.callLater(function() { root.checkCliJob() }) })',
    "cli-nowallet": 'root.manage("hopper")',
    "unset": "",
    "welcome-tokens": 'root.cliNames = []; root.cliPending = []',
    "get-tokens": 'root.openGetTokens()',
    "get-tokens-plan": 'root.openGetTokens(); bridgeAmountField.text = "5000000"; root.bridgeRun(false)',
    "get-tokens-confirm": 'root.openGetTokens(); bridgeAmountField.text = "5000000"; root.bridgeRun(false); Qt.callLater(function() { Qt.callLater(function() { root.bridgeConfirm = true }) })',
    "get-tokens-sent": 'root.openGetTokens(); bridgeAmountField.text = "5000000"; root.bridgeRun(false); Qt.callLater(function() { Qt.callLater(function() { root.bridgeRun(true) }) })',
    "get-tokens-error": 'root.openGetTokens(); bridgeAmountField.text = "999999999"; root.bridgeRun(false)',
    "get-tokens-badkey": 'root.openGetTokens(); bridgeKeyField.text = "not a key"; bridgeAmountField.text = "5"; root.bridgeRun(false)',
    "get-tokens-nowallet": 'root.openGetTokens()',
    "get-tokens-move": 'root.openGetTokens(); Qt.callLater(function() { root.moveToPrivate() })',
    # The Welcome steps, from nothing installed to picking a name.
    "welcome-install": 'root.cliPath = ""; root.cliNames = []; root.cliPending = []',
    "welcome-terminal": 'root.cliPath = ""; root.cliNames = []; root.cliPending = []; root.terminalNoteOpen = true',
    "welcome-installing": 'root.cliPath = ""; root.cliNames = []; root.cliPending = []; root.installCli(); Qt.callLater(function() { Qt.callLater(function() { root.checkInstall() }) })',
    "welcome-install-failed": 'root.cliPath = ""; root.cliNames = []; root.cliPending = []; root.installCli(); Qt.callLater(function() { Qt.callLater(function() { root.checkInstall() }) })',
    "welcome-wallet": 'root.cliNames = []; root.cliPending = []',
    "welcome-phrase": 'root.cliNames = []; root.cliPending = []; root.createWallet()',
    "welcome-wallet-error": 'root.cliNames = []; root.cliPending = []; root.walletError = "The names CLI failed: cannot write to /Users/you/.lee/wallet."',
    "welcome-unfunded": 'root.cliNames = []; root.cliPending = []',
    "welcome-funded": 'root.cliNames = []; root.cliPending = []',
    "welcome-faucet": 'root.cliNames = []; root.cliPending = []; root.getFreeTokens()',
    "welcome-faucet-error": 'root.cliNames = []; root.cliPending = []; root.getFreeTokens()',
    "welcome-moving": 'root.cliNames = []; root.cliPending = []; root.moveToPrivate()',
    "welcome-move-poor": 'root.cliNames = []; root.cliPending = []; root.moveToPrivate(); Qt.callLater(function() { Qt.callLater(function() { root.checkShield() }) })',
    "welcome-moved": 'root.cliNames = []; root.cliPending = []',
    # Paying an account rather than a name.
    "approve-private-call": 'root.appRequests = [{"id": "r1", "kind": "private_call", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "check": "ready", "message_len": 6595, "prove_expect": "Proving runs on this computer: about 11 minutes and about 9.5 GB of memory (measured for a bridge claim on an Apple M4 with 16 GB, 4 threads). Other apps may be slow meanwhile.", "app_summary": "Claim 100 dUSDC from Sepolia", "sheet": {"lines": ["Private/2if1EJ…6HUC receives 20,000,000 LEZ.", "Private/2if1EJ…6HUC receives 100 (100,000,000 base units) dUSDC (FBqNAw…LQZJ).", "Nothing leaves your account except the fee.", "Fee: about 5,723,160 LEZ, at most 9,300,135, paid inside the proof from your account; what isn\'t used comes back to it."], "public_changes": ["Public/END4S7…cXFu: JECKBk…nUx7\'s data changes", "Public/HugSoW…qKf3: JECKBk…nUx7\'s data changes", "dUSDC (FBqNAw…LQZJ): supply 200,000,000 → 300,000,000", "Public/HugSoW…qKf3: LEZ 80,000,000 → 60,000,000"], "privacy": ["Public: the programs it runs (3), the public accounts it reaches and the changes above, the fee reserve, and the time.", "Public: the one-time address B8UnZG…i3wT and what it receives. Nobody can tell it is yours.", "Private: Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC, its balance, and the message to it (proven, not shown).", "Checked: your account\'s ID appears nowhere in the public state and events this call leaves (the messages between programs aren\'t checked)."], "programs": [{"program": "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}, {"program": "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}, {"program": "4VhxvwLN5n3u3rZbsz2VNFVavLwc1niNvgxq8JJ15veK", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}], "warnings": ["This app hasn\'t used JECKBk…nUx7 (JECKBk…nUx7) before.", "The Program Registry isn\'t installed, so who wrote these programs and whether their source is verified is unknown."], "refusals": [], "refused": false, "digest": "4ed3"}, "choices": [{"account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "account_kind": "private", "pays": false, "label": "the account the app asked for", "granted": true}], "zone": "home", "zone_name": "Home", "zone_in_use": true, "zone_removed": false, "publisher": null}]; root.appChosen = ["Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC"]; root.appChosenFor = "r1"',
    "approve-private-refused": 'root.appRequests = [{"id": "r1", "kind": "private_call", "app": "evil_ui", "created": 0, "expires": 4102444800000, "account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "check": "ready", "message_len": 6595, "prove_expect": "Proving runs on this computer: about 11 minutes and about 9.5 GB of memory (measured for a bridge claim on an Apple M4 with 16 GB, 4 threads). Other apps may be slow meanwhile.", "app_summary": "Free airdrop!", "sheet": {"lines": ["Private/2if1EJ…6HUC pays 100 (100,000,000 base units) dUSDC (FBqNAw…LQZJ).", "Fee: about 5,723,160 LEZ, at most 9,300,135, paid inside the proof from your account; what isn\'t used comes back to it."], "public_changes": ["Public/6NAoZK…4f4f: dUSDC (FBqNAw…LQZJ) 100,000,000 → 200,000,000"], "privacy": ["Public: the programs it runs (3), the public accounts it reaches and the changes above, the fee reserve, and the time.", "Public: the one-time address B8UnZG…i3wT and what it receives. Nobody can tell it is yours.", "Private: Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC, its balance, and the message to it (proven, not shown).", "Checked: your account\'s ID appears nowhere in the public state and events this call leaves (the messages between programs aren\'t checked)."], "programs": [{"program": "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}, {"program": "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}, {"program": "4VhxvwLN5n3u3rZbsz2VNFVavLwc1niNvgxq8JJ15veK", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}, "new_for_app": true}], "warnings": ["This app hasn\'t used JECKBk…nUx7 (JECKBk…nUx7) before.", "The Program Registry isn\'t installed, so who wrote these programs and whether their source is verified is unknown."], "refusals": ["It takes 100 (100,000,000 base units) dUSDC (FBqNAw…LQZJ) from your account, but the app declared 0. The wallet won\'t sign what takes more than the app said."], "refused": true, "digest": "4ed3"}, "choices": [{"account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "account_kind": "private", "pays": false, "label": "the account the app asked for", "granted": true}], "zone": "home", "zone_name": "Home", "zone_in_use": true, "zone_removed": false, "publisher": null}]; root.appChosen = ["Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC"]; root.appChosenFor = "r1"',
    "approve-relayed-call": 'root.appRequests = [{"id": "r1", "kind": "relayed_call", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", "program": "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "check": "ready", "message_len": 6595, "app_summary": "Claim 100 dUSDC from Sepolia", "sheet": {"lines": ["lez_bridge asks your wallet to sign a call that another account pays for.", "Network: LEZ 0.4 \u00b7 bridge test network", "Program: JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7 (the Program Registry isn\'t installed, so it has no label)", "Signs: Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak, your account", "Fee: paid by Public/1HHhGGKfPayerRelayPayerAccount1MgNT, not by you: at most 9,300,135 (gas limit 200,000, tip 0).", "Reaches Public/HugSoWm7oKZ4rjuqDwy1Yxs7aWvN5AqsqgSbQwQUqKf3 (data for JECKBk\u2026nUx7).", "Reaches Public/FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ (data for 4VScJ7\u2026eSex).", "Creates the program\'s account Public/END4S7ZPj9bQyB7NYeB4ka8RWpBpWzmiFZ5jRA7cXFu.", "Exactly these bytes are signed: 5f1c0e77c2a1b6f0a8d3e94b7c2d10f3e6a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5 (the message is 6,595 bytes, SHA-256 9b1d\u2026)."], "warnings": ["The Program Registry isn\'t installed, so who wrote this program and whether its source is verified is unknown."], "privacy": "A public claim publishes the claim secret and links the Sepolia deposit to this account.", "after": "Your wallet signs and gives the transaction back to the app; the other account sends it and pays the fee. It can\'t see what happens to it after that. The payer can only submit exactly these bytes, and only once on this chain: changing the program, an account, the fee or one byte of the message breaks your signature. After a reset of this test network, or on another LEZ 0.4 chain where your account\'s nonce comes back to what it is now, the payer could submit it again.", "app_says": "Claim 100 dUSDC from Sepolia", "hash": "5f1c", "refused": false, "why": []}, "choices": [{"account": "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", "account_kind": "public", "pays": false, "label": "the account the app asked for", "granted": true}], "publisher": null}]; root.appChosen = ["Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak"]; root.appChosenFor = "r1"',
    "approve-programs": 'root.appRequests = [{"id": "r2", "kind": "programs", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "programs": ["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7"], "app_summary": "To claim your bridge deposits", "choices": [{"account": "Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak", "account_kind": "public", "pays": false, "label": "Public account", "granted": true}], "publisher": null, "sheet": {"programs": [{"program": "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "label": {"registry": false, "text": null, "kind": null, "verified": null, "stale": null, "upgradeable": null}}], "warnings": ["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7: The Program Registry isn\'t installed, so who wrote this program and whether its source is verified is unknown."]}}]; root.appChosen = ["Public/7kHXTZHzMCz3PgiQVA9PE5DZLa18xpYvijdPCPK4J6ak"]; root.appChosenFor = "r2"',
    "approve-secret": 'root.appRequests = [{"id": "r1", "kind": "secret", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "scope": "RFP022/deposit", "reason": "", "choices": [{"account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "account_kind": "private", "pays": false, "label": "the account the app asked for", "granted": true}], "publisher": null, "explanation": "lez_bridge asks for secret values of its own, \\"RFP022/deposit\\", made from Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC. The wallet makes them from this account\'s key, so they come back if you restore the wallet from its recovery phrase. lez_bridge gets only these values, never the account\'s key, and no other app can get them. Whoever holds them can see what lez_bridge does with them (for a bridge, which deposits are yours), but can\'t spend from your account: every transaction still needs your approval here. You can take this back in Settings, Connected apps."}]; root.appChosen = ["Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC"]; root.appChosenFor = "r1"',
    "approve-token-source": 'root.appRequests = [{"id": "r1", "kind": "token_source", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "token_program": "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex", "publisher": null, "definitions": [{"definition": "FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ", "name": "Wrapped dUSDC", "symbol": "dUSDC", "decimals": 6, "origin": "Sepolia 0xAc20…a71e"}, {"definition": "CswQqK9jzUHf5sraiM8G9HfgMT2bcJ8GhVpbAA9trwQ6", "name": "Wrapped ETH", "symbol": "wETH", "decimals": 18, "origin": "Sepolia ETH"}], "choices": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "label": "Names wallet, main public account", "granted": true}]}]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "wallet-bridged": 'root.tokenProgram = "Tok3nProgram1111111111111111111111111111111"; root.openWallet("home")',
    "approve-verified": 'root.appRequests = [{ id: "r1", kind: "connect", app: "forumer_ui", reason: "to post on the forum as you", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "verified", publisher: "forumer", module: "forumer_ui", version: "1.2.0" } }]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "approve-fake": 'root.appRequests = [{ id: "r1", kind: "connect", app: "forumer_ui", reason: "to post on the forum as you", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "mismatch", publisher: "forumer", module: "forumer_ui", version: "1.2.0" } }]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    # An app asks for a testimonial inscribed on the Logos Blockchain (the stand-in's pending_inscriptions).
    "inscribe": 'root.openWallet("home")',
    "inscribe-short": 'root.openWallet("home")',
    "inscribe-nofigure": 'root.openWallet("home")',
    "inscribe-node-down": 'root.openWallet("home")',
    "inscribe-fake": 'root.openWallet("home")',
    "inscribe-fake-chosen": 'root.openWallet("home"); Qt.callLater(function() { Qt.callLater(function() { root.chooseInscribeAccount("73058ad5edb0e72b66c3e05ae90471382c3bb4d61314db995ef40e674dbf591e"); root.inscribeUnderstood = true }) })',
    "settings-oldcli": 'root.section = "settings"; root.loadAccounts()',
    "settings-accounts": 'root.section = "settings"; root.loadAccounts(); root.accountRenaming = "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"',
    "settings-account-new": 'root.section = "settings"; root.loadAccounts(); root.accountNewKind = "private"',
}

# What the module's stand-in says in each Welcome state (see `mock` in BRIDGE).
NO_WALLET = {"status": {"wallet": False, "public": None, "private": None}, "balance": None}
# Entries on both zones, as `activity` answers them once there are two.
ZONE_ACTIVITY = [
    {"id": "z1", "at": 1792000000, "kind": "pay_account", "chain": "lez", "zone": ZONE_B, "zone_name": "Games zone", "to": "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg", "amount": "1000", "fee": "4088", "state": "done"},
    {"id": "z2", "at": 1791990000, "kind": "pay", "chain": "lez", "zone_name": "Home", "name": "ada", "amount": "250000", "fee": "4088", "state": "done"},
    {"id": "z3", "at": 1791980000, "kind": "pay_account", "chain": "lez", "zone": "07" * 32, "zone_name": None, "to": "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg", "amount": "77", "state": "done"},
    {"id": "z4", "at": 1791970000, "kind": "send", "chain": "logos", "from": "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", "to": "73058ad5edb0e72b66c3e05ae90471382c3bb4d61314db995ef40e674dbf591e", "amount": "5000", "fee": "2018", "state": "in_block"},
]
MOCKS = {
    'setup-card-stuck': {'bridge': {}},
    'setup-receive-unload': {'bridge': {}},
    'setup-claim-stale': {'bridge': {}},
    'setup-receive-done': {'bridge': {}},
    'setup-receive-install': {'packages': []},
    'card-update-ready': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': '788f2f0dde4bd7ae'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
    'receive-update-ready': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': '788f2f0dde4bd7ae'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
    'receive-restart-needed': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
    'card-restart-needed': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
    'receive-choose': {'bridge': {}},
    'receive-ready-to-make': {'bridge': {}},
    'receive-needs': {'bridge': {'make_public_deposit_request': {'needs': 'secret', 'request': {'request': 'r5', 'state': 'pending'}, 'message': 'Approve "RFP022/deposit" in the Names app…'}, 'wallet_request': {'state': 'pending'}}},
    'receive-code': {'bridge': {'make_public_deposit_request': {'deposit': {'id': 'dep-a50e0f44940e', 'symbol': 'ETH', 'account': 'CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'account_kind': 'public'}, 'symbol': 'ETH', 'denom_text': '0.1', 'total_text': '0.1003', 'bridge_fee': {'desk': '0x6b7F38D0…DBc5', 'fee_bps': 30, 'percent_text': '0.3%', 'amount_text': '0.0003', 'total_text': '0.1003', 'text': 'Bridge fee 0.0003 ETH (0.3% of 0.1 ETH); you send 0.1003 ETH in all.'}, 'commitment': '0xa50e0f44940eca30735460fa9b657d6ab948fc422b29b1293be0f7e5d6b7ad2e', 'request': 'logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty', 'request_url': 'https://nameswallet.app/bridge/#logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty', 'request_qr': {'size': 33, 'rows': ['111111111010110111110110001111111', '100000110101111010010111011000001', '101110110110010000101101111011101', '101110100001000100101000011011101', '101110110111110111011000011011101', '100000111001111101111000001000001', '111111101000011101111111111111111', '110011111001111110110010100001110', '001100000010111110010001111100010', '010000110100110011100100010110000', '100111101110110010000111101100100', '010110000010000101000010001111110', '001001001011001101110101110010001', '110111001100011011000101011001101', '111111010111110011010110100010001', '111101110001111111000101111101011', '111111000000110100100000100000000', '011111000001100000101110100111000', '011101011100010111010011001110101', '001000010111111111010010001110011', '111010110110000000010100110111100', '101011110110011111111000110111101', '000111110001100101100001110000001', '010100111001100110111000111001110', '110011110100010101101011110011101', '111000001100101111111010110011110', '111111111011001110100110010111000', '100000101110011110100000111011011', '101110110100011110111000001010101', '101110111100110010001100011110001', '101110101100100111110011101110011', '100000110011011101011001101101010', '111111111100010101100110011001101'], 'text': 'https://nameswallet.app/bridge/#logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty'}, 'compare': {'first': 'a50e0f', 'last': 'b7ad2e', 'text': 'Compare with the deposit page: its commitment starts a50e0f and ends b7ad2e.'}, 'not_private': {'title': 'Not private', 'text': "A public claim publishes the claim secret and links the Sepolia deposit to this account. Claiming into a public account publishes the deposit's claim secret on LEZ and links your Sepolia deposit to Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk for good. A private account keeps them apart (its claim needs a proof)."}, 'steps': [], 'privacy': {}, 'export_text': 'LEZ bridge deposit request'}}},
    'receive-code-details': {'bridge': {'make_public_deposit_request': {'deposit': {'id': 'dep-a50e0f44940e', 'symbol': 'ETH', 'account': 'CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'account_kind': 'public'}, 'symbol': 'ETH', 'denom_text': '0.1', 'total_text': '0.1003', 'bridge_fee': {'desk': '0x6b7F38D0…DBc5', 'fee_bps': 30, 'percent_text': '0.3%', 'amount_text': '0.0003', 'total_text': '0.1003', 'text': 'Bridge fee 0.0003 ETH (0.3% of 0.1 ETH); you send 0.1003 ETH in all.'}, 'commitment': '0xa50e0f44940eca30735460fa9b657d6ab948fc422b29b1293be0f7e5d6b7ad2e', 'request': 'logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty', 'request_url': 'https://nameswallet.app/bridge/#logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty', 'request_qr': {'size': 33, 'rows': ['111111111010110111110110001111111', '100000110101111010010111011000001', '101110110110010000101101111011101', '101110100001000100101000011011101', '101110110111110111011000011011101', '100000111001111101111000001000001', '111111101000011101111111111111111', '110011111001111110110010100001110', '001100000010111110010001111100010', '010000110100110011100100010110000', '100111101110110010000111101100100', '010110000010000101000010001111110', '001001001011001101110101110010001', '110111001100011011000101011001101', '111111010111110011010110100010001', '111101110001111111000101111101011', '111111000000110100100000100000000', '011111000001100000101110100111000', '011101011100010111010011001110101', '001000010111111111010010001110011', '111010110110000000010100110111100', '101011110110011111111000110111101', '000111110001100101100001110000001', '010100111001100110111000111001110', '110011110100010101101011110011101', '111000001100101111111010110011110', '111111111011001110100110010111000', '100000101110011110100000111011011', '101110110100011110111000001010101', '101110111100110010001100011110001', '101110101100100111110011101110011', '100000110011011101011001101101010', '111111111100010101100110011001101'], 'text': 'https://nameswallet.app/bridge/#logosdep1qyqqqqqqqqrz6jhxk59t8g5c2ar9nps3arxrjzy7yhqgu02kmqzszqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzz5zt7u43kzcnyw3ah4f7ylu8y4ngr8eq2pz3jfr7mvdh5grkx9p3yel3q6msl9yy8ty'}, 'compare': {'first': 'a50e0f', 'last': 'b7ad2e', 'text': 'Compare with the deposit page: its commitment starts a50e0f and ends b7ad2e.'}, 'not_private': {'title': 'Not private', 'text': "A public claim publishes the claim secret and links the Sepolia deposit to this account. Claiming into a public account publishes the deposit's claim secret on LEZ and links your Sepolia deposit to Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk for good. A private account keeps them apart (its claim needs a proof)."}, 'steps': [], 'privacy': {}, 'export_text': 'LEZ bridge deposit request'}}},
    'card-receives': {'bridge': {'deposits': {'deposits': []}, 'receive_status': {'__seq': [{'stage': 'claimable'}, {'stage': 'deposited', 'confirmations': 12, 'final_in_min': 9}, {'stage': 'requested'}]}}},
    'card-receives-old': {'bridge': {}},
    'claim-arrived': {'bridge': {'relayed_claim_check_deposit': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    'claim-arrived-secret': {'bridge': {'relayed_claim_check_deposit': {'needs': 'secret', 'request': {'request': 'r5', 'state': 'pending'}, 'message': 'Approve "RFP022/deposit" in the Names app…'}, 'wallet_request': {'state': 'pending'}}},
    'backup-state': {},
    'backup-make': {},
    'backup-made': {'backup': {'backup_make': {'file': '/Users/you/Names Wallet Backups/names-wallet-backup-20261114-2350.lnsbackup', 'networks': ['lez-0.3', 'lez-0.4'], 'folders': [{'id': 'lez-0.3', 'folder': '/Users/you/.lee/wallet'}, {'id': 'lez-0.4', 'folder': '/Users/you/.config/logos-names/lez-0.4/wallet'}], 'created': 1794700200, 'check': {'opens': True, 'created': 1794700200, 'networks': [{'id': 'lez-0.3', 'matches': True, 'differs': [], 'accounts': [{'account': 'Public/9vXeP3mGQ2kz8YbT6nR4wHs1JcL5dA7uFqE2iZoKyBtN', 'kind': 'public', 'imported': False}, {'account': 'Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC', 'kind': 'private', 'imported': False}]}, {'id': 'lez-0.4', 'matches': True, 'differs': [], 'accounts': [{'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'kind': 'public', 'imported': True}, {'account': 'Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR', 'kind': 'public', 'imported': False}, {'account': None, 'kind': 'private', 'imported': True}]}]}}}},
    'backup-made-details': {'backup': {'backup_make': {'file': '/Users/you/Names Wallet Backups/names-wallet-backup-20261114-2350.lnsbackup', 'networks': ['lez-0.3', 'lez-0.4'], 'created': 1794700200, 'check': {'opens': True, 'created': 1794700200, 'networks': [{'id': 'lez-0.3', 'matches': True, 'differs': [], 'accounts': [{'account': 'Public/9vXeP3mGQ2kz8YbT6nR4wHs1JcL5dA7uFqE2iZoKyBtN', 'kind': 'public', 'imported': False}, {'account': 'Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC', 'kind': 'private', 'imported': False}]}, {'id': 'lez-0.4', 'matches': True, 'differs': [], 'accounts': [{'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'kind': 'public', 'imported': True}, {'account': 'Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR', 'kind': 'public', 'imported': False}, {'account': None, 'kind': 'private', 'imported': True}]}]}}}},
    'backup-restore': {},
    'backup-restore-open': {'backup': {'backup_open': {'created': 1794700200, 'networks': [{'id': 'lez-0.3', 'shown': {'version': 'LEZ 0.3'}, 'accounts': [{'account': 'Public/9vXeP3mGQ2kz8YbT6nR4wHs1JcL5dA7uFqE2iZoKyBtN', 'kind': 'public', 'imported': False}, {'account': 'Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC', 'kind': 'private', 'imported': False}], 'folder': '/Users/you/.config/logos-names/lez-0.3/wallet', 'has_wallet': True, 'can_restore': False}, {'id': 'lez-0.4', 'shown': {'version': 'LEZ 0.4'}, 'accounts': [{'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'kind': 'public', 'imported': True}, {'account': 'Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR', 'kind': 'public', 'imported': False}, {'account': None, 'kind': 'private', 'imported': True}], 'folder': '/Users/you/.config/logos-names/lez-0.4/wallet', 'has_wallet': False, 'can_restore': True}]}}},
    'backup-wrong-password': {'backup': {'backup_open': {'error': 'the password is wrong, or the file is damaged'}}},
    'backup-restored': {'backup': {'backup_restore': {'network': 'lez-0.4', 'folder': '/Users/you/.config/logos-names/lez-0.4/wallet', 'files': ['names.json', 'storage.json'], 'accounts': [{'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'kind': 'public', 'imported': True}, {'account': 'Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR', 'kind': 'public', 'imported': False}, {'account': None, 'kind': 'private', 'imported': True}]}}},
    'claim-approve-sheet': {'bridge': {'relayed_claim_check': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    'claim-notfinal': {'bridge': {'relayed_claim_check': {'ok': False, 'error': {'code': 'not_final', 'message': 'The deposit is in block 9,612,004; Sepolia finalizes it in about 6 minutes.', 'eta_s': 330}, 'network': 'LEZ 0.4 · bridge test network'}}},
    'claim-checkagain': {'bridge': {'relayed_claim_check': {'__seq': [{'ok': False, 'error': {'code': 'not_final', 'message': 'The deposit is in block 9,612,004; Sepolia finalizes it in about 6 minutes.', 'eta_s': 330}, 'network': 'LEZ 0.4 · bridge test network'}, {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}]}}},
    'claim-relaycap': {'bridge': {'relayed_claim_check': {'ok': False, 'error': {'code': 'relay_cap', 'message': 'The relay has sent 20 claims in the last 24 hours (DAILY_MAX).'}, 'network': 'LEZ 0.4 · bridge test network'}}},
    'claim-details': {'bridge': {'relayed_claim_check': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    'claim-install': {'packages': []},
    'claim-install-asking': {'packages': []},
    'claim-installing': {'packages': []},
    'claim-install-slow': {'packages': []},
    'claim-install-failed': {'packages': []},
    'claim-idle': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}]},
    'claim-starting': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'ee90c31ffa02781abf1beb8735acf2ad352af23b932a8e11a41912e31634c0a7'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}]},
    'claim-older': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': '788f2f0dde4bd7ae'}}], 'bridge': {}},
    'claim-updated': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': '788f2f0dde4bd7ae'}}], 'bridge': {}},
    'approve-checking-publisher': {},
    'wallet-starting': {},
    "claim-missing": {},
    "claim-start": {'bridge': {}},
    "claim-checking": {'bridge': {}},
    "claim-ready": {'bridge': {'relayed_claim_check': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    "claim-elsewhere": {'bridge': {'relayed_claim_check': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/9vXeP3mGQ2kz8YbT6nR4wHs1JcL5dA7uFqE2iZoKyBtN', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    "claim-wrong-bridge": {'bridge': {'relayed_claim_check': {'ok': True, 'id': 'rc-0123456789abcdef', 'network': 'LEZ 0.4 · bridge test network', 'wording': "The wallet signs the claim; the bridge's relay sends it and pays the fee.", 'privacy': 'A public claim publishes the claim secret and links the Sepolia deposit to this account.', 'deposit': {'amount_text': '0.1 ETH', 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'leaf_index': 7, 'leaves': 9}, 'relay': {'payer': 'Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq', 'max_fee': '2500000'}, 'call': {'program': 'JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7'}, 'checks': [{'key': 'file', 'ok': True, 'text': 'The claim file reads, and its secret matches its commitment'}, {'key': 'bridge', 'ok': True, 'text': 'It is for this bridge and its vault'}, {'key': 'deposit', 'ok': True, 'text': 'The deposit is in the vault, final on Sepolia'}, {'key': 'root', 'ok': True, 'text': 'Its root is recorded on the bridge test network'}, {'key': 'unclaimed', 'ok': True, 'text': "It hasn't been claimed"}, {'key': 'relay', 'ok': True, 'text': 'The relay pays the fee'}], 'wallet': {'needs': None}, 'approval': 'This app shows the call your wallet built and asks you to approve it.'}}},
    "claim-refused": {'bridge': {'relayed_claim_check': {'ok': False, 'error': {'code': 'already_claimed', 'message': 'This deposit was claimed in LEZ block 5249.'}, 'network': 'LEZ 0.4 · bridge test network'}}},
    "claim-needs": {'bridge': {'wallet_request': {'state': 'pending'}}},
    "claim-sheet": {'bridge': {'wallet_request': {'state': 'pending'}}},
    "claim-uncertain": {'bridge': {}},
    "claim-busy": {'bridge': {}},
    "claim-noanswer": {'bridge': {}},
    "claim-running": {'bridge': {'relayed_claims': [{'id': 'rc-job-1', 'state': 'running', 'phase': 'relay', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': 'The relay is sending it.', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': None}], 'relayed_claim_status': {'id': 'rc-job-1', 'state': 'running', 'phase': 'relay', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': 'The relay is sending it.', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': None}}},
    "claim-done": {'bridge': {'relayed_claims': [{'id': 'rc-job-1', 'state': 'done', 'phase': 'done', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': 'Finished.', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': None}], 'relayed_claim_status': {'id': 'rc-job-1', 'state': 'done', 'phase': 'done', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': 'Finished.', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': None}}},
    "claim-failed": {'bridge': {'relayed_claims': [{'id': 'rc-job-1', 'state': 'running', 'phase': 'relay', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': '', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': {'code': 'relay_refused', 'message': 'the relay refused it: its nonce moved'}}], 'relayed_claim_status': {'id': 'rc-job-1', 'state': 'failed', 'phase': 'relay', 'can_cancel': False, 'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'block': 5249, 'note': '', 'result': {'balance_text': '0.1 ETH', 'native': '5688340'}, 'error': {'code': 'relay_refused', 'message': 'the relay refused it: its nonce moved'}}}},
    "settings-zones": {"zones": zones("home")},
    "settings-zones-add": {"zones": zones("home")},
    "settings-zones-remove": {"zones": zones("home")},
    "settings-zones-other-chain": {"zones": zones("home"), "otherChain": True},
    "wallet-zone": {"zones": zones(ZONE_B), "zone": ON_B},
    "wallet-zone-nowallet": {"zones": zones(ZONE_B, False), "zone": ON_B, "balance": None, "status": {"wallet": False, "public": None, "private": None}},
    "wallet-zone-names": {"zones": zones(ZONE_B), "zone": ON_B},
    "wallet-zone-activity": {"zones": zones(ZONE_B), "zone": ON_B, "activity": ZONE_ACTIVITY},
    "wallet-zone-review": {"zones": zones(ZONE_B), "zone": ON_B},
    "approve-zone": {"zones": zones(ZONE_B), "zone": ON_B},
    "wallet-bridged": {"bridged": [{"token_program": "4VScJ7mtwaanQ5gcSZZw2iAMHYAbLrVmbYKwEGx2eSex", "app": "lez_bridge", "updating": False, "error": None, "holdings": [{"account": "Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC", "account_kind": "private", "definition": "FBqNAwJs9T4eNDw9pQXxXrLZNcnR1sTnnBEYzhudLQZJ", "balance": "100000000", "name": "Wrapped dUSDC", "symbol": "dUSDC", "decimals": 6, "origin": "Sepolia 0xAc20…a71e"}]}]},
    "approve-other-zone": {"zones": zones("home")},
    "approve-removed-zone": {"zones": zones("home")},
    "welcome-install": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-terminal": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-installing": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-install-failed": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None,
                               "installJob": {"id": "-install-1792000000000", "name": "-install", "kind": "install", "state": "failed", "exit": 6,
                                              "started": 1792000000000, "finished": 1792000004000, "log_tail": "Downloading names for macos-arm64...\ncurl: (6) Could not resolve host: github.com",
                                              "problem": "Couldn't reach the internet. Check your connection, then try again."}},
    "welcome-wallet": NO_WALLET,
    "names-reveal-countdown": {"busy": ["pending_requests", "pending_inscriptions"],
                               "pending": [{"name": "m2-test-111", "display": "m2-test-111.lez", "owner": "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk",
                                            "stage": "committed", "ready_in": 7, "by_in": 1428, "tx": None}]},
    "welcome-outdated-03": {"status": {"outdated": True, "cli_line": "names 0.1.0", "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-net04-nocli": {"noNames": True, "status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-net04-funding": {"noNames": True, "private": "0", "public": "0"},
    "noname-net04-wallet": {"noNames": True},
    "noname-net04-request": {"noNames": True},
    "nowallet-net04-welcome": {"noNames": True, "status": {"wallet": False, "public": None, "private": None}, "balance": None},
    "nowallet-net04-request": {"noNames": True, "status": {"wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-phrase": NO_WALLET,
    "welcome-wallet-error": NO_WALLET,
    "get-tokens-nowallet": NO_WALLET,
    "welcome-unfunded": {"private": "0", "public": "0"},
    "welcome-faucet": {"private": "0", "public": "0"},
    "welcome-faucet-error": {"private": "0", "public": "0", "faucetError": "this account was funded in the last day"},
    "welcome-funded": {"private": "0", "public": "6000000"},
    "get-tokens-move": {"private": "0", "public": "6000000"},
    "welcome-moving": {"private": "0", "public": "6000000"},
    "welcome-move-poor": {"private": "0", "public": "6000000",
                          "walletJob": {"id": "-wallet-1792000000001", "name": "-wallet", "kind": "shield", "state": "done", "exit": 0, "started": 1792000000000, "finished": 1792000003000,
                                        "log_tail": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk holds 1200; send it at least 5000000 testnet tokens", "moved": False,
                                        "problem": "Your public account holds 1,200, and moving to private needs at least 5,000,000. Get more tokens, then try again."}},
    "welcome-moved": {"private": "5000000", "public": "1000000"},
    "pay-account": {"private": "250000000", "public": "1250000"},
    "wallet-down": {"nodeDown": True},
    # The owner's wallet as the real-setup test read it: 14 public accounts, one holding something.
    "wallet-net04-many": {"bridgeNet": dict(NET04, not_read=0, read=dict(NET04_READ, accounts=[NET04_READ["accounts"][0]] + [
        {"account": "Public/" + a, "found": False, "native": "0", "tokens": net04_held("0", "0"), "other_tokens": 0} for a in
        ["WJhfAxHvoPvFGZ1DYFLvwZLETEGqWx3kbpz5jdoDVAb", "4FxHAXMqG45em3UZPYzVLEKGzWm2JNS1JSRZMSspFgtP", "6Fbtv64VQE1NciFYqmZdiNMSxud1sGLQPN1Y7iPsMipm", "74Xmu712RoXa9GuGcEox1S6cCseckVr1KVH8Uwfzy2F6",
         "8sWBKiKPR56CTc55yvD61Gnm9iKeaeRg2hEMjqvC6A2b", "9W9huNeAds11pdGpMijrdhTkSQ8upNudRnrZJbNNWfoW", "BHJQN5ePVJoLTkfHF6jk2Nwio3gJcqa4joNpdtphxfVB", "CCE8TqnKQavGSnqzwoUf9T3mfpNJmFtmnwb4252UiaY9",
         "CKBM6zaH6nhpYGM9WkBXKLpFhCtYVp5Z2ULy5mBFACjr", "CnhoAmXXnZeLS2Q7RGPwxnK5G2LYFyEzoLQibtKAycqE", "F3MsAVK7RXyktPT1hpHYgBpiLq8TfD9FA1dQTvEeaBb9"]]))},
    "wallet-net04-watch": {"bridgeNet": dict(NET04, watched=["Public/BVDcg8SbSdCTKzx4mJbSpVgEuaoPEMhxmmRwvVCGyBnD"], read=dict(NET04_READ, accounts=NET04_READ["accounts"] + [
        {"account": "Public/BVDcg8SbSdCTKzx4mJbSpVgEuaoPEMhxmmRwvVCGyBnD", "found": True, "native": "99799912984428", "tokens": net04_held("0", "0"), "other_tokens": 0, "watched": True}]))},
    "wallet-net04-down": {"bridgeNet": dict(NET04, error="can't reach the bridge test network: it answered HTTP 502 instead of a reading")},
    "wallet-net04-never": {"bridgeNet": dict(NET04, read=None, error="can't reach the bridge test network: timed out")},
    "wallet-net04-reset": {"bridgeNet": dict(NET04, read=dict(NET04_READ, height=12, reset_at=1791999000,
                           accounts=[dict(NET04_READ["accounts"][0], native="0", tokens=net04_held("0", "0"), found=False), NET04_READ["accounts"][1]]))},
    "wallet-net04-activity": {"activity": [
        {"id": "n1", "at": 1792000000, "kind": "arrived", "chain": "lez-0.4", "zone": "bridge-0.4", "network": "bridge-0.4", "network_name": "bridge test network",
         "to": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "amount": "100000000000000000", "token": "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw", "symbol": "wETH", "decimals": 18, "state": "arrived"},
        {"id": "n2", "at": 1791999000, "kind": "arrived", "chain": "lez-0.4", "zone": "bridge-0.4", "network": "bridge-0.4", "network_name": "bridge test network",
         "to": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "amount": "23786858", "state": "arrived"},
        {"id": "n3", "at": 1791990000, "kind": "pay", "chain": "lez", "name": "ada", "amount": "250000", "fee": "4088", "state": "done"}]},
    "wallet-chain-notready-page": {"notReady": True},
    "wallet-chain-ready-page": {"notReady": True},
    "wallet-claim": {"waiting": {"count": "2", "total": "4000000"}},
    "wallet-bridge-exact": {"exact": True},
    "wallet-bridge-nochannel": {"exact": True, "noChannel": True},
    "wallet-bridge-confirm": {"exact": True},
    "wallet-bridge-feeshort": {"exact": True, "feeShort": True},
    "settings-oldcli": {"status": {"outdated": True}, "accountsError": "The names tool set for this network is older than this app and can't do this. The rest of the app still works."},
    "wallet-waiting": {"tokenPending": [{"position": 21, "definition": "GoLDdef1nition11111111111111111111111111111", "name": "Gold", "amount": "50", "to": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"}]},
    "wallet-sending": {"private": "250000000", "public": "1250000"},
    "inscribe": {"inscription": {}},
    "inscribe-short": {"inscription": {"problem": "That key doesn't hold enough for this and its fee."}},
    "inscribe-nofigure": {"inscription": {"noFigure": True}},
    "inscribe-node-down": {"nodeDown": True, "inscription": {"node": "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app."}},
    "inscribe-fake": {"inscription": {"publisher": {"status": "mismatch", "publisher": "jef", "module": "testimonial_ui", "version": "0.2.0"},
                                      "text": "Line one of a longer testimonial.\nLine two, with a very long unbroken word: " + "x" * 300}},
    "inscribe-fake-chosen": {"inscription": {"publisher": {"status": "mismatch", "publisher": "jef", "module": "testimonial_ui", "version": "0.2.0"}}},
}

MOCKS["wallet-net04-many-open"] = MOCKS["wallet-net04-many"]

def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
    filled = {"zone_b": ZONE_B, "zone_request": zone_request(True), "zone_request_gone": zone_request(False, True)}
    source = open(os.path.join(HERE, "..", "qml", "Main.qml")).read()
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
            "mock": json.dumps(MOCKS.get(scenario, {})),
            "bridge_key": "" if scenario == "get-tokens" else "8e3b0f4a2c71d95e6a0b4c8d2f1e7a3954c6b0d8e2f4a6c8b0d2e4f6a8c0b2d4",
            "home_channel": HOME_CHANNEL,
            "bridge_net": json.dumps(NET04),
            "scenario": (SCENARIOS[scenario] % dict(filled, zone_request=zone_request(scenario == "approve-zone"))) if "%(zone" in SCENARIOS[scenario] else SCENARIOS[scenario], "out": out, "height": 2700 if scenario in ("settings", "release", "page-all", "cli-settings", "cli-saving", "cli-failed", "cli-nowallet", "profile-tab", "start", "settings-tab", "register", "register-net04", "wallet-names-net04", "send-net04", "name", "settings-accounts", "settings-oldcli", "settings-account-new", "settings-network") or scenario.startswith("settings-zones") or scenario.startswith("backup") else 2000 if scenario.startswith("approve-private") or scenario.startswith("approve-relayed") else 1100 if scenario.startswith("approve-") and "zone" in scenario else 1500 if scenario.startswith("get-tokens") or scenario.startswith("wallet") or scenario.startswith("claim") else 1700 if scenario.startswith("welcome") else 1000 if scenario.startswith("pay-account") else 1100 if scenario.startswith("inscribe") else 1300 if scenario in ("name", "profile", "source", "page-link", "page-text", "page-long", "nowallet") else 760,
        }
        # Beside Main.qml, so its relative imports (logic.mjs) resolve.
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=os.path.join(HERE, "..", "qml")) as handle:
            # A "-light" scenario reads the light palette directly. Never through Theme.settings:
            # that is design-system.ini, shared by every Logos app and render on the machine.
            themed = source.replace("Theme.palette", "Theme.lightTheme").replace("Theme.isDark", "false") if scenario.endswith("-light") else source
            handle.write(themed.replace(marker, marker + bridge, 1))
        try:
            result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=60)
        except subprocess.TimeoutExpired:
            # One scenario that never quits doesn't stop the rest (M-8).
            print(scenario, "FAILED", "  it didn't quit within 60 s", sep="\n")
            continue
        finally:
            os.unlink(handle.name)  # never leave it beside Main.qml: the package build copies qml/ whole
        problems = [line for line in result.stderr.splitlines() if "qt.qpa.fonts" not in line and line.strip()]
        print(scenario, "ok" if os.path.exists(out) else "FAILED", *problems[:6], sep="\n  " if problems else " ")

main()
