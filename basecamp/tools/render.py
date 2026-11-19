#!/usr/bin/env python3
"""Renders qml/Main.qml to PNG files without Basecamp, with a stand-in for the Logos bridge.

Usage: render.py <qml binary> <output dir> [scenario ...]
Needs QML_IMPORT_PATH and QT_PLUGIN_PATH set for a Qt 6 install, and
QT_QPA_PLATFORM=offscreen to run without a display.
"""
import json, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
# The build the shipped LEZ Bridge reports (logic.mjs BUNDLED_BRIDGE[0].build): a stand-in bridge that
# is meant to be the current one says it, so the app never sees an older bridge and holds Receive
# ("Update it first"). Read here, never copied: a copy went stale when Names shipped a new bridge.
import re as _re_build
SHIPPED_BUILD = _re_build.search(r'export const BUNDLED_BRIDGE = \[.*?build: "([0-9a-f]+)"', open(os.path.join(HERE, "..", "qml", "logic.mjs")).read(), _re_build.S).group(1)
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
                return JSON.stringify(pk ? [me].concat(pk) : [me, { name: "lez_bridge", version: "0.1.0", hashes: { root: "e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848" } }, { name: "lez_bridge_ui", version: "0.1.0", hashes: { root: "ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9" } }])
            }
            if (module === "names" && method === "file_sha256")
                return ok(String(args[0]).indexOf("lez_bridge_ui") >= 0 ? "0d7e317c1fbef3e9c9d4155c10746256af8e9ed2b5a8156413823fbc568ea270" : "0a57e700a2a8aa0f94e6722def280d330d6dc59f8cc94199e4f9946802f62da5")
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
                if (answer && answer.__error) return JSON.stringify(JSON.stringify({ success: false, value: null, error: answer.__error, code: answer.__code }))
                if (answer === undefined && method !== "config" && method !== "relayed_claims" && method !== "relayed_claim_forget" && method !== "receives") return JSON.stringify({ error: "Method not found" })
                if (answer === undefined) answer = method === "config" ? { build: { commit: "%(shipped_build)s" }, deployment: { name: "bridge-test-network-v5", bridge_program: "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA" }, features: ["relayed_claim", "relayed_claim_check_with", "relayed_claim_build", "relayed_claim_submit", "account_hint", "receive", "deposit_code", "public_receive", "receive_status", "receives", "receive_request", "relayed_claim_check_deposit", "send_back", "send_back_settle", "bridges"], notices: { release: (%(mock)s).release || null, not_private: { title: "Not private", text: "A public claim publishes the claim secret and links the Sepolia deposit to this account, for good. A private account keeps them apart (its claim needs a proof)." } } } : method === "relayed_claims" ? [] : method === "receives" ? { receives: [] } : null
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
            if (method === "get_config") return (function(c) { return wrapNet ? ok(c) : JSON.stringify(c) })({ features: ["reveal_name", "request_access_with", "file_sha256", "grant_add_account", "app_note", "test_lez_faucet"], bridge_programs: ["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr", "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf", "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA"], names_network: { id: root.mockNet || "lez-0.3" }, zone: (%(mock)s).zone || { id: "home", name: "Home", channel: "%(home_channel)s", home: true }, names_sequencer: "http://127.0.0.1:3040", registry: %(registry)s, sequencer: (%(mock)s).zone ? "http://127.0.0.1:3041" : "http://127.0.0.1:3040", wallet_config: "/Users/you/wallet/wallet_config.json", wallet_storage: "/Users/you/wallet/storage.json", cli: "/Users/you/.local/bin/names", cli_wallet: "/Users/you/.lee/wallet", bridge_key: "%(bridge_key)s", bridge_node: null, module_version: root.neededModule })
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
            // The LEZ 0.4 test-LEZ faucet (faucet_info / faucet_claim), when a scenario gives it.
            if (method === "faucet_info" && mock.faucet) return ok(mock.faucet.info)
            if (method === "faucet_claim" && mock.faucet) return ok(mock.faucet.claim)
            // The wallet adds the bridges it ships (Logic.bridgesToAdd); the module answers true.
            if (method === "set_bridge_programs") return ok(true)
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
            // Collectibles (NFTs, nft.rs): stand-ins from the scenario's mock.nft; nothing is sent.
            if (method === "pending_nft_requests") return ok(mock.nft ? mock.nft.requests || [] : [])
            if (method === "membership_passes") return ok(mock.nft ? mock.nft.passes || [] : [])
            if (method === "membership_proof_sheet") return mock.nft && mock.nft.proofSheet ? ok(mock.nft.proofSheet) : JSON.stringify({ success: false, value: null, error: "there is no such proof request" })
            if (/^nft_/.test(method)) {
                var nm = mock.nft || {}
                if (method === "nft_programs") return ok(nm.programs || [])
                if (method === "nft_holdings") return nm.readError ? JSON.stringify({ success: false, busy: true, value: null, error: "still reading your nfts; this updates in a moment" }) : ok({ programs: nm.read || [] })
                if (method === "nft_sheet") return JSON.stringify({ success: true, value: nm.sheet, error: null, digest: "ab".repeat(32), warnings: nm.warnings || [], program_label: nm.label || null })
                if (method === "nft_image") return ok(%(avatar)s)
                if (method === "nft_grants") return ok(nm.grants || [])
                if (method === "nft_receiving_keys") return ok({ account: args[0], keys: "ab".repeat(32) + ":" + "cd".repeat(33), reveals: "Whoever has these keys can send NFTs and payments to this private account, and can work out its account ID. They can't spend from it or see what it holds. Everything sent to these keys can be told apart as going to the same account, and the keys can be passed on." })
                return ok(null)
            }
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

# The LEZ 0.4 card with this wallet's receives (`receives`, `receiveStatuses`, then more JS), and
# the Receive screen the same way.
CARD_04 = 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.receivesLoaded = true; root.receives = %s; root.receiveStatuses = %s; %s }) })'
RECEIVE_04 = CARD_04
ARR = '[{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR" }]'
CHECKED = 'root.receiveCheckedAt = Date.now() - 3000; root.receiveNextAt = 0; root.againAfter(50, function() { root.receiveAskNow = false; root.sbAskNow = false })'
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
    # "Arriving from Ethereum": the card's own panel in the third tone, and Receive following one receive.
    'card-arriving-none': CARD_04 % ('[]', '({})', ''),
    'card-arriving-waiting': CARD_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 7 } })', CHECKED),
    'card-arriving-waiting-light': CARD_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 7 } })', CHECKED),
    'card-arriving-code': CARD_04 % (ARR, '({ "dep-1": { stage: "requested" } })', CHECKED),
    'card-arriving-arrived': CARD_04 % (ARR, '({ "dep-1": { stage: "claimable" } })', CHECKED),
    'card-arriving-update': CARD_04 % (ARR, '({ "dep-1": { stage: "claimable" } })', CHECKED + '; root.loadInstalled(root.checkBundle); root.bridgeCall("config", [], function(v) { root.bridgeAnswered(v) })'),
    'card-arriving-claimed': CARD_04 % (ARR, '({ "dep-1": { stage: "claimed", claimed_block: 6054 } })', CHECKED + '; root.receiveClaimedAt = { "dep-1": Date.now() - 60000 }'),
    'card-arriving-many': CARD_04 % ('[' + ", ".join('{ id: "dep-%d", symbol: "ETH", amount: "%s", account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR" }' % (i, a) for i, a in [(1, "0.1"), (2, "0.01"), (3, "0.01"), (4, "1"), (5, "0.1")]) + ']',
                                    '({ "dep-1": { stage: "final" }, "dep-2": { stage: "requested" }, "dep-3": { stage: "requested" }, "dep-4": { stage: "requested" }, "dep-5": { stage: "claimable" } })', CHECKED),
    'receive-deposited': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 14 } })', CHECKED + '; root.openEthReceive("dep-1")'),
    'receive-deposited-light': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 14 } })', CHECKED + '; root.openEthReceive("dep-1")'),
    'receive-final': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "final" } })', CHECKED + '; root.openEthReceive("dep-1")'),
    'receive-claimable': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "claimable" } })', CHECKED + '; root.openEthReceive("dep-1")'),
    'receive-claimed': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "claimed", claimed_block: 6054 } })', CHECKED + '; root.openEthReceive("dep-1")'),
    'receive-old-code': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "requested" } })', CHECKED + '; root.openEthReceive("dep-1")'),
    # A code of an earlier session shown again (receive_request), made for the old vault (v3).
    'receive-old-code-shown': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "requested" } })', CHECKED + '; root.openEthReceive("dep-1")'),
    # The bridge can't show it again (an older build kept no vault): one line, the reason under Details.
    'receive-old-code-refused': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "requested" } })', CHECKED + '; root.openEthReceive("dep-1"); root.receiveDetails = true'),
    # The bridge 0acf483: one receive on the older vault (labelled), one it can't follow, one arrived where the relay moved on.
    'card-arriving-0acf483': CARD_04 % ('[{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR" }, { id: "dep-2", symbol: "ETH", amount: "0.01", account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR" }, { id: "dep-3", symbol: "ETH", amount: "0.1", account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR" }]',
        '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 7, deployment: { id: "bridge-test-network-v3", label: "bridge v3", current: false } }, "dep-2": { stage: "unknown", action: "recover", text: "This deposit was made by an older build of the LEZ Bridge, which didn\'t keep which vault it went to. Recover finds it from your wallet." }, "dep-3": { stage: "claimable", relay: false, text: "Arrived: 0.1 ETH is ready to claim on bridge v3 (an older vault). The bridge\'s relay now serves bridge v4 only, so this claim pays its own fee, which the Names Wallet can\'t send yet. It stays claimable on bridge v3.", deployment: { id: "bridge-test-network-v3", label: "bridge v3", current: false } } })', CHECKED),
    'receive-noclaim': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "claimable", relay: false, text: "Arrived: 0.1 ETH is ready to claim on bridge v3 (an older vault). The bridge\'s relay now serves bridge v4 only, so this claim pays its own fee, which the Names Wallet can\'t send yet. It stays claimable on bridge v3.", deployment: { id: "bridge-test-network-v3", label: "bridge v3", current: false } } })', CHECKED + '; root.openEthReceive("dep-1")'),
    # An older bridge still runs: no new code until it is updated.
    'receive-hold-old-bridge': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
    # Opening Receive with a deposit on its way: that first, "Make another code" under it.
    'receive-entry-onitsway': RECEIVE_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 7 } })', CHECKED + '; root.openEthReceive()'),
    # The code just made, then the deposit seen: step 4 done by itself, step 5 the wait.
    'receive-code-then-deposited': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.receivesLoaded = true; root.openEthReceive(); root.receivePublicOk = true; root.againAfter(200, function() { root.makeReceive(); root.againAfter(150, function() { var all = root.receiveStatuses; all["dep-a50e0f44940e"] = { stage: "deposited", confirmations: 3, final_in_min: 14 }; root.receiveStatuses = all; root.receiveStatusesChanged(); root.receiveCheckedAt = Date.now() - 2000 }) }) }) })',
    # The deposit keys asked inside Receive: answered on the screen as its step, not in a sheet.
    'receive-needs-inline': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true; root.againAfter(300, function() { root.makeReceive(); root.againAfter(100, function() { root.appRequests = [{ id: "r5", kind: "secret", app: "lez_bridge", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", scope: "RFP022/deposit", created: Date.now(), expires: Date.now() + 600000, publisher: null, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", account_kind: "public", granted: true }] }]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"] }) }) }) })',
    # The same request while the person is elsewhere in Names: the sheet over everything.
    'receive-needs-elsewhere': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.receivePublicOk = true; root.againAfter(300, function() { root.makeReceive(); root.againAfter(100, function() { root.appRequests = [{ id: "r5", kind: "secret", app: "lez_bridge", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", scope: "RFP022/deposit", created: Date.now(), expires: Date.now() + 600000, publisher: null }]; root.walletView = "home" }) }) }) })',
    'card-receives-old': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.receivesLoaded = true; root.bridgeConfig = { deployment: {} }; root.receives = [{ id: "dep-1", symbol: "ETH", amount: "0.1", account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }] }) })',
    'claim-arrived': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.claimReceive("dep-1") }) })',
    'claim-arrived-secret': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.claimReceive("dep-1") }) })',
    'backup-state': 'root.section = "settings"; root.settingsOpen = true; ',
    # After an update: "What's new" at the top of Settings, three lines, Got it.
    'settings-whats-new': 'root.section = "settings"',
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
    'setup-card-stuck': 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.installedList = [{ name: "names_ui", installDir: "/x" }, { name: "lez_bridge", hashes: { root: "e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848" } }]; root.bridgeAnswers = false; root.bridgeFails = 2 }) })',
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
    'receive-restart-needed': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
    'card-restart-needed': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}], 'bridge': {'config': {'deployment': {'name': 'bridge-test-network-v3', 'bridge_program': '4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr'}}}},
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
    'backup-restore-open': {'backup': {'backup_open': {'created': 1794700200, 'networks': [{'id': 'lez-0.3', 'shown': {'version': 'LEZ 0.3'}, 'accounts': [{'account': 'Public/9vXeP3mGQ2kz8YbT6nR4wHs1JcL5dA7uFqE2iZoKyBtN', 'kind': 'public', 'imported': False}, {'account': 'Private/2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC', 'kind': 'private', 'imported': False}], 'folder': '/Users/you/.config/logos-names/lez-0.3/wallet', 'has_wallet': True, 'can_restore': False}, {'id': 'lez-0.4', 'settings': True, 'sequencer': 'https://testnet.lez.logos.co/', 'shown': {'version': 'LEZ 0.4'}, 'accounts': [{'account': 'Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk', 'kind': 'public', 'imported': True}, {'account': 'Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR', 'kind': 'public', 'imported': False}, {'account': None, 'kind': 'private', 'imported': True}], 'folder': '/Users/you/.config/logos-names/lez-0.4/wallet', 'has_wallet': False, 'can_restore': True}]}}},
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
    'claim-idle': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}]},
    'claim-starting': {'packages': [{'name': 'lez_bridge', 'version': '0.1.0', 'hashes': {'root': 'e6ababdad401a2924312ac8b5ef1e285497fadbcd6c565f639548aab2444e848'}}, {'name': 'lez_bridge_ui', 'version': '0.1.0', 'hashes': {'root': 'ee7c5d59ac3a91ecaedb525f0c887e41b6737da5aeaff4de3ea44fed661d68d9'}}]},
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
# The receive scenarios: the bridge answers each check with the state the scenario shows.
def _arriving(status, **more):
    return dict({"bridge": {"receive_status": status, "receives": {"receives": []}}}, **more)
for _name, _status in [("card-arriving-waiting", {"stage": "deposited", "confirmations": 20, "final_in_min": 7}), ("card-arriving-code", {"stage": "requested"}),
                       ("card-arriving-arrived", {"stage": "claimable"}), ("card-arriving-many", {"stage": "requested"}),
                       ("receive-deposited", {"stage": "deposited", "confirmations": 20, "final_in_min": 14}), ("receive-final", {"stage": "final"}),
                       ("receive-claimable", {"stage": "claimable"}), ("receive-claimed", {"stage": "claimed", "claimed_block": 6054}), ("receive-old-code", {"stage": "requested"}),
                       ("receive-entry-onitsway", {"stage": "deposited", "confirmations": 20, "final_in_min": 7})]:
    MOCKS[_name] = _arriving(_status)
MOCKS["card-arriving-waiting-light"] = MOCKS["card-arriving-waiting"]
MOCKS["receive-deposited-light"] = MOCKS["receive-deposited"]
MOCKS["card-arriving-none"] = {"bridge": {}}
MOCKS["receive-needs-inline"] = MOCKS["receive-needs"]
SCENARIOS["receive-code-words"] = SCENARIOS["receive-code"]
# The bridge 5cad169's code: the four words, large; the digits under Details.
import copy as _copy
MOCKS["receive-code-words"] = _copy.deepcopy(MOCKS["receive-code"])
MOCKS["receive-code-words"]["bridge"]["make_public_deposit_request"]["compare"] = {"first": "a50e0f", "last": "b7ad2e", "words": ["cream", "laugh", "load", "visual"], "words_text": "cream · laugh · load · visual", "text": "Compare with the deposit page: cream · laugh · load · visual."}
MOCKS["receive-needs-elsewhere"] = MOCKS["receive-needs"]
# An update ready for a bridge that still receives: Update is a plain button, Claim the yellow one.
MOCKS["card-arriving-update"] = dict(_arriving({"stage": "claimable"}), packages=[{"name": "lez_bridge", "version": "0.1.0", "hashes": {"root": "a2c6c3117443a92f2a5b2a1a2267175f4f82ed209a3dae8e554461bd09df0d18"}}, {"name": "lez_bridge_ui", "version": "0.1.0", "hashes": {"root": "788f2f0dde4bd7ae"}}])
MOCKS["card-arriving-update"]["bridge"]["config"] = {"build": {"commit": "02bfbd0"}, "deployment": {"name": "bridge-test-network-v3", "bridge_program": "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr"}, "features": ["relayed_claim", "relayed_claim_check_with", "receive", "deposit_code", "public_receive", "receive_status", "receives", "relayed_claim_check_deposit"]}
# Claimed a minute ago: the account it went into is shown, holding it.
MOCKS["card-arriving-claimed"] = dict(_arriving({"stage": "claimed", "claimed_block": 6054}), bridgeNet=dict(NET04, read=dict(NET04_READ, accounts=[NET04_READ["accounts"][0],
    dict(NET04_READ["accounts"][1], found=True, tokens=net04_held("0", "100000000000000000"))])))
MOCKS["receive-code-then-deposited"] = {"bridge": dict(MOCKS["receive-code"]["bridge"], receive_status={"stage": "deposited", "confirmations": 3, "final_in_min": 14}, receives={"receives": []})}


# After Copy the link: "Copied." on the button for a moment.
SCENARIOS["receive-code-copied"] = SCENARIOS["receive-code"].replace("root.makeReceive() })", "root.makeReceive(); root.againAfter(100, function() { root.receiveLinkCopied = true }) })")
MOCKS["receive-code-copied"] = MOCKS["receive-code-words"]
MOCKS["receive-code-details"] = _copy.deepcopy(MOCKS["receive-code-words"])
# v4's notice as the bridge 652ee7a gives it (rfp-022 v4-live deployments.json), under Details.
MOCKS["receive-code-details"]["release"] = {"text": "Withdrawals back to Ethereum are checked by proof on Ethereum. The first proofs are being set up; until then a burn waits.", "detail": ""}
_shown = MOCKS["receive-code-words"]["bridge"]["make_public_deposit_request"]
MOCKS["receive-old-code-shown"] = _arriving({"stage": "requested"})
MOCKS["receive-old-code-shown"]["bridge"]["receive_request"] = {"id": "dep-1", "request": _shown["request"], "link": _shown["request_url"], "request_url": _shown["request_url"], "request_qr": _shown["request_qr"],
    "compare": _shown["compare"], "commitment": _shown["commitment"], "symbol": "ETH", "denom": "100000000000000000", "denom_text": "0.1", "amount_text": "0.1 ETH", "denom_index": 1,
    "chain_id": 11155111, "vault": "0x26af59855b99a8af199361289A5Ed6088729E60E", "deployment": "bridge-test-network-v3", "network": "LEZ 0.4 · bridge test network",
    "account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "account_kind": "public", "claimed": False, "stage": "requested", "text": None, "as_of": None}
MOCKS["receive-old-code-refused"] = _arriving({"stage": "requested"})
MOCKS["receive-old-code-refused"]["bridge"]["receive_request"] = {"__error": "The deposit code of dep-1 can't be shown again: it was made by an older build of this module, which did not keep the vault it was for. Nothing was guessed; the code is in the reply that first made it (its export text), or make a new deposit request."}
MOCKS["card-arriving-0acf483"] = {"bridge": {"receives": {"receives": []}, "receive_status": {"__error": "busy"}}}
MOCKS["receive-noclaim"] = {"bridge": {"receives": {"receives": []}, "receive_status": {"stage": "claimable", "relay": False, "text": "Arrived: 0.1 ETH is ready to claim on bridge v3 (an older vault). The bridge's relay now serves bridge v4 only, so this claim pays its own fee, which the Names Wallet can't send yet. It stays claimable on bridge v3.", "deployment": {"id": "bridge-test-network-v3", "label": "bridge v3", "current": False}}, "receive_request": {"__error": "no"}}}
# 15 Nov: the programs step when two accounts are shared with the bridge. Nothing ticked: the
# accounts on the short sheet (no dead Approve); with the claim's account: it is ticked.
_PROGS = ('{"id": "r9", "kind": "programs", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "programs": ["BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf", "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7"], '
          '"sheet": {"lines": [], "warnings": ["The Program Registry isn\'t installed, so who wrote this program and whether its source is verified is unknown."]}, '
          '"choices": [{"account": "Public/BawxwquWtPEVcrwSFftD6jS3XFxZNv4qYrV8pkyo9mu7", "account_kind": "public", "label": "Names wallet, public account", "granted": true}, '
          '{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "account_kind": "public", "label": "Names wallet, public account", "granted": true}], "publisher": null}')
SCENARIOS["claim-programs-choose"] = SCENARIOS["claim-approve-sheet"].split("root.appRequests = ")[0] + "root.appRequests = [" + _PROGS + "]; root.appChosen = []; root.appNeedsChoice = true }) }) })"
SCENARIOS["claim-programs-ticked"] = SCENARIOS["claim-approve-sheet"].split("root.appRequests = ")[0] + "root.appRequests = [" + _PROGS + "]; root.appChosen = [\"Public/BawxwquWtPEVcrwSFftD6jS3XFxZNv4qYrV8pkyo9mu7\"]; root.appNeedsChoice = false; root.appUnderstood = true }) }) })"
MOCKS["claim-programs-choose"] = MOCKS["claim-approve-sheet"]
MOCKS["claim-programs-ticked"] = MOCKS["claim-approve-sheet"]
# Register → Check while the wallet is busy: "Checking…", the amber line under it.
SCENARIOS["register-check-busy"] = 'root.openRegister(); Qt.callLater(function() { registerField.text = "jef04"; root.checkRegister("jef04") })'
MOCKS["register-check-busy"] = {"busy": ["normalize", "resolve"]}
MOCKS["receive-hold-old-bridge"] = {"bridge": {"config": {"build": {"commit": "5cad169"}, "deployment": {"name": "bridge-test-network-v3", "bridge_program": "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr"},
    "features": ["relayed_claim", "relayed_claim_check_with", "receive", "deposit_code", "public_receive", "receive_status", "receives", "relayed_claim_check_deposit"]}},
    "packages": [{"name": "lez_bridge", "version": "0.1.0", "hashes": {"root": "d55b06acb191406f1a785d7606abc949b3552c7bd5973882f2089846f65cd6b7"}}]}

# ── Send back to Ethereum (rfp-022 modules/NAMES-SEND-BACK.md) ──
# The card on LEZ 0.4 with 0.15 wrapped ETH on bridge v4 in the owner's account; the bridge
# module's send_back_* answers as its sendback.rs gives them.
V4 = "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf"
OWNER = "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"
_v4_tokens = [{"definition": "92PJXZ2E3QPupUFuhc5f4rgsRiAZKv62TpSYfZEgLeru", "eth_token": "0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e", "symbol": "dUSDC", "decimals": 6, "name": "Wrapped dUSDC (demo v4)", "bridge": V4, "bridge_label": "bridge v4"},
              {"definition": "9tcLmjKixcGZJMnXS381Dhuzd8u9jPvQYF21F3vDhGE2", "eth_token": "0x0000000000000000000000000000000000000000", "symbol": "wETH", "decimals": 18, "name": "Wrapped Sepolia ETH (demo v4)", "bridge": V4, "bridge_label": "bridge v4"}]
NET04_V4 = dict(NET04, read=dict(NET04_READ, tokens=_v4_tokens, bridges=[{"program": V4, "label": "bridge v4", "bridge": True, "token_program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7"}],
    accounts=[{"account": OWNER, "found": True, "native": "0", "tokens": [dict(_v4_tokens[0], amount="0"), dict(_v4_tokens[1], amount="150000000000000000")], "other_tokens": 0},
              {"account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "found": False, "native": "0", "tokens": [dict(_v4_tokens[0], amount="0"), dict(_v4_tokens[1], amount="0")], "other_tokens": 0}]))
SB_FEATURES = ["relayed_claim", "relayed_claim_check_with", "relayed_claim_build", "relayed_claim_submit", "account_hint", "receive", "deposit_code", "public_receive", "receive_status", "receives", "receive_request", "relayed_claim_check_deposit", "send_back"]
SB_CONFIG = {"build": {"commit": SHIPPED_BUILD}, "deployment": {"name": "bridge-test-network-v4", "bridge_program": V4}, "features": SB_FEATURES, "notices": {}}
SB_LINK = {"link": "https://nameswallet.app/bridge/#withdraw", "deployment": "bridge-test-network-v4", "vault": "0xC2c6A059eAB928966B24FA5549d441A3B17B54Ed", "chain_id": 11155111, "available": True, "why": None}
SB_CODE = "logoswd1qyqqqqqqqqqzr7dm2wcgqnmxn3uq7c2yupx7qfguwl6zcw0d2ej8zhwpsa8x4a5xg9ws3xcqp"
SB_RECIPIENT = "0xE2C4857976b346F8F9b3a7be714Cd92d0bF68680"
SB_CHECK = {"ok": True, "recipient": SB_RECIPIENT, "recipient_short": "0xE2C4…8680", "words": ["maple", "tiger", "seven", "lamp"], "words_text": "maple · tiger · seven · lamp", "fee_choice": 0, "chain_id": 11155111, "vault": "0xC2c6A059eAB928966B24FA5549d441A3B17B54Ed", "warnings": []}
def sb_view(stage, **more):
    texts = {"planned": "Ready to send back: approve the burn in your Names Wallet.", "approve": "Waiting for you in the Names app.", "burning": "Sent; waiting for its block on the bridge test network (about 15 s).",
             "burned": "Burned. Waiting for Ethereum to record it: the bridge operator posts updates by hand today.", "recorded": "Recorded on Ethereum. Make the release proof (about 20 s on this computer).",
             "proving": "Making the release proof on this computer (about 20 s).", "proved": "Proof made. Waiting for its Ethereum wrapper (about 3 min, by the bridge operator today).",
             "wrapped": "Proof ready. Waiting for the vault's one-hour delay.", "releasable": "Ready: copy the release code into the bridge page and press Release.", "released": "Released on Ethereum."}
    return dict({"id": "sb-e2ba7765d0fbf247", "stage": stage, "text": texts.get(stage, ""), "account": OWNER, "symbol": "ETH", "amount_text": "0.01 ETH", "receive_text": "0.00995 ETH", "relay_fee_text": "0 ETH",
                 "recipient": SB_RECIPIENT, "words_text": "maple · tiger · seven · lamp", "exit_commitment": "0xe2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5",
                 "burn_tx": None if stage in ("planned", "approve") else "9f2c" * 16, "burn_block": None, "leaf_index": None, "root": None, "releasable_at": None, "seal": False, "released_tx": None,
                 "fee_payer": "The bridge's relay pays the LEZ fee: your account needs no LEZ.", "privacy": "A public burn publishes this account, the amount and an exit commitment; the Ethereum address stays hidden until the release.",
                 "wait": None}, **more)
def sb_mock(status=None, **bridge):
    b = {"config": SB_CONFIG, "send_back_link": SB_LINK, "send_back_check": SB_CHECK, "send_backs": {"send_backs": [sb_view(status["stage"])] if status else []}, "receives": {"receives": []}}
    if status:
        b["send_back_status"] = status
    b.update(bridge)
    return {"bridge": b, "bridgeNet": NET04_V4}
SB_CARD = 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.bridgeConfig = %s; root.loadBridgeNet(); %s }) })' % (json.dumps(SB_CONFIG), "%s")
SB_ASSET = '{ symbol: "wETH", name: "Wrapped Sepolia ETH (demo v4)", amount: "0.15", bridge: "%s", base: "150000000000000000", decimals: 18 }' % V4
SB_OPEN = SB_CARD % ('root.openSendBack("%s", %s)' % (OWNER, SB_ASSET) + '; %s')
# From the card's button: the accounts read from the bridge test network (NET04_V4: one holds v4 wETH).
SB_BUTTON = SB_CARD % 'root.againAfter(150, function() { root.openSendBack("", null) })'

_SB_FOCUS = SB_CARD % ('root.sendBacksLoaded = true; root.sendBacks = [{ id: "sb-e2ba7765d0fbf247", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "maple · tiger · seven · lamp", stage: "burned" }]; root.openSendBack("", null, "sb-e2ba7765d0fbf247"); root.sbStatuses = { "sb-e2ba7765d0fbf247": @VIEW@ }; root.sbCheckedAt = Date.now() - 4000; root.againAfter(50, function() { root.sbAskNow = false; root.receiveAskNow = false; root.sbScheduleNext() }); %s' % (OWNER, SB_RECIPIENT, "%s"))
def sb_focus(view, js=""):
    return _SB_FOCUS.replace("@VIEW@", json.dumps(view).replace('"@SOON@"', "Math.floor(Date.now() / 1000) + 1500").replace('"@HOUR@"', "Math.floor(Date.now() / 1000) + 3400")) % js
SB_BURN_SHEET = ('root.appRequests = [{"id": "r9", "kind": "relayed_call", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "%s", "program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "check": "ready", "message_len": 145, '
                 '"app_summary": "Send back 0.01 ETH to %s on Ethereum (exit 0xe2ba7765…). The bridge\'s relay sends it and pays the fee.", '
                 '"sheet": {"lines": ["lez_bridge asks your wallet to sign a burn that the bridge\'s relay pays for.", "Network: LEZ 0.4 · bridge test network", "Burns: 0.01 wETH (bridge v4) from %s, your account, to send it back to Ethereum.", '
                 '"Program: the bridge\'s token program 5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7; it tells bridge v4 (%s) what was burned.", "Exit commitment: 0xe2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5 (it hides the Ethereum address until the release).", '
                 '"Fee: paid by Public/6zkdhN9yAmzxkPrJ2Co92QAD7EKESM1AQcmoNEQ3KQpq, not by you: at most 2,500,000 (gas limit 120,000, tip 0).", "Exactly these bytes are signed: 5f1c0e77c2a1b6f0a8d3e94b7c2d10f3e6a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5 (the message is 145 bytes, SHA-256 9b1d…)."], '
                 '"warnings": ["This destroys the wrapped tokens on Logos and can\'t be undone; the ETH is released on Ethereum after the bridge\'s steps."], '
                 '"privacy": "A public burn publishes this account, the amount and an exit commitment; the Ethereum address stays hidden until the release.", "after": "", "hash": "5f1c", '
                 '"burn": {"amount_text": "0.01", "symbol": "wETH", "bridge_label": "bridge v4", "exit": "0xe2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5"}, "refused": false, "why": []}, '
                 '"publisher": null, "choices": [{"account": "%s", "account_kind": "public", "label": "Names wallet, main public account", "granted": true}]}]; root.appChosen = ["%s"]') % (OWNER, SB_RECIPIENT, OWNER, V4, OWNER, OWNER)
for _name, _js, _mock in [
    ("card-sendback", SB_CARD % 'root.sendBacksLoaded = true; root.sendBacks = [{ id: "sb-e2ba7765d0fbf247", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "", stage: "burned" }]; root.sbStatuses = { "sb-e2ba7765d0fbf247": { stage: "burned" } }' % (OWNER, SB_RECIPIENT), sb_mock(sb_view("burned"))),
    # Opening pre-selects the smallest amount (step 2); the amounts are step 1, after "Change".
    ("sendback-amount", SB_OPEN % 'root.againAfter(300, function() { root.sbDenom = -1 })', sb_mock()),
    ("sendback-link", SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0 })', sb_mock()),
    ("sendback-words", SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode() })' % SB_CODE, sb_mock()),
    ("sendback-bad-code", SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode() })' % SB_CODE, sb_mock(send_back_check={"ok": False, "error": {"code": "other_vault", "message": "This code is for another bridge vault: open the bridge page from this wallet's link again."}})),
    ("sendback-approve", SB_OPEN % 'root.againAfter(60, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode(); root.againAfter(60, function() { root.sbTicked = true; root.sbPlan(); root.againAfter(60, function() { %s }) }) })' % (SB_CODE, SB_BURN_SHEET),
        sb_mock(send_back_plan=sb_view("planned"), send_back_burn=sb_view("approve"))),
    ("sendback-names-update", SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode(); root.againAfter(200, function() { root.sbTicked = true; root.sbPlan() }) })' % SB_CODE,
        sb_mock(send_back_plan=sb_view("planned"), send_back_burn={"needs": "names_update", "request": None, "message": "x"})),
    ("sendback-burned", sb_focus(sb_view("burned")), sb_mock(sb_view("burned"))),
    # v7 (seq-sig gate): the LEZ Bridge's `update`/`text`/`wait` while the staged update's contest
    # window runs (rfp-022 sendback.rs gate_wait_text, word for word); stage stays `burned`.
    ("sendback-burned-v7-window", sb_focus(sb_view("burned", update={"window_s": 600, "pending": True, "finalizable_at": "@SOON@", "block_id": 28000, "covers": True, "dead": False},
        text="Burned. Waiting for the 10-minute contest window: Ethereum has the update that records this burn, and it can be applied when the window ends.",
        wait={"what": "the 10-minute contest window", "until": "@SOON@", "seconds": 1500})), sb_mock(sb_view("burned"))),
    ("sendback-recorded", sb_focus(sb_view("recorded", releasable_at="@HOUR@")), sb_mock(sb_view("recorded"))),
    ("sendback-witness", sb_focus(sb_view("recorded", releasable_at="@HOUR@"), 'root.againAfter(50, function() { root.sbProve() })'), sb_mock(sb_view("recorded"), send_back_prove={"needs": "prover", "request": None, "message": "x"},
        send_back_witness={"id": "sb-e2ba7765d0fbf247", "command": ["/Volumes/exos162/logos/rfp-022/public-demo/prove/prove-release-rc3.sh", "--secret-file", "/Users/you/Library/Application Support/lez_bridge/send-back/sb-e2ba7765d0fbf247/k", "--inputs", "…/inputs.json", "--leaves", "…/leaves.txt"],
                           "note": "The file k holds this exit's secret (mode 600). It can only release to the recipient fixed in the burn; it shows which burn pays which address. Delete it after the proof."})),
    ("sendback-proved", sb_focus(sb_view("proved", releasable_at="@HOUR@")), sb_mock(sb_view("proved"))),
    ("sendback-wrapped", sb_focus(sb_view("wrapped", seal=True, releasable_at="@SOON@")), sb_mock(sb_view("wrapped"))),
    ("sendback-releasable", sb_focus(sb_view("releasable", seal=True)), sb_mock(sb_view("releasable", seal=True))),
    ("sendback-released", sb_focus(sb_view("released", seal=True, released_tx="0x" + "7a" * 32)), sb_mock(sb_view("released", seal=True, released_tx="0x" + "7a" * 32))),
    ("sendback-burning", sb_focus(sb_view("burning")), sb_mock(sb_view("burning"))),
    ("sendback-proving", sb_focus(sb_view("proving", releasable_at="@HOUR@")), sb_mock(sb_view("proving"))),
    ("sendback-burn-unclear", sb_focus(sb_view("burn_unclear", text="Included in a block, but the account's balance did not go down: it may have been refused. Look in the Names app.")), sb_mock(sb_view("burn_unclear"))),
    ("sendback-relay-refused", sb_focus(sb_view("approve", note="The bridge's relay refused the burn: the relay's daily limit for burns is reached. Nothing was sent.")), sb_mock(sb_view("approve"))),
    ("sendback-exit-keys", SB_OPEN % ('root.againAfter(60, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode(); root.againAfter(60, function() { root.sbTicked = true; root.sbPlan(); root.againAfter(60, function() { '
        'root.appRequests = [{ id: "r5", kind: "secret", app: "lez_bridge", account: "%s", scope: "RFP022/exit", explanation: "lez_bridge asks for keys of its own, made from this account key: the wallet can make the same ones again after a restore, and they cannot sign or spend", created: Date.now(), expires: Date.now() + 600000, publisher: null, choices: [{ account: "%s", account_kind: "public", label: "Names wallet, main public account", granted: true }] }]; root.appChosen = ["%s"] }) }) })') % (SB_CODE, OWNER, OWNER, OWNER),
        sb_mock(send_back_plan={"needs": "secret", "request": {"request": "r5", "state": "pending"}, "message": "Approve \"RFP022/exit\" in the Names app: your wallet makes this app's exit secrets from your account, so a send-back can be released after you restore the wallet."})),
    ("sendback-busy", sb_focus(sb_view("burned", busy=True, note="The bridge's endpoint is busy; trying again shortly.")), sb_mock(sb_view("burned"))),
    ("sendback-from-button", SB_BUTTON, sb_mock()),
    ("sendback-none", SB_CARD % 'root.againAfter(150, function() { root.openSendBack("", null) })', dict(sb_mock(), bridgeNet=NET04)),
    ("sendback-checking", sb_focus(sb_view("burned"), 'root.againAfter(120, function() { root.sbAskNow = true })'), sb_mock(sb_view("burned"))),
    ("sendback-unavailable", SB_OPEN % '', sb_mock(send_back_link=dict(SB_LINK, available=False, why="this bridge's vault can't release (bridge v3); send back works on bridge v4"))),
]:
    SCENARIOS[_name] = _js
    MOCKS[_name] = _mock

# A decoded burn that is NOT this app's own send-back: another app's (to bridge v7 here), and the
# LEZ Bridge's while Names is on its wallet page. Each needs the tick and shows where it goes.
SB_BURN_SHEET_BRIDGE = SB_BURN_SHEET.replace('"bridge_label": "bridge v4"', '"bridge": "%s", "bridge_label": "bridge v4"' % V4)
SB_BURN_SHEET_OTHER = (SB_BURN_SHEET_BRIDGE.replace('"app": "lez_bridge"', '"app": "other_app"').replace("lez_bridge asks your wallet", "other_app asks your wallet")
                       .replace("bridge v4", "bridge v7").replace(V4, "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93"))
for _name, _js in [
    ("approve-burn-other-app", SB_CARD % ('root.againAfter(150, function() { %s })' % SB_BURN_SHEET_OTHER)),
    ("approve-burn-other-app-ticked", SB_CARD % ('root.againAfter(150, function() { %s; root.appChosenFor = "r9"; root.appUnderstood = true })' % SB_BURN_SHEET_OTHER)),
    ("approve-burn-bridge-away", SB_CARD % ('root.againAfter(150, function() { %s })' % SB_BURN_SHEET_BRIDGE)),
]:
    SCENARIOS[_name] = _js
    MOCKS[_name] = sb_mock()
# Exit keys for an Ethereum address not approved yet (exit_recipient.rs): the address in full, the
# four words this wallet made, the bridge; the tick. And one for a bridge it doesn't know (no words).
_EXIT = {"recipient": "0x3c2f2c4baec708aee232379eb00e9ecfa1b351f8", "bridge_label": "bridge v4", "vault": "0xc2c6a059eab928966b24fa5549d441a3b17b54ed", "chain_id": 11155111,
         "cfg_id": "0x0c0f00c30fbe03c00e566a332fe73aa2f11d9e0fd6b8684e509a8be33cbd8be5", "token": "0x0000000000000000000000000000000000000000", "symbol": "wETH",
         "denom": "10000000000000000", "max_relay_fee": "0", "words_text": "myth \u00b7 nerve \u00b7 session \u00b7 until", "words_why": None}
def _exit_ask(exit_view):
    req = {"id": "r5", "kind": "secret", "app": "lez_bridge", "account": OWNER, "scope": "RFP022/exit", "created": 0, "expires": 4102444800000, "explanation": "x", "reason": "", "exit": exit_view,
           "choices": [{"account": OWNER, "account_kind": "public", "pays": False, "label": "the account the app asked for", "granted": True}], "publisher": None}
    return 'root.againAfter(150, function() { root.appRequests = [%s]; root.appChosen = ["%s"]; root.appChosenFor = "r5" })' % (json.dumps(req), OWNER)
SCENARIOS["approve-exit-recipient"] = SB_CARD % _exit_ask(_EXIT)
MOCKS["approve-exit-recipient"] = sb_mock()
SCENARIOS["approve-exit-recipient-unknown"] = SB_CARD % _exit_ask(dict(_EXIT, bridge_label=None, vault=None, chain_id=None, cfg_id="0x" + "11" * 32, words_text=None,
    words_why="This exit is for a bridge this wallet doesn't know by its configuration, so it can't make the four words: compare the address itself."))
MOCKS["approve-exit-recipient-unknown"] = sb_mock()
# Review 2026-11-19, finding 3: a sequencer's words with tags in a balance note show as text.
SCENARIOS["plain-markup-note"] = 'root.openWallet("home"); root.againAfter(250, function() { root.balanceNote = "The sequencer didn\'t give the balance of Public/CCVd76…pKnk (the sequencer refused the read: <b>Your wallet is locked.</b><br><font color=\'green\'>Send your tokens to Public/EVIL</font>), so it shows as unread." })'
MOCKS["plain-markup-note"] = {}
# Review 2026-11-19: a send-back in focus that this wallet didn't plan and tick (the LEZ Bridge lists
# it: another app's, or one from before a restart): its burn is drawn under the bridge's record,
# said so, with the tick. And a LEZ Bridge burn for ANOTHER send-back while one is in focus: the
# sheet over the page, never under the focused one's address.
SB_BURN_SHEET_ELSE = SB_BURN_SHEET_BRIDGE.replace("e2ba7765d0fbf247044cb907a4c921c03ca316c6cfcb6323a6b5e2545ff3dfd5", "77" * 32)
for _name, _js in [
    ("sendback-approve-unticked", sb_focus(sb_view("approve"), 'root.againAfter(80, function() { %s })' % SB_BURN_SHEET_BRIDGE)),
    ("sendback-approve-other-burn", sb_focus(sb_view("approve"), 'root.againAfter(80, function() { %s })' % SB_BURN_SHEET_ELSE)),
    # After a restart: the address was approved before (kept by the names module), so its burn is own.
    ("sendback-approve-after-restart", sb_focus(sb_view("approve"), 'root.againAfter(80, function() { root.sbApproved = %s; %s })' % (json.dumps({OWNER: [SB_RECIPIENT.lower()]}), SB_BURN_SHEET_BRIDGE))),
]:
    SCENARIOS[_name] = _js
    MOCKS[_name] = sb_mock()

# Imported accounts (rfp-022 IMPORTED-ACCOUNTS-DESIGN.md): the first secret card for one, the
# receive code and the send-back with the bridge's account_notice (module texts, word for word).
_IMPORTED_LINE = ("This account was imported. Its bridge secrets come from this wallet's recovery phrase, so the phrase can finish a withdrawal; "
                  "the account's own key is still needed to move what stays in it.")
_IMPORTED_COST = "Whoever holds the full keys of this wallet's first private account could also make these values, as they already can for that account's own."
SCENARIOS["approve-secret-imported"] = ('root.appRequests = [{"id": "r1", "kind": "secret", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "scope": "RFP022/exit", "reason": "", "imported": true, '
    '"choices": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "account_kind": "public", "pays": false, "label": "the account the app asked for", "granted": true}], "publisher": null, '
    '"explanation": %s}]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"') % json.dumps(
    _IMPORTED_LINE + " " + _IMPORTED_COST + " lez_bridge asks for secret values of its own, \"RFP022/exit\", made from Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk. lez_bridge gets only these values, never the account's key, and no other app can get them. "
    "Whoever holds them can see what lez_bridge does with them (for a bridge, which deposits are yours), but can't spend from your account: every transaction still needs your approval here. You can take this back in Settings, Connected apps.")
MOCKS["approve-secret-imported"] = MOCKS.get("approve-secret", {})
SCENARIOS["receive-code-imported"] = SCENARIOS["receive-code"]
MOCKS["receive-code-imported"] = _copy.deepcopy(MOCKS["receive-code-words"])
MOCKS["receive-code-imported"]["bridge"]["make_public_deposit_request"]["account_notice"] = _IMPORTED_LINE
SCENARIOS["sendback-imported-private"] = sb_focus(sb_view("burned")).replace('stage: "burned" }]', 'stage: "burned", notice: "This account was imported. A wallet restored from the recovery phrase can\'t find this account\'s withdrawals by itself; keep a backup." }]', 1)
MOCKS["sendback-imported-private"] = sb_mock(sb_view("burned"))

# Checks on demand: Receive with a deposit becoming final, its one gentle check announced in minutes;
# the card while a check runs.
SCENARIOS["receive-deposited-gentle"] = RECEIVE_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 14 } })', 'root.receiveCheckedAt = Date.now() - 3000; root.openEthReceive("dep-1"); root.againAfter(200, function() { root.receiveAskNow = false; root.receiveCheckedAt = Date.now() - 3000; root.receiveNextAt = Date.now() + 147000 })')
MOCKS["receive-deposited-gentle"] = MOCKS.get("receive-deposited", {})
SCENARIOS["card-arriving-checking"] = CARD_04 % (ARR, '({ "dep-1": { stage: "deposited", confirmations: 20, final_in_min: 7 } })', CHECKED + '; root.againAfter(120, function() { root.receiveAskNow = true })')
MOCKS["card-arriving-checking"] = MOCKS.get("card-arriving-waiting", {})

# The test-LEZ faucet on the LEZ 0.4 card: offered under each public account; got; closed.
_FAUCET_CARD = SB_CARD % 'root.namesNet = "0.4"; root.loadFaucetInfo(); %s'
SCENARIOS["card-faucet"] = _FAUCET_CARD % ''
MOCKS["card-faucet"] = dict(sb_mock(), faucet={"info": {"amount": "10000000", "per_account_hours": "24", "available": True}, "claim": {"ok": True, "tx": "ab" * 32, "block": "11222", "amount": "10000000", "account": OWNER}})
SCENARIOS["card-faucet-got"] = _FAUCET_CARD % ('root.againAfter(120, function() { root.getTestLez("%s") })' % OWNER)
MOCKS["card-faucet-got"] = MOCKS["card-faucet"]
SCENARIOS["card-faucet-refused"] = _FAUCET_CARD % ('root.againAfter(120, function() { root.getTestLez("%s") })' % OWNER)
MOCKS["card-faucet-refused"] = dict(sb_mock(), faucet={"info": {"amount": "10000000", "per_account_hours": "24", "available": True}, "claim": {"ok": False, "code": "already_today", "reason": "This account already had test LEZ in the last 24 hours.", "retry": False}})
SCENARIOS["card-faucet-closed"] = _FAUCET_CARD % ''
MOCKS["card-faucet-closed"] = dict(sb_mock(), faucet={"info": {"amount": "10000000", "per_account_hours": "24", "available": False, "reason": "The faucet is empty for today; it fills again at midnight UTC."}})
OFFER = {"available": True, "reason": None, "consent_needed": True, "consent_text": "This wallet has no proof helper, so the bridge's service will make the proof: it learns which burn pays which Ethereum address."}
SCENARIOS["sendback-settle-offer"] = sb_focus(sb_view("recorded", settle_offer=OFFER))
MOCKS["sendback-settle-offer"] = sb_mock(sb_view("recorded", settle_offer=OFFER))
SCENARIOS["sendback-settle-more"] = sb_focus(sb_view("recorded", settle_offer=OFFER), 'root.againAfter(60, function() { root.sbMoreOpen = true })')
MOCKS["sendback-settle-more"] = sb_mock(sb_view("recorded", settle_offer=OFFER))
SCENARIOS["sendback-settle-consent"] = sb_focus(sb_view("recorded", settle_offer=OFFER), 'root.againAfter(60, function() { root.sbSettle(false) })')
MOCKS["sendback-settle-consent"] = sb_mock(sb_view("recorded", settle_offer=OFFER), send_back_settle={"needs": "consent", "request": None, "message": OFFER["consent_text"]})
SCENARIOS["sendback-settling"] = sb_focus(sb_view("recorded", settle_id="s1", settle={"stage": "wrapping"}))
MOCKS["sendback-settling"] = sb_mock(sb_view("recorded", settle_id="s1", settle={"stage": "wrapping"}))
SCENARIOS["sendback-settle-failed"] = sb_focus(sb_view("recorded", settle_id="s1", settle={"stage": "failed", "error": "The settlement service no longer has this send-back; finish it again."}, settle_offer=OFFER))
MOCKS["sendback-settle-failed"] = sb_mock(sb_view("recorded", settle_id="s1", settle={"stage": "failed", "error": "The settlement service no longer has this send-back; finish it again."}, settle_offer=OFFER))
SCENARIOS["sendback-settle-closed"] = sb_focus(sb_view("recorded", settle_offer={"available": False, "reason": "The bridge's settlement service is paused for maintenance."}))
MOCKS["sendback-settle-closed"] = sb_mock(sb_view("recorded", settle_offer={"available": False, "reason": "The bridge's settlement service is paused for maintenance."}))
SCENARIOS["sendback-seal-wrong"] = sb_focus(sb_view("proved"), 'root.againAfter(60, function() { root.sbSealInput = "%s"; root.sbSeal() })' % SB_CODE)
MOCKS["sendback-seal-wrong"] = sb_mock(sb_view("proved"))

# Settings → Bridge (rfp-022 NAMES-SEND-BACK.md §9): the bridge in use with its four words, the
# others to choose, Add a bridge (checked; refused; built in).
_BR_FEATURES = ["relayed_claim", "relayed_claim_check_with", "relayed_claim_build", "relayed_claim_submit", "account_hint", "receive", "deposit_code", "public_receive",
                "receive_status", "receives", "receive_request", "relayed_claim_check_deposit", "send_back", "send_back_settle", "bridges"]
_BR_V5 = {"id": "bridge-test-network-v5", "label": "bridge v5", "kind": "built in", "current": True, "vault": "0xAFbDF192434E9888Bb23DAE6f2D15Eea6246eD10", "vault_short": "0xAFbD…eD10",
          "bridge_program": "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA", "words_text": "toss · welcome · unaware · engage", "has_code": True, "items": 1, "removable": False}
_BR_V4 = dict(_BR_V5, id="bridge-test-network-v4", label="bridge v4", current=False, vault="0xC2c6A059eAB928966B24FA5549d441A3B17B54Ed", vault_short="0xC2c6…54Ed",
              bridge_program="BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf", words_text="faint · dish · hawk · jealous", items=0)
_BR_V6 = dict(_BR_V5, id="added-3f2a91c07d55", label="bridge v6", kind="added", current=False, vault="0x6666666666666666666666666666666666666666", vault_short="0x6666…6666",
              bridge_program="Six6", words_text="maple · tiger · seven · lamp", endpoint_host="lez.example.org", items=0, removable=True)
def _br_mock(**more):
    b = {"config": {"build": {"commit": SHIPPED_BUILD}, "deployment": {"name": "bridge-test-network-v5", "bridge_program": _BR_V5["bridge_program"]}, "features": _BR_FEATURES},
         "bridges": {"active": _BR_V5["id"], "active_row": _BR_V5, "list": [_BR_V5, _BR_V4, _BR_V6], "warning": "A bridge code decides where your deposits go. Take it only from the bridge's own page."}}
    b.update(more)
    return {"bridge": b}
_BR_CODE = "logosbr1" + "q" * 60
_BR_CHECKS = [{"key": "sepolia", "ok": True, "text": "Sepolia answers as chain 11155111."}, {"key": "vault", "ok": True, "text": "The vault 0x6666…6666 on Sepolia has this cfgId, bridge program, LEZ chain and zone."},
              {"key": "desk", "ok": True, "text": "The deposit desk 0x6767…6767 sends deposits into this vault."}, {"key": "lez", "ok": True, "text": "On lez.example.org, the bridge's Config names this vault, cfgId, chain, token program and tokens."}]
SCENARIOS["settings-bridge"] = 'root.section = "settings"; root.loadBridges()'
MOCKS["settings-bridge"] = _br_mock()
SCENARIOS["settings-bridge-add"] = 'root.section = "settings"; root.loadBridges(); root.bridgeAddOpen = true'
MOCKS["settings-bridge-add"] = _br_mock()
SCENARIOS["settings-bridge-checking"] = 'root.section = "settings"; root.loadBridges(); root.bridgeAddOpen = true; root.againAfter(150, function() { root.bridgeCodeText = "%s"; root.bridgeChecking = true })' % _BR_CODE
MOCKS["settings-bridge-checking"] = _br_mock()
SCENARIOS["settings-bridge-checked"] = 'root.section = "settings"; root.loadBridges(); root.bridgeAddOpen = true; root.againAfter(150, function() { root.bridgeCodeText = "%s"; root.checkBridgeCode(); root.againAfter(100, function() { root.bridgeChecksOpen = true }) })' % _BR_CODE
MOCKS["settings-bridge-checked"] = _br_mock(bridge_check={"ok": True, "id": "added-9a0b1c2d3e4f", "builtin": False, "added": False, "current": False, "label": "bridge v7", "vault": "0x7777777777777777777777777777777777777777",
    "vault_short": "0x7777…7777", "endpoint_host": "lez.example.org", "words_text": "cream · laugh · load · visual", "checks": _BR_CHECKS, "error": None})
SCENARIOS["settings-bridge-refused"] = SCENARIOS["settings-bridge-checked"]
MOCKS["settings-bridge-refused"] = _br_mock(bridge_check={"ok": False, "error": {"code": "mismatch", "message": "The deposit desk 0x6767…6767 sends deposits into 0x9999…9999, not this vault. Refused: nothing was added."},
    "checks": [_BR_CHECKS[0], _BR_CHECKS[1], {"key": "desk", "ok": False, "text": "The deposit desk 0x6767…6767 sends deposits into 0x9999…9999, not this vault."}]})
SCENARIOS["settings-bridge-builtin"] = SCENARIOS["settings-bridge-checked"]
MOCKS["settings-bridge-builtin"] = _br_mock(bridge_check={"ok": True, "id": "bridge-test-network-v4", "builtin": True, "added": False, "current": False, "label": "bridge v4", "vault_short": "0xC2c6…54Ed",
    "endpoint_host": "lez.nameswallet.app", "words_text": "faint · dish · hawk · jealous", "checks": _BR_CHECKS})
SCENARIOS["settings-bridge-chosen"] = 'root.section = "settings"; root.loadBridges(); root.againAfter(200, function() { root.useBridge("bridge-test-network-v4") })'
MOCKS["settings-bridge-chosen"] = _br_mock(bridge_use={"active": _BR_V4["id"], "active_row": dict(_BR_V4, current=True), "list": [dict(_BR_V5, current=False), dict(_BR_V4, current=True), _BR_V6]})
SCENARIOS["settings-bridge-old"] = 'root.section = "settings"; root.loadBridges()'
MOCKS["settings-bridge-old"] = {"bridge": {}}

# Bridge to Ethereum by a pasted address: read back in two halves, the four words, the one sentence;
# and an address used before, offered first.
_ADDR = "0x68B54942A37aE5ceBA34D47eFB8d33a293d3975d"
_ADDR_CHECK = {"ok": True, "recipient": _ADDR, "recipient_short": "0x68B5…975d", "recipient_halves": [_ADDR[:22], _ADDR[22:]], "route": "address",
               "code": SB_CODE, "address_warning": "Withdrawals to a wrong address can't be recovered.", "history": {"checked": True, "sent": 4, "balance": "20000000000000000", "balance_text": "0.02 ETH"}, "words": ["lens", "mushroom", "enjoy", "orient"],
               "words_text": "lens · mushroom · enjoy · orient", "fee_choice": 0, "chain_id": 11155111, "vault": "0xAFbDF192434E9888Bb23DAE6f2D15Eea6246eD10", "warnings": []}
def _sb_feat(m):
    m = _copy.deepcopy(m)
    m["bridge"].setdefault("config", _copy.deepcopy(SB_CONFIG))
    m["bridge"]["config"]["features"] = list(m["bridge"]["config"].get("features", [])) + ["send_back_address", "send_back_recipients", "bridges"]
    return m
SCENARIOS["sendback-address"] = SB_OPEN % ('root.againAfter(200, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode() })' % _ADDR)
MOCKS["sendback-address"] = _sb_feat(sb_mock(send_back_check=_ADDR_CHECK))
SCENARIOS["sendback-address-field"] = SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0 })'
MOCKS["sendback-address-field"] = _sb_feat(sb_mock())
SCENARIOS["sendback-address-lowercase"] = SB_OPEN % ('root.againAfter(200, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode() })' % _ADDR.lower())
MOCKS["sendback-address-lowercase"] = _sb_feat(sb_mock(send_back_check={"ok": False, "error": {"code": "bad_address:no_checksum", "message": "This address is all in small (or all in capital) letters, so it has no checksum and a typo can't be caught. Copy it from your wallet, don't type it."}}))
SCENARIOS["sendback-saved"] = SB_OPEN % 'root.againAfter(200, function() { root.sbDenom = 0 })'
MOCKS["sendback-saved"] = _sb_feat(sb_mock(send_back_recipients={"recipients": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "chain_id": 11155111, "address": _ADDR, "short": "0x68B5…975d", "last_used": 1}], "chain_id": 11155111}))
SCENARIOS["settings-bridge-saved"] = 'root.section = "settings"; root.loadBridges()'
MOCKS["settings-bridge-saved"] = _br_mock(send_back_recipients={"recipients": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "chain_id": 11155111, "address": _ADDR, "short": "0x68B5…975d", "last_used": 1}], "chain_id": 11155111})
_BR_FEATURES += ["send_back_address", "send_back_recipients"]


# ── The private claim and the private send-back (rfp-022 proposal/PRIVATE-IN-WALLET-SCOPE.md, step 7):
# a private choice on Receive, "Claim privately" on an arrived private receive, a private Bridge to
# Ethereum; the wallet's own sheet in place; the proving wait; each ending. The bridge's answers are
# what Logic.PRIVATE_BRIDGE reads (claim_plan, claim, job, jobs, send_back_burn). ──
V5 = "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA"
PRIV = "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"
PV_FEATURES = SB_FEATURES + ["send_back_settle", "bridges", "private_claim_standin", "private_send_back"]
PV_CONFIG = {"build": {"commit": SHIPPED_BUILD}, "deployment": {"name": "bridge-test-network-v5", "bridge_program": V5, "release": "proof"}, "features": PV_FEATURES, "notices": {}}
OLD_CONFIG = dict(PV_CONFIG, features=SB_FEATURES)
PV_ARR = '[{ id: "dep-p1", symbol: "ETH", amount: "0.1", account: "%s", account_kind: "private" }]' % PRIV
PV_REG = {"tokens": [{"symbol": "ETH", "decimals": 18, "denoms": [{"index": 0, "value": "10000000000000000", "text": "0.01"}, {"index": 1, "value": "100000000000000000", "text": "0.1"}, {"index": 2, "value": "1000000000000000000", "text": "1"}]}]}

def pv_job(phase, state="running", wr=None, error=None, note=None, ago=60, kind="claim", subject="dep-p1", tx=None, request="r-pv1"):
    """The module's wallet job view (lez_bridge wallet.rs job_view), as `job(id)` answers it."""
    wr = wr or {"state": "approved" if phase not in ("approve",) else "pending", "check": "ready", "phase": phase}
    return {"id": "wr-1792000000001", "kind": kind, "subject": subject, "request": request, "started": "@AGO:%d@" % ago, "state": state, "phase": phase,
            "wallet_request": wr, "error": error, "result": {"tx": tx, "ok": True} if state == "done" else None, "elapsed_s": ago, "note": note,
            "can_cancel": state == "running" and phase in ("approve", "simulate", "prove"), "wallet": "names", "prove_estimate_s": 671}
def js(v):
    out = json.dumps(v)
    import re as _re
    return _re.sub(r'"@AGO:(\d+)@"', lambda m: "Date.now() - %d" % (int(m.group(1)) * 1000), out)

def pv_request(kind_of="claim", check="ready", problem=None, rid="r-pv1"):
    """A private_call as pending_requests answers it: the wallet's own sheet (provider_private.rs)."""
    if kind_of == "claim":
        lines = [PRIV[:14] + "…4Z receives 0.1 (100,000,000,000,000,000 base units) wETH (DbPGo6…BRVJ).", PRIV[:14] + "…4Z receives 19,345,048 LEZ: the bridge's sponsored fee credit, less the fee.",
                 "Nothing leaves your account.", "Fee: about 5,654,952 LEZ, at most 9,300,135, paid inside the proof from the bridge's sponsored credit."]
        summary = "Claim 0.1 ETH from Sepolia (deposit dep-p1)"
        changes = ["Public/END4S7…cXFu: CuV1ct…nDMA's data changes", "wETH (DbPGo6…BRVJ): supply 1,200,000,000,000,000,000 → 1,300,000,000,000,000,000"]
    else:
        lines = [PRIV[:14] + "…4Z sends 0.01 (10,000,000,000,000,000 base units) wETH (DbPGo6…BRVJ) to be burned.", "The burn tells bridge v5 (CuV1ct…nDMA) about it, with an exit commitment that hides the Ethereum address.",
                 "Fee: about 4,903,112 LEZ, at most 8,800,000, paid inside the proof from your account."]
        summary = "Send back 0.01 ETH to 0xE2C4857976b346F8F9b3a7be714Cd92d0bF68680 on Ethereum (exit 0xe2ba7765…)"
        changes = ["wETH (DbPGo6…BRVJ): supply 1,300,000,000,000,000,000 → 1,290,000,000,000,000,000", "Public/END4S7…cXFu: CuV1ct…nDMA's data changes"]
    sheet = None if check != "ready" else {"lines": lines, "public_changes": changes,
             "privacy": ["Public: the programs it runs (2), the public accounts it reaches and the changes above, the fee reserve, and the time.", "Private: " + PRIV + ", its balance, and the message to it (proven, not shown).",
                         "Checked: your account's ID appears nowhere in the public state and events this call leaves (the messages between programs aren't checked)."],
             "programs": [{"program": V5, "label": {"registry": False, "text": None, "kind": None, "verified": None, "stale": None, "upgradeable": None}, "new_for_app": False},
                          {"program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "label": {"registry": False, "text": None, "kind": None, "verified": None, "stale": None, "upgradeable": None}, "new_for_app": False}],
             "warnings": [], "refusals": [], "refused": False, "digest": "77e1"}
    return {"id": rid, "kind": "private_call", "app": "lez_bridge", "created": 0, "expires": 4102444800000, "account": PRIV, "check": check, "check_problem": problem,
            "message_len": 1335 if kind_of == "claim" else 145, "prove_expect": "Proving runs on this computer: about 11 minutes and about 9.5 GB of memory (measured for a bridge claim on an Apple M4 with 16 GB, 4 threads). Other apps may be slow meanwhile.",
            "app_summary": summary, "sheet": sheet, "choices": [{"account": PRIV, "account_kind": "private", "pays": True, "label": "the account the app asked for", "granted": True}],
            "zone": "home", "zone_name": "Home", "zone_in_use": True, "zone_removed": False, "publisher": None}

PV_HELD = [{"token_program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "app": "lez_bridge", "holdings": [{"account": PRIV, "account_kind": "private", "definition": "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw", "symbol": "wETH", "balance": "100000000000000000", "decimals": 18}]}]
def pv_mock(**bridge):
    b = {"config": PV_CONFIG, "registry": PV_REG, "receives": {"receives": []}, "jobs": {"jobs": []}, "send_back_link": SB_LINK, "send_back_check": SB_CHECK, "send_backs": {"send_backs": []}}
    b.update(bridge)
    # The private account's bridged holding, as token_source_holdings answers it.
    return {"bridge": b, "bridgeNet": NET04_V4, "bridged": PV_HELD}

# The card, then Claim privately on the arrived receive dep-p1 (`then`: more JS once it is open).
PV_CLAIM = CARD_04 % (PV_ARR, '({ "dep-p1": { stage: "claimable" } })', CHECKED + '; root.bridgeConfig = %s; root.openPrivateClaim("dep-p1"); root.againAfter(80, function() { %%s })' % json.dumps(PV_CONFIG))
def pv_claim(then=""):
    return PV_CLAIM % then
def pv_take(job, extra=""):
    return 'root.pcTakeJob(%s); %s' % (js(job), extra)
APPROVED_4MIN = 'root.privateApprovedAt = { "wr-1792000000001": Date.now() - 252000 }; '
def pv_inline(req):
    return 'root.appRequests = [%s]; root.appChosen = ["%s"]; root.appChosenFor = "%s"' % (json.dumps(req).replace("4102444800000", "Date.now() + 540000"), PRIV, req["id"])
FAILED_TOOL = "This build of the LEZ 0.4 names tool can't make proofs: it has no in-process prover, and no r0vm was found."

PRIVATE_SCENARIOS = [
    # Receive: Private is chosen first (the privacy default), its one line, the private accounts.
    ("receive-private-choice", 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })', pv_mock()),
    # The code for a private account (make_deposit_request), then the page as before.
    ("receive-private-code", 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.againAfter(200, function() { root.makeReceive() }) }) })',
     pv_mock(make_deposit_request={"deposit": {"id": "dep-p1", "account": PRIV, "account_kind": "private"}, "symbol": "ETH", "denom_text": "0.1", "request": "logosdep1qyqqqqqqqqqzr7dm2wcgqnmxn3uq7c2yupx7qfguwl6zcw0d2ej8zhwpsa8x4a5xg9ws3xcqp",
                                     "request_url": "https://nameswallet.app/bridge/#logosdep1qyqqqqqqqqqzr7dm2wcgqnmxn3uq7c2yupx7qfguwl6zcw0d2ej8zhwpsa8x4a5xg9ws3xcqp", "request_qr": {"size": 0, "rows": []},
                                     "compare": {"first": "a50e0f", "last": "9d21c4", "text": "a50e0f … 9d21c4", "words": ["cream", "laugh", "load", "visual"], "words_text": "cream · laugh · load · visual"},
                                     "bridge_fee": {"text": "0.1 ETH into " + PRIV[:14] + "…; the bridge fee is 0.0003 ETH (0.3%); you send 0.1003 ETH in all."}, "commitment": "0x" + "a5" * 32})),
    # The deposit keys asked the first time, answered on the screen.
    ("receive-private-keys", 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.againAfter(200, function() { root.makeReceive(); root.againAfter(100, function() { root.appRequests = [{ id: "r-k1", kind: "secret", app: "lez_bridge", account: "%s", scope: "RFP022/deposit", explanation: "The LEZ Bridge gets values made from this account\'s key for its deposit secrets. It can\'t spend from the account.", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "%s", label: "Private account", granted: true }], publisher: null, checking: false }]; root.appChosen = ["%s"]; root.appChosenFor = "r-k1" }) }) }) })' % (PRIV, PRIV, PRIV),
     pv_mock(make_deposit_request={"needs": "secret", "request": {"request": "r-k1", "state": "pending"}, "message": "Approve \"RFP022/deposit\" in the Names app."})),
    # No private account on LEZ 0.4 yet.
    ("receive-private-noaccount", 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive() }) })',
     dict(pv_mock(), accounts=[{"account": OWNER, "kind": "public", "label": None, "balance": "1250000", "default": True, "public_key": "ab" * 32}])),
    # An older LEZ Bridge (no private codes): Public is chosen; choosing Private says why it can't.
    ("receive-private-old-bridge", 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); root.againAfter(200, function() { root.ethReceiveKind = "private" }) }) })',
     pv_mock(config=OLD_CONFIG)),
    # The card: a private receive arrived, "Claim privately".
    ("card-arriving-private", CARD_04 % (PV_ARR, '({ "dep-p1": { stage: "claimable" } })', CHECKED + '; root.bridgeConfig = %s' % json.dumps(PV_CONFIG)), pv_mock()),
    # The card while its claim is proved here, and a private burn on its way.
    ("card-private-proving", CARD_04 % (PV_ARR, '({ "dep-p1": { stage: "claimable" } })', CHECKED + '; root.bridgeConfig = %s; root.privateClaimJobs = { "dep-p1": %s }' % (json.dumps(PV_CONFIG), js(pv_job("prove")))), pv_mock()),
    # Claim privately: the note and the button.
    ("claim-private-start", pv_claim(), pv_mock()),
    # The button at once: "Checking…" (the bridge's checks under way).
    ("claim-private-checking", pv_claim('root.pcChecking = true'), pv_mock()),
    # A memory warning on a computer under 16 GB.
    ("claim-private-memory", pv_claim('root.machineMemory = 8 * 1024 * 1024 * 1024'), pv_mock()),
    # The checks refused it: not final yet (a wait, Check now), already claimed (the way back).
    ("claim-private-notfinal", pv_claim('root.pcStart()'), pv_mock(claim_plan={"ok": False, "error": {"code": "not_final", "message": "The deposit is not final on Sepolia yet: about 6 minutes to go."}})),
    ("claim-private-claimed", pv_claim('root.pcStart()'), pv_mock(claim_plan={"ok": False, "error": {"code": "already_claimed", "message": "This deposit is already claimed."}})),
    # The first-time question (the deposit keys), answered here.
    ("claim-private-keys", pv_claim('root.pcStart(); root.againAfter(60, function() { root.appRequests = [{ id: "r-k1", kind: "secret", app: "lez_bridge", account: "%s", scope: "RFP022/deposit", explanation: "The LEZ Bridge gets values made from this account\'s key for its deposit secrets. It can\'t spend from the account.", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "%s", label: "Private account", granted: true }], publisher: null, checking: false }]; root.appChosen = ["%s"]; root.appChosenFor = "r-k1" })' % (PRIV, PRIV, PRIV)),
     pv_mock(claim_plan={"ok": False, "needs": {"needs": "secret", "request": {"request": "r-k1", "state": "pending"}, "message": "Approve \"RFP022/deposit\" in the Names app."}})),
    # Proposed: the wallet simulates it before its sheet can be approved.
    ("claim-private-simulating", pv_claim('root.pcStart(); root.againAfter(60, function() { %s })' % pv_inline(pv_request(check="checking"))),
     pv_mock(claim_plan={"ok": True, "sponsored": True}, claim=pv_job("approve", wr={"state": "pending", "check": "checking", "phase": None}))),
    # The wallet's own sheet, in place: "Approve, prove and send".
    ("claim-private-approve", pv_claim('root.pcStart(); root.againAfter(60, function() { %s })' % pv_inline(pv_request())),
     pv_mock(claim_plan={"ok": True, "sponsored": True}, claim=pv_job("approve", wr={"state": "pending", "check": "ready", "phase": None}))),
    # The wallet's check failed (this build's names tool can't prove): the sheet says why; Reject.
    ("claim-private-check-failed", pv_claim(pv_take(pv_job("approve", state="failed", wr={"state": "pending", "check": "failed", "phase": "plan"}, error={"code": "wallet_check", "message": "The Names Wallet's check stopped this call. Open the Names app to see why."})) + pv_inline(pv_request(check="failed", problem=FAILED_TOOL))), pv_mock()),
    # Approved: proving on this computer, the clock, the page may be left.
    ("claim-private-proving", pv_claim(APPROVED_4MIN + pv_take(pv_job("prove"))), pv_mock()),
    # Re-entry: the claim found with the wallet (jobs()), checked once more before proving.
    ("claim-private-reopened", pv_claim(''), pv_mock(jobs={"jobs": [pv_job("simulate", ago=95)]})),
    # The wallet was busy when asked: the wait goes on, in amber.
    ("claim-private-busy", pv_claim(APPROVED_4MIN + pv_take(pv_job("prove", note="Asking your Names Wallet how this stands: it is busy. Asking again shortly."))), pv_mock()),
    # Taking longer than usual (17 min).
    ("claim-private-long", pv_claim('root.privateApprovedAt = { "wr-1792000000001": Date.now() - 1030000 }; ' + pv_take(pv_job("prove"))), pv_mock()),
    # Check now pressed: "Checking…" at once.
    ("claim-private-checknow", pv_claim(APPROVED_4MIN + pv_take(pv_job("prove")) + '; root.pcPolling = true'), pv_mock()),
    # Proved: sent, waiting for its block.
    ("claim-private-sending", pv_claim(APPROVED_4MIN + pv_take(pv_job("submit"))), pv_mock()),
    # Done.
    ("claim-private-done", pv_claim(pv_take(pv_job("done", state="done", tx="9f2c" * 16)) + '; root.pcDetails = true'), pv_mock()),
    # Failures: the proof or the send failed; rejected; lapsed while still being checked; nobody answered.
    ("claim-private-failed", pv_claim(APPROVED_4MIN + pv_take(pv_job("prove", state="failed", wr={"state": "failed", "check": "ready", "phase": "prove"}, error={"code": "wallet_failed", "message": "The Names Wallet says: proving stopped: this computer ran out of memory. Open the Names app for the details."}))), pv_mock()),
    ("claim-private-rejected", pv_claim(pv_take(pv_job("approve", state="cancelled", wr={"state": "rejected", "check": "ready", "phase": None}, error={"code": "wallet_rejected", "message": "You rejected it in the Names app; nothing was signed or sent."}))), pv_mock()),
    ("claim-private-lapsed", pv_claim(pv_take(pv_job("approve", state="cancelled", wr={"state": "expired", "check": "checking", "phase": "plan"}, error={"code": "wallet_rejected", "message": "Nobody answered it in the Names app within 10 minutes; nothing was signed or sent."}))), pv_mock()),
    ("claim-private-expired", pv_claim(pv_take(pv_job("approve", state="cancelled", wr={"state": "expired", "check": "ready", "phase": None}, error={"code": "wallet_rejected", "message": "Nobody answered it in the Names app within 10 minutes; nothing was signed or sent."}))), pv_mock()),
    # A proposal that got no answer: it may be waiting in the Names app; Continue after looking.
    ("claim-private-uncertain", pv_claim('root.pcStart()'), pv_mock(claim_plan={"ok": True, "sponsored": True}, claim={"__error": "Your Names Wallet didn't answer in time, so this claim may be waiting in the Names app. If it is there, approve or reject it there (approved, the wallet proves and sends it itself, and this deposit then shows as claimed). This app doesn't propose it again for 11 minutes. Nothing was sent from here."})),
    # The bridge call itself failed (busy): one line, Try again.
    ("claim-private-error", pv_claim('root.pcStart()'), pv_mock(claim_plan={"__error": "the bridge test network can't be reached (https://lez.nameswallet.app/): connection timed out"})),
    # An older LEZ Bridge: no private claim; why under the button.
    ("claim-private-old-bridge", CARD_04 % (PV_ARR, '({ "dep-p1": { stage: "claimable" } })', CHECKED + '; root.bridgeConfig = %s; root.openPrivateClaim("dep-p1")' % json.dumps(OLD_CONFIG)), pv_mock(config=OLD_CONFIG)),
]
for _name, _js, _mock in PRIVATE_SCENARIOS:
    SCENARIOS[_name] = _js
    MOCKS[_name] = _mock

# Bridge to Ethereum from a private account.
PV_SB_CARD = 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.bridgeConfig = %s; root.loadBridgeNet(); root.bridgedHoldings = %s; %%s }) })' % (
    json.dumps(PV_CONFIG), json.dumps([{"token_program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "app": "lez_bridge", "holdings": [{"account": PRIV, "account_kind": "private", "definition": "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw", "symbol": "wETH", "balance": "100000000000000000", "decimals": 18}]}]))
PV_SB_ID = "sb-p1"
def pv_sb_focus(stage, job, extra=""):
    view = sb_view(stage, id=PV_SB_ID, account=PRIV, kind="private", route="private_call", wallet_job=job["id"] if job else None, burn_phase=job["phase"] if job and stage == "approve" else None, fee_payer="Your wallet pays the LEZ fee from this private account's own LEZ, inside the proof.", privacy="A private burn publishes the amount and an exit commitment; the account it leaves and the Ethereum address stay hidden until the release.")
    jobs = {"jobs": [job]} if job else {"jobs": []}
    js_ = PV_SB_CARD % ('root.sendBacksLoaded = true; root.sendBacks = [{ id: "%s", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "maple · tiger · seven · lamp", stage: "%s" }]; root.openSendBack("", null, "%s"); root.sbStatuses = { "%s": %s }; root.againAfter(50, function() { root.sbAskNow = false; root.receiveAskNow = false; root.sbScheduleNext() }); %s'
                       % (PV_SB_ID, PRIV, SB_RECIPIENT, stage, PV_SB_ID, PV_SB_ID, json.dumps(view), extra))
    return js_, pv_mock(send_backs={"send_backs": [view]}, send_back_status=view, jobs=jobs, job=job)

PRIVATE_SB = [
    # The accounts to send from: the public ones as before, then the private ones (proved here).
    ("sendback-private-choose", PV_SB_CARD % 'root.againAfter(150, function() { root.openSendBack("", null) })', pv_mock()),
    # A private account chosen: its amounts, then the address step with its line.
    ("sendback-private-address", PV_SB_CARD % 'root.againAfter(120, function() { root.openSendBack("", null); root.againAfter(120, function() { root.sbPick(root.sbPrivateSources[0]); root.sbDenom = 0 }) })', pv_mock()),
    # From a private account, with the address book: a previous address picked and ticked.
    ("sendback-private-address-ticked", PV_SB_CARD % 'root.againAfter(100, function() { root.openSendBack("", null); root.againAfter(100, function() { root.sbPick(root.sbPrivateSources[0]); root.sbDenom = 0; root.againAfter(80, function() { root.sbChooseAddress(root.sbBook[0]); root.againAfter(60, function() { root.sbTicked = true }) }) }) })',
     pv_mock(config=dict(PV_CONFIG, features=PV_FEATURES + ["send_back_address", "send_back_recipients", "address_book"]), address_book_list={"addresses": [{"address": SB_RECIPIENT, "short": "0xE2C4…8680", "nickname": "ledger", "words_text": "maple · tiger · seven · lamp", "saved": True}]})),
    # The burn proposed: the address and its words, then the wallet's own sheet in place.
    ("sendback-private-approve", *pv_sb_focus("approve", pv_job("approve", kind="redeem", subject=PV_SB_ID, wr={"state": "pending", "check": "ready", "phase": None}), 'root.againAfter(120, function() { %s })' % pv_inline(pv_request("burn")))),
    # Approved: proving the burn on this computer.
    ("sendback-private-proving", *pv_sb_focus("approve", pv_job("prove", kind="redeem", subject=PV_SB_ID), APPROVED_4MIN)),
    # The proof failed.
    ("sendback-private-failed", *pv_sb_focus("approve", pv_job("prove", state="failed", kind="redeem", subject=PV_SB_ID, wr={"state": "failed", "check": "ready", "phase": "prove"}, error={"code": "wallet_failed", "message": "The Names Wallet says: the sequencer refused it: the account doesn't hold 0.01 wETH. Open the Names app for the details."}), APPROVED_4MIN)),
    # Sent: the send-back's own stages follow (burned, then Ethereum records it).
    ("sendback-private-burned", *pv_sb_focus("burned", pv_job("done", state="done", kind="redeem", subject=PV_SB_ID, tx="9f2c" * 16))),
]
for _name, _js, _mock in PRIVATE_SB:
    SCENARIOS[_name] = _js
    MOCKS[_name] = _mock
# ── wallet-1120: a private holding with a burn (the M2 install of 2026-11-19) and the release
# prover that changed (the LEZ Bridge next-1119b's "prover_changed"). ──
PV_DEF = "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw"
PV_REG_DEF = {"tokens": [dict(PV_REG["tokens"][0], definition=PV_DEF)]}
PV_BURNED = sb_view("recorded", id="sb-p0", account=PRIV, kind="private", route="private_call", symbol="ETH")
def pv_held_case(bridged, registry=PV_REG):
    js_ = PV_SB_CARD % ('root.sendBacksLoaded = true; root.sendBacks = [{ id: "sb-p0", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "maple · tiger · seven · lamp", stage: "recorded", symbol: "ETH" }]; root.bridgedHoldings = %s; root.againAfter(150, function() { root.openSendBack("", null) })'
                        % (PRIV, SB_RECIPIENT, json.dumps(bridged)))
    m = pv_mock(send_backs={"send_backs": [PV_BURNED]}, send_back_status=PV_BURNED, registry=registry)
    m["bridged"] = bridged
    return js_, m
# As on the M2: the token listing not accepted, so the wallet can't read the account; its wETH
# burned earlier is held back with a line and "Show the token listing", not offered at every amount.
SCENARIOS["sendback-private-burned-held"], MOCKS["sendback-private-burned-held"] = pv_held_case([])
# The listing accepted, its read not made yet: nothing offered, "reading what it holds…".
SCENARIOS["sendback-private-reading"], MOCKS["sendback-private-reading"] = pv_held_case(
    [{"token_program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "app": "lez_bridge", "definitions": [PV_DEF], "read": False, "updating": True, "error": None, "holdings": []}], PV_REG_DEF)
# Read: the account holds none (it was burned): no chip for it.
SCENARIOS["sendback-private-burned-read"], MOCKS["sendback-private-burned-read"] = pv_held_case(
    [{"token_program": "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "app": "lez_bridge", "definitions": [PV_DEF], "read": True, "updating": False, "error": None,
      "holdings": [{"account": PRIV, "account_kind": "private", "definition": PV_DEF, "symbol": "wETH", "balance": "0", "decimals": 18}]}], PV_REG_DEF)
PROVER_CHANGED_WORDS = "The release prover changed since it was set (/Volumes/exos162/logos/rfp-022/v4/release-prover.sh), so this module didn't run it. If you updated it on purpose: use the release prover again from the Names app (only the Names app can set it)."
SCENARIOS["sendback-prover-changed"] = sb_focus(sb_view("recorded", releasable_at="@HOUR@"), 'root.againAfter(50, function() { root.sbProve() })')
MOCKS["sendback-prover-changed"] = sb_mock(sb_view("recorded"), send_back_prove={"__error": PROVER_CHANGED_WORDS, "__code": "prover_changed"})
# "Use this prover again" pressed: set_release_prover with the bridge's path, then the proof once more.
SCENARIOS["sendback-prover-again"] = sb_focus(sb_view("recorded", releasable_at="@HOUR@"), 'root.againAfter(50, function() { root.sbProve(); root.againAfter(250, function() { root.sbProverAgain() }) })')
MOCKS["sendback-prover-again"] = sb_mock(sb_view("recorded"), send_back_prove={"__seq": [{"__error": PROVER_CHANGED_WORDS, "__code": "prover_changed"}, {"stage": "proving"}]},
                                         set_release_prover={"release_prover": ["/Volumes/exos162/logos/rfp-022/v4/release-prover.sh"], "release_prover_sha256": "ab" * 32, "release_prover_files": []})
# ── wallet-next (rfp-022 proposal/WALLET-ROADMAP.md items 1, 4, 5, 7, 8): Bridge to Ethereum's address
# step, Save to my addresses, Remove on the card. The bridge's new calls (address_book_*, receive_remove,
# send_back_remove) answer as docs/WALLET-NEXT-MODULE-CONTRACT.md says; nothing here is a real module. ──
WN_FEATURES = SB_FEATURES + ["send_back_settle", "bridges", "send_back_address", "send_back_recipients", "address_book", "receive_remove", "send_back_remove"]
WN_CONFIG = dict(SB_CONFIG, features=WN_FEATURES)
_ADDR3 = "0x4B20993Bc481177ec7E8f571ceCaE8A9e22C02db"
WN_BOOK = {"addresses": [
    {"address": _ADDR, "short": "0x68B5…975d", "nickname": "main", "words": ["lens", "mushroom", "enjoy", "orient"], "words_text": "lens · mushroom · enjoy · orient", "last_used": 1795000000, "saved": True},
    {"address": _ADDR3, "short": "0x4B20…02db", "nickname": "ledger", "words": ["frost", "cabin", "upper", "salad"], "words_text": "frost · cabin · upper · salad", "last_used": 1794000000, "saved": True},
    {"address": SB_RECIPIENT, "short": "0xE2C4…8680", "nickname": "", "words": ["maple", "tiger", "seven", "lamp"], "words_text": "maple · tiger · seven · lamp", "last_used": 1793000000, "saved": False}],
    "deployment": "bridge-test-network-v4", "chain_id": 11155111}
def wn_mock(status=None, **bridge):
    return sb_mock(status, **dict({"config": WN_CONFIG, "address_book_list": WN_BOOK}, **bridge))
_STEP2 = SB_OPEN % 'root.againAfter(120, function() { root.sbDenom = 0; %s })'
_PICK = 'root.againAfter(120, function() { root.sbChooseAddress(root.sbBook[0]); %s })'
_SB_SAVE_FOCUS = sb_focus(sb_view("burned"), 'root.againAfter(120, function() { %s })')
WN_ARR = ('[{ id: "dep-old", symbol: "ETH", amount: "1", account: "%s" }, { id: "dep-2", symbol: "ETH", amount: "0.1", account: "%s" }]' % (OWNER, OWNER))
WN_ARR_ST = '({ "dep-old": { stage: "requested" }, "dep-2": { stage: "deposited", confirmations: 20, final_in_min: 7 } })'
WN_SBS = ('root.sendBacksLoaded = true; root.sendBacks = [{ id: "sb-planned", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "", stage: "planned" }, { id: "sb-e2ba7765d0fbf247", amount: "0.01 ETH", account: "%s", recipient: "%s", words: "", stage: "burned" }]; '
          'root.sbStatuses = { "sb-planned": { stage: "planned" }, "sb-e2ba7765d0fbf247": { stage: "burned" } }' % (OWNER, SB_RECIPIENT, OWNER, SB_RECIPIENT))
def wn_card(then="", arr=WN_ARR, st=WN_ARR_ST, sbs=WN_SBS):
    return CARD_04 % (arr, st, CHECKED + '; root.bridgeConfig = %s; %s; root.againAfter(60, function() { %s })' % (json.dumps(WN_CONFIG), sbs, then))
def wn_card_mock(**bridge):
    return {"bridge": dict({"config": WN_CONFIG, "receives": {"receives": []}, "send_backs": {"send_backs": []}}, **bridge), "bridgeNet": NET04_V4}
WALLET_NEXT = [
    # Step 2, the address step: the box empty (nothing pre-filled), the previous addresses under it.
    ("sendback-address-empty", _STEP2 % '', wn_mock()),
    # The first time: no previous addresses; where the code comes from.
    ("sendback-address-first", _STEP2 % '', wn_mock(address_book_list={"addresses": [], "deployment": "bridge-test-network-v4"})),
    # Something typed that isn't a code or an address yet.
    ("sendback-address-partial", _STEP2 % 'root.sbSetBox("0x68B54942A37a")', wn_mock()),
    # A pasted address being checked by the bridge.
    ("sendback-address-checking", _STEP2 % ('root.sbSetBox("%s"); root.sbChecking = true' % _ADDR), wn_mock()),
    # A previous address picked: the box filled, the address in full with its four words, what they are,
    # where to compare them; the tick not set, the button waiting for it.
    ("sendback-address-picked", _STEP2 % (_PICK % ''), wn_mock(send_back_check=_ADDR_CHECK)),
    # Ticked: the button can be pressed.
    ("sendback-address-ticked", _STEP2 % (_PICK % 'root.againAfter(60, function() { root.sbTicked = true })'), wn_mock(send_back_check=_ADDR_CHECK)),
    # The bridge page's code pasted (not a previous address): the same read-back.
    ("sendback-address-code", _STEP2 % ('root.sbSetBox("%s"); root.sbCheckCode()' % SB_CODE), wn_mock()),
    # An address without its checksum: refused by the bridge, in its words.
    ("sendback-address-refused", _STEP2 % ('root.sbSetBox("%s"); root.sbCheckCode()' % _ADDR.lower()),
     wn_mock(send_back_check={"ok": False, "error": {"code": "bad_address:no_checksum", "message": "This address is all in small (or all in capital) letters, so it has no checksum and a typo can't be caught. Copy it from your wallet, don't type it."}})),
    # The bridge busy with another call: a wait, with Check again.
    ("sendback-address-busy", _STEP2 % ('root.sbSetBox("%s"); root.sbCheckCode()' % _ADDR), wn_mock(send_back_check={"__error": "The LEZ Bridge is still answering an earlier call."})),
    # An older LEZ Bridge (no address book): the addresses this account sent to before, without names.
    ("sendback-address-old-bridge", _STEP2 % '', _sb_feat(sb_mock(send_back_recipients={"recipients": [{"account": OWNER, "chain_id": 11155111, "address": _ADDR, "short": "0x68B5…975d", "last_used": 1}], "chain_id": 11155111}))),
    # Step 3: the burn's approval keeps the address and its words, and says the burn is next.
    ("sendback-approve-next", SB_OPEN % ('root.againAfter(60, function() { root.sbDenom = 0; root.sbSetBox("%s"); root.sbCheckCode(); root.againAfter(60, function() { root.sbTicked = true; root.sbContinue(); root.againAfter(60, function() { %s }) }) })' % (SB_CODE, SB_BURN_SHEET)),
     wn_mock(send_back_plan=sb_view("planned"), send_back_burn=sb_view("approve"))),
    # After the first send to an address with no name here: Save to my addresses.
    ("sendback-save-offer", _SB_SAVE_FOCUS % '', wn_mock(sb_view("burned"))),
    ("sendback-save-typed", _SB_SAVE_FOCUS % 'sbSaveField.text = "ledger 2"', wn_mock(sb_view("burned"))),
    ("sendback-save-badname", _SB_SAVE_FOCUS % 'sbSaveField.text = "my/wallet"', wn_mock(sb_view("burned"))),
    ("sendback-save-saving", _SB_SAVE_FOCUS % 'sbSaveField.text = "ledger 2"; root.sbSaving = true', wn_mock(sb_view("burned"))),
    ("sendback-saved", _SB_SAVE_FOCUS % 'sbSaveField.text = "ledger 2"; root.sbSaveAddress()',
     wn_mock(sb_view("burned"), address_book_save={"ok": True, "entry": {"address": SB_RECIPIENT, "nickname": "ledger 2"}, "addresses": [dict(WN_BOOK["addresses"][2], nickname="ledger 2", saved=True)] + WN_BOOK["addresses"][:2]})),
    ("sendback-save-refused", _SB_SAVE_FOCUS % 'sbSaveField.text = "ledger 2"; root.sbSaveAddress()', wn_mock(sb_view("burned"), address_book_save={"ok": False, "error": {"code": "full", "message": ""}})),
    # The card: Remove on the unpaid code and the never-proposed send-back only (not on the deposit on
    # its way, not on the burned send-back).
    ("card-remove", wn_card(), wn_card_mock()),
    ("card-remove-checking", wn_card('root.bridgeRemoving = "receive:dep-old"'), wn_card_mock()),
    ("card-remove-done", wn_card('root.removeBridgeRow("receive", "dep-old")'), wn_card_mock(receive_remove={"removed": True, "id": "dep-old", "code": "no_deposit", "checked": {"chain": "sepolia", "deposit_found": False}})),
    # Paid meanwhile: the bridge's check at the press found the deposit; nothing removed.
    ("card-remove-refused", wn_card('root.removeBridgeRow("receive", "dep-old")'), wn_card_mock(receive_remove={"removed": False, "id": "dep-old", "code": "deposited", "checked": {"chain": "sepolia", "deposit_found": True}})),
    ("card-remove-unreachable", wn_card('root.removeBridgeRow("receive", "dep-old")'), wn_card_mock(receive_remove={"removed": False, "id": "dep-old", "code": "unreachable"})),
    ("card-remove-sendback", wn_card('root.removeBridgeRow("send_back", "sb-planned")'), wn_card_mock(send_back_remove={"removed": True, "id": "sb-planned", "code": "never_burned", "checked": {"chain": "lez", "burn_found": False}})),
    # The last row removed: the panel goes, the line stays.
    ("card-remove-last", wn_card('root.removeBridgeRow("receive", "dep-old")', arr='[{ id: "dep-old", symbol: "ETH", amount: "1", account: "%s" }]' % OWNER, st='({ "dep-old": { stage: "requested" } })', sbs='root.sendBacks = []'),
     wn_card_mock(receive_remove={"removed": True, "id": "dep-old", "code": "no_deposit"})),
    # Settings → Bridge: the address book, names and all, each with Remove.
    ("settings-bridge-book", 'root.section = "settings"; root.loadBridges()', _br_mock(config={"build": {"commit": SHIPPED_BUILD}, "deployment": {"name": "bridge-test-network-v5", "bridge_program": _BR_V5["bridge_program"]},
     "features": _BR_FEATURES + ["send_back_address", "send_back_recipients", "address_book"]}, address_book_list=WN_BOOK)),
    # Remove on a claimed receive and on a released send-back (both still shown on the card for a while).
    ("card-remove-finished", wn_card('root.receiveClaimedAt = { "dep-old": Date.now() - 60000 }; root.sbReceivedAt = { "sb-planned": Date.now() - 60000 }',
        st='({ "dep-old": { stage: "claimed", claimed_block: 6054 }, "dep-2": { stage: "deposited", confirmations: 20, final_in_min: 7 } })',
        sbs=WN_SBS.replace('"sb-planned": { stage: "planned" }', '"sb-planned": { stage: "released", released_tx: "0x7a" }')), wn_card_mock()),
    # A send-back the bridge keeps: its burn found on the bridge test network at the press.
    ("card-remove-sendback-kept", wn_card('root.removeBridgeRow("send_back", "sb-planned")'), wn_card_mock(send_back_remove={"removed": False, "id": "sb-planned", "code": "in_flight", "checked": {"chain": "lez", "burn_found": True}})),
    # A name another saved address already has.
    ("sendback-save-taken", _SB_SAVE_FOCUS % 'sbSaveField.text = "main"; root.sbSaveAddress()', wn_mock(sb_view("burned"), address_book_save={"ok": False, "error": {"code": "nickname_taken", "message": ""}})),
    # Search: the network names are checked on (WALLET-ROADMAP item 4: "checking a name", never "look up").
    ("search-checked-on", 'root.section = "search"', {}),
]
for _name, _js, _mock in WALLET_NEXT:
    SCENARIOS[_name] = _js
    MOCKS[_name] = _mock

# Move wrapped tokens on the bridge test network (roadmap 12): from the imported CCVd (0.1 wETH on
# bridge v5) to the phrase account 7wx3, the case of 2026-11-18. The module's answers are set as state.
_MV_FEAT = 'root.againAfter(150, function() { root.moduleFeatures = root.moduleFeatures.concat(["bridge_token_send"]) })'
_MV_ASSET = {"symbol": "wETH", "name": "Wrapped Sepolia ETH (v5)", "amount": "0.1", "bridge": "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA", "bridgeLabel": "bridge v5",
             "base": "100000000000000000", "decimals": 18, "definition": "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw"}
_MV_TO = "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"
_MV_PLAN = {"bridge": _MV_ASSET["bridge"], "bridge_label": "bridge v5", "definition": _MV_ASSET["definition"], "symbol": "wETH", "decimals": 18,
            "amount": "50000000000000000", "amount_shown": "0.05", "from": OWNER, "to": _MV_TO, "to_name": None, "fee_payer": OWNER, "relay_pays": False}
def mv(js=""):
    return (CARD_04 % ("[]", "({})", _MV_FEAT + '; root.againAfter(250, function() { root.openMove("%s", %s); root.againAfter(80, function() { %s }) })'
                       % (OWNER, json.dumps(_MV_ASSET), js)))
def mv_sheet(state, more=""):
    check = {"id": "", "plan": _MV_PLAN, "args": [], "state": state, "fee": {"about": "2140", "at_most": "3000"} if state == "ready" else None, "problem": ""}
    return mv('root.moveTo = "%s"; moveAmountField.text = "0.05"; root.againAfter(30, function() { root.moveCheck = %s; %s })' % (_MV_TO, json.dumps(check), more))
MOVE = [
    # The card: Move beside each wrapped token the wallet's own account holds.
    ("card-move", CARD_04 % ("[]", "({})", _MV_FEAT)),
    # The form, with the list of the wallet's accounts open (the imported one marked).
    ("move-form", mv('root.moveToOpen = true')),
    ("move-form-filled", mv('root.moveTo = "%s"; moveAmountField.text = "0.05"' % _MV_TO)),
    ("move-too-much", mv('root.moveTo = "%s"; moveAmountField.text = "0.5"' % _MV_TO)),
    ("move-checking", mv_sheet("checking")),
    ("move-sheet", mv_sheet("ready")),
    ("move-short", mv_sheet("failed", 'root.moveCheck = Object.assign({}, root.moveCheck, { problem: "CCVd76…pKnk has no test LEZ, and the fee needs about 3,000 test LEZ. Get test LEZ for it first (Get test LEZ, on the bridge test network card). Nothing was sent." })')),
    ("move-old-tool", mv_sheet("failed", 'root.moveCheck = Object.assign({}, root.moveCheck, { problem: "This wallet\'s LEZ 0.4 names tool can\'t move wrapped tokens yet: it needs a newer tool. Nothing was sent." })')),
    ("move-sending", mv_sheet("ready", 'root.moveSent = { id: "", plan: %s, state: "sending", block: null, tx: null, problem: "" }' % json.dumps(_MV_PLAN))),
    ("move-done", mv('root.moveTo = "%s"; root.moveSent = { id: "", plan: %s, state: "done", block: 28512, tx: "ab12", problem: "" }' % (_MV_TO, json.dumps(_MV_PLAN)))),
    # Review 2026-11-19: a name's account pinned at the check (and its forward); a send with no answer in time.
    ("move-sheet-name", mv('moveToField.text = "alice"; moveAmountField.text = "0.05"; root.againAfter(30, function() { root.moveCheck = %s })'
        % json.dumps({"id": "", "plan": dict(_MV_PLAN, to="alice", to_name="alice"), "args": ["b", "d", "0.05", OWNER, "alice"], "state": "ready", "fee": {"about": "2140", "at_most": "3000"}, "problem": "",
                      "to_account": _MV_TO, "forwarded_to": "bob"}))),
    ("move-unclear", mv('root.moveTo = "%s"; root.moveSent = { id: "", plan: %s, state: "unclear", block: null, tx: null, problem: Logic.MOVE_UNCLEAR }' % (_MV_TO, json.dumps(_MV_PLAN)))),
    # To an imported account: said before the review.
    ("move-to-imported", mv('moveToField.text = "%s"; moveAmountField.text = "0.05"' % OWNER).replace('root.openMove("%s"' % OWNER, 'root.openMove("%s"' % _MV_TO)),
]
for _name, _js in MOVE:
    SCENARIOS[_name] = _js
# An imported account's deposit code (item C): its notice in the card, the secret line in Details said once.
SCENARIOS["receive-code-imported-card"] = ('root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.openEthReceive(); '
    'root.againAfter(300, function() { root.receiveResult = Object.assign({}, %s, { account_notice: %s }); root.receiveDetails = true }) }) })'
    % (json.dumps(MOCKS["receive-code"]["bridge"]["make_public_deposit_request"]), json.dumps(_IMPORTED_LINE)))
HEIGHTS_EXTRA = {"receive-code-imported-card": 1700}
# Import account (item D): the form with the key pasted (shown as dots), and the line after an import.
_IMP = 'root.section = "settings"; root.loadAccounts(); root.againAfter(150, function() { root.moduleFeatures = root.moduleFeatures.concat(["account_import"]); root.againAfter(50, function() { %s }) })'
SCENARIOS["settings-account-import"] = _IMP % 'root.accountImportOpen = true; importKeyField.text = "5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1"; importLabelField.text = "Old wallet"'
SCENARIOS["settings-account-imported"] = _IMP % ('root.accountImportKind = "done"; root.accountImportLine = Logic.importResultLine({ account: "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", label: "Old wallet", notice: %s })'
                                                 % json.dumps("Your recovery phrase does not bring this account back: keep its key (or a backup of this wallet) safe. Bridged ETH claimed into it can be moved to an account of your recovery phrase with Move."))
SCENARIOS["settings-account-import-refused"] = _IMP % 'root.accountImportOpen = true; root.accountsError = "This wallet has that account already. Nothing was imported."'
HEIGHTS_EXTRA.update({"settings-account-import": 2700, "settings-account-imported": 2700, "settings-account-import-refused": 2700})
# Signed withdraw-to codes (roadmap 9): the bridge says the code is signed by its recipient, or not.
SCENARIOS["sendback-words-signed"] = SCENARIOS["sendback-words"]
MOCKS["sendback-words-signed"] = sb_mock(send_back_check=dict(SB_CHECK, signed=True, signed_by=SB_RECIPIENT, signature_line="(the module's line is not shown)"))
SCENARIOS["sendback-words-unsigned"] = SCENARIOS["sendback-words"]
MOCKS["sendback-words-unsigned"] = sb_mock(send_back_check=dict(SB_CHECK, signed=False, signed_by=None))

# ── ERC-20 as a first-class path (rfp-022 docs/ERC20-PATH.md): a bridge (added from its code) whose
# config lists USDT (6 decimals) and DAI (18) and no ETH; Receive starts on USDT at its smallest
# amount, its deposit steps say approve first; a paused token is named; Bridge to Ethereum offers USDT.
_E20_USDT = "0x" + "11" * 20
_E20_DAI = "0x" + "da" * 20
_E20_TOKENS = [{"eth_token": _E20_USDT, "symbol": "USDT", "decimals": 6, "definition": "UsdtDef1111111111111111111111111111111111111", "denoms": ["100000000", "1000000000", "10000000000"]},
               {"eth_token": _E20_DAI, "symbol": "DAI", "decimals": 18, "definition": "DaiDef11111111111111111111111111111111111111", "denoms": ["100000000000000000000", "1000000000000000000000", "10000000000000000000000"]}]
_E20_CONFIG = dict(SB_CONFIG, deployment={"name": "added-0e20", "label": "bridge e20", "bridge_program": V4, "release": "proof", "tokens": _E20_TOKENS}, features=SB_FEATURES + ["bridges"])
_E20_CODE = _copy.deepcopy(MOCKS["receive-code-words"]["bridge"]["make_public_deposit_request"])
_E20_CODE.update({"symbol": "USDT", "denom_text": "100", "total_text": "100", "bridge_fee": None, "amount_text": "100 USDT"})
_E20_CODE["deposit"]["symbol"] = "USDT"
_E20_CODE["deposit"]["eth_token"] = _E20_USDT
_E20_REG_PAUSED = {"tokens": [dict(_E20_TOKENS[0], denoms=[{"index": 0, "value": "100000000", "text": "100"}, {"index": 1, "value": "1000000000", "text": "1,000"}], freeze={"eth_deposits": False}),
                              dict(_E20_TOKENS[1], denoms=[{"index": 0, "value": "100000000000000000000", "text": "100"}], freeze={"eth_deposits": True})]}
_E20_RECEIVE = 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.bridgeConfig = %s; root.openEthReceive(); root.receivePublicOk = true; %s }) })' % (json.dumps(_E20_CONFIG), "%s")
# The bridge test network as read for this bridge: the account holds 1,500 USDT on it (so it is the
# source chip, as wETH is on bridge v4 in sendback-amount).
_E20_HELD = [{"definition": _E20_TOKENS[0]["definition"], "eth_token": _E20_USDT, "symbol": "USDT", "decimals": 6, "name": "Wrapped USDT (demo)", "bridge": V4, "bridge_label": "bridge e20"}]
_E20_NET = dict(NET04_V4, read=dict(NET04_V4["read"], tokens=_E20_HELD, bridges=[dict(NET04_V4["read"]["bridges"][0], label="bridge e20")],
    accounts=[{"account": OWNER, "found": True, "native": "0", "tokens": [dict(_E20_HELD[0], amount="1500000000")], "other_tokens": 0},
              {"account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "found": False, "native": "0", "tokens": [dict(_E20_HELD[0], amount="0")], "other_tokens": 0}]))
_E20_USDT_ASSET = '{ symbol: "USDT", name: "Wrapped USDT (demo)", amount: "1,500", bridge: "%s", base: "1500000000", decimals: 6, definition: "%s", ethToken: "%s" }' % (V4, _E20_TOKENS[0]["definition"], _E20_USDT)
_E20_SB = 'root.cliNames = []; root.cliPending = []; root.openWallet("home"); Qt.callLater(function() { root.setWalletNet("0.4"); Qt.callLater(function() { root.bridgeConfig = %s; root.loadBridgeNet(); root.openSendBack("%s", %s); root.againAfter(300, function() { root.sbDenom = -1 }) }) })' % (json.dumps(_E20_CONFIG), OWNER, _E20_USDT_ASSET)
for _name, _js, _mock in [
    # Receive on a bridge without ETH: USDT chosen, 100 USDT pre-selected.
    ("receive-erc20-choose", _E20_RECEIVE % "", {"bridge": {"config": _E20_CONFIG}}),
    # Its code: step 3 says approve exactly the amount, then deposit.
    ("receive-erc20-code", _E20_RECEIVE % "root.againAfter(300, function() { root.makeReceive() })", {"bridge": {"config": _E20_CONFIG, "make_public_deposit_request": _E20_CODE}}),
    # The registry says DAI deposits are paused: DAI is not offered and one line says so.
    ("receive-erc20-paused", _E20_RECEIVE % "", {"bridge": {"config": _E20_CONFIG, "registry": _E20_REG_PAUSED}}),
    # Bridge to Ethereum from a USDT balance: amounts in USDT's 6 decimals (shown after "Change": opening
    # pre-selects the smallest, 100 USDT, and lands on step 2).
    ("sendback-erc20-amount", _E20_SB, {"bridge": dict(sb_mock()["bridge"], config=_E20_CONFIG), "bridgeNet": _E20_NET}),
]:
    SCENARIOS[_name] = _js
    MOCKS[_name] = _mock

def _ago(text):
    """A job's start in a mock ("@AGO:n@"): n seconds before this render, in ms."""
    import re as _re, time as _time
    return _re.sub(r'"@AGO:(\d+)@"', lambda m: str(int(_time.time() * 1000) - int(m.group(1)) * 1000), text)
HEIGHTS = {name: 2100 for name in ("claim-private-approve", "claim-private-simulating", "claim-private-check-failed", "sendback-private-approve")}
HEIGHTS.update({"card-private-proving": 1500, "card-arriving-private": 1500})
HEIGHTS.update({name: 1500 for name in ("approve-burn-other-app", "approve-burn-other-app-ticked", "approve-burn-bridge-away", "sendback-approve-unticked", "sendback-approve-other-burn", "plain-markup-note", "approve-exit-recipient", "approve-exit-recipient-unknown", "sendback-approve-after-restart")})
HEIGHTS.update({name: 1700 for name, _j, _m in WALLET_NEXT if name.startswith("sendback-address") or name.startswith("sendback-approve")})
HEIGHTS.update({name: 1800 for name, _j, _m in WALLET_NEXT if name.startswith("card-remove")})
HEIGHTS.update({name: 1500 for name, _j in MOVE})
HEIGHTS.update(HEIGHTS_EXTRA)

# Collectibles (NFTs): one program on a throwaway chain, a copy on the public account, the master
# on a private one, a pending credit, and the sheets the module answers.
NFT_P = "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu"
NFT_ART = "HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5"
NFT_COLL = {"definition": NFT_ART, "name": "Harbour at dawn", "printable_supply": "3", "standard": "Simple", "uri": "https://img.example.com/harbour.png", "creators": "ada.lez"}
NFT_PROGRAMS = [{"program": NFT_P, "label": "Test chain NFTs", "added": 1, "registry": {"registry": False}}]
NFT_READ = [{"program": NFT_P, "label": "Test chain NFTs", "registry": {"registry": False}, "read_at": 1792000000,
             "holdings": [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "account_kind": "public", "state": "copy", "holds": True, "definition": NFT_ART, "owned": True, "collection": NFT_COLL},
                          {"account": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "account_kind": "private", "state": "master", "holds": True, "definition": NFT_ART, "print_balance": "2", "collection": NFT_COLL},
                          {"account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "account_kind": "public", "state": "copy", "holds": True, "definition": "9iHYqf77noB5oyGrxhL5GNVLSKcKQZeGBzp294fMi3Mv", "owned": True, "collection": None},
                          {"account": "Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2", "account_kind": "private", "state": "empty", "holds": False}],
             "pending": [{"position": 161, "definition": NFT_ART, "value": "copy", "to": "Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2"}],
             "collections": [{"definition": NFT_ART, "name": "Harbour at dawn", "readable": True, "count": 2, "accounts": []},
                             {"definition": "9iHYqf77noB5oyGrxhL5GNVLSKcKQZeGBzp294fMi3Mv", "name": None, "readable": False, "count": 1, "accounts": []}]}]
NFT_PAYERS = [{"account": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", "balance": "250000000", "enough": True},
              {"account": "Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2", "balance": "1000000", "enough": False}]
NFT_LABEL_WARN = ["The Program Registry isn't installed, so who wrote this NFT program and whether its source is verified is unknown."]
NFT_PUBLIC_PAYERS = [{"account": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "balance": "1250000", "enough": True},
                     {"account": "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR", "balance": "0", "enough": False}]
def nft_sheet(action, proven, public, private, lines, refusal=None):
    return {"action": action, "from": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z" if proven else "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "collection": NFT_COLL,
            "lines": lines, "public": public, "private": private, "proven": proven,
            "fee": {"about": "1601600", "at_most": "5070616", "inside_proof": proven} if proven else {"about": "232000", "at_most": "640000", "inside_proof": False},
            "payers": NFT_PAYERS if proven else NFT_PUBLIC_PAYERS, "refusal": refusal}
NFT_BASE = {"programs": NFT_PROGRAMS, "read": NFT_READ}
PROOF_REQ = {"id": "p1", "kind": "membership_proof", "app": "door_ui", "collection": NFT_ART, "program": NFT_P, "discloses": "may ask you to prove you hold an NFT of a collection; learns a pass id, nothing about your accounts"}
PROOF_SHEET = {"request": "p1", "app": "door_ui", "publisher": None, "first_time": True, "title": "door_ui asks you to prove you hold an NFT of Harbour at dawn.", "purpose": "Check in to the members' room",
               "public": ["That someone made a membership proof now. Not which collection, and not for which app.", "A pass id. It stays the same for this NFT's account. Apps you show it to can tell it is the same holder as before. They cannot tell who you are.", "When the pass starts and ends: until 1795003200000."],
               "not_shown": "Which NFT, which account holds it, your balances, your other NFTs.",
               "good_to_know": ["It takes about 10 minutes on this computer and uses most of its memory.", "It replaces your earlier pass for this NFT, if one is live.", "This account can't send anything until it is done.", "A fee reserve is held while it runs and mostly returned."],
               "holdings": ["Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"], "public_only": False, "live_passes": [], "fee_payer": "this NFT's account",
               "checks": {"gate": None, "gate_label": None, "nft_program": NFT_P, "nft_program_label": {"registry": False}}}
OPEN_NFT = 'root.setWalletNet("0.4"); Qt.callLater(function() { root.namesNet = "0.4"; root.openCollectibles(); %s })'
SEND_ACTION = '{ action: "send", from: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", to: "Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2" }'
for key, mock, then in [
    ("collectibles-empty", {"programs": [], "read": []}, ""),
    ("collectibles", NFT_BASE, ""),
    ("collectibles-reading", {"programs": NFT_PROGRAMS, "readError": True}, ""),
    ("collectibles-send-private", dict(NFT_BASE, warnings=NFT_LABEL_WARN, sheet=nft_sheet("send", True, ["Only that the NFT program ran is public."], ["Neither account and not the collection are named: no link between sender and recipient."], ["To Private/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2"])),
     'root.nftOpenSheet("Sending Harbour at dawn (master)", "%s", %s, null, null)' % (NFT_P, SEND_ACTION)),
    ("collectibles-send-refused", dict(NFT_BASE, sheet=nft_sheet("send", False, ["Both accounts, the collection and the NFT that moved are public."], [], ["To alice (Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR)"], "That account isn't ready for this collection: it has to receive a holding first.")),
     'root.nftOpenSheet("Sending Harbour at dawn (copy)", "%s", { action: "send", from: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", to: "alice" }, null, null)' % NFT_P),
    ("collectibles-burn", dict(NFT_BASE, sheet=nft_sheet("burn", False, ["The account, the collection and the burn are public."], [], ["Destroys this copy for good."])),
     'root.nftOpenSheet("Burning Harbour at dawn (copy)", "%s", { action: "burn", from: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk" }, null, { state: "copy" }); Qt.callLater(function() { root.nftTick("burn", true) })' % NFT_P),
    ("collectibles-receive", dict(NFT_BASE, sheet=nft_sheet("init", True, ["The collection is public; the private account is not named."], [], ["An account takes NFTs of one collection, after it says so in a small transaction."])),
     'root.nftOpenSheet("Getting an account ready for this collection", "%s", { action: "init", definition: "%s", "new": "private" }, null, null)' % (NFT_P, NFT_ART)),
    ("collectibles-ready", dict(NFT_BASE, read=[dict(NFT_READ[0], holdings=NFT_READ[0]["holdings"] + [{"account": "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg", "account_kind": "public", "state": "copy", "holds": False, "definition": NFT_ART, "owned": False, "collection": NFT_COLL}])]),
     'root.nftResult = "Ready: Public FaPur3…Zsdg takes this collection now (block 2481). Give the sender this account."; root.nftResultKind = "done"; root.nftResultCopy = "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg"'),
    ("collectibles-job", NFT_BASE, 'root.nftJob = { id: "-wallet-1", kind: "nft_send", label: "Sending Harbour at dawn (master)", started: Date.now() - 190000, proves: true, asking: false }'),
    ("collectibles-image", NFT_BASE, 'root.nftConsentFor = "%s|Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk|%s"; root.nftConsentUri = "https://img.example.com/harbour.png"' % (NFT_P, NFT_ART)),
    ("collectibles-image-shown", NFT_BASE, 'root.nftLoadImage("%s", "%s", "https://img.example.com/harbour.png")' % (NFT_P, NFT_ART)),
    ("collectibles-proof-sheet", dict(NFT_BASE, requests=[PROOF_REQ], proofSheet=PROOF_SHEET), 'root.openProofSheet("p1")'),
    ("collectibles-proof-public-only", dict(NFT_BASE, requests=[PROOF_REQ], proofSheet=dict(PROOF_SHEET, holdings=[], public_only=True)), 'root.openProofSheet("p1")'),
    ("collectibles-passes", dict(NFT_BASE, passes=[{"request": "p1", "app": "door_ui", "status": {"state": "proving", "elapsed_seconds": 245}},
                                                   {"request": "p2", "app": "door_ui", "status": {"state": "failed", "code": "GATE_NOT_PINNED", "plain_message": "The membership gate on this network isn't the pinned, unchangeable one (or none is set up yet). Nothing was sent."}}],
                                 grants=[{"app": "gallery_ui", "accounts": ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"], "capability": "nft_list", "discloses": "which NFTs these accounts hold, with their collections and metadata"},
                                         {"app": "gallery_ui", "accounts": None, "capability": "nft_transfers", "discloses": "may ask you to send an NFT from an account it can list; you approve each one"},
                                         {"app": "door_ui", "accounts": None, "capability": "membership_proof", "discloses": "may ask you to prove you hold an NFT of a collection; learns a pass id, nothing about your accounts"}]), ""),
    ("collectibles-keys", dict(NFT_BASE, read=[dict(NFT_READ[0], holdings=NFT_READ[0]["holdings"] + [{"account": "Private/HqRvXf9Lw3jQm5nYc2KpT8sDzV6bR4aE1uGhJ7kNoP2x", "account_kind": "private", "state": "copy", "holds": False, "definition": NFT_ART, "owned": False, "collection": NFT_COLL}])]),
     'root.nftShowKeys("Private/HqRvXf9Lw3jQm5nYc2KpT8sDzV6bR4aE1uGhJ7kNoP2x")'),
    ("collectibles-app-list", dict(NFT_BASE, requests=[{"id": "q1", "kind": "nft_list", "app": "gallery_ui", "reason": "to show your art on your profile", "discloses": "which NFTs these accounts hold, with their collections and metadata"}]), ""),
    ("collectibles-app-send", dict(NFT_BASE, warnings=NFT_LABEL_WARN, requests=[{"id": "q2", "kind": "nft_transfer", "app": "gallery_ui", "summary": "a gift for ada", "program": NFT_P, "from": "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", "destination": "ada", "action": '{"action":"send","from":"Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk","to":"ada"}'}],
         sheet=nft_sheet("send", False, ["Both accounts, the collection and the NFT that moved are public."], [], ["Leaving: a copy of this collection.", "To ada (Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR)"])),
     'root.nftOpenSheet("Sending an NFT for gallery_ui", "%s", { action: "send", from: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", to: "ada" }, { id: "q2", app: "gallery_ui" }, null)' % NFT_P),
]:
    MOCKS[key] = {"nft": mock}
    SCENARIOS[key] = OPEN_NFT % (("Qt.callLater(function() { " + then + " })") if then else "")
    HEIGHTS[key] = 2600
# Settings, Connected apps, with NFT capabilities (one row each, with Remove).
MOCKS["settings-nft-grants"] = MOCKS["collectibles-passes"]
SCENARIOS["settings-nft-grants"] = 'root.setWalletNet("0.4"); Qt.callLater(function() { root.namesNet = "0.4"; root.section = "settings"; root.loadAppGrants() })'
HEIGHTS["settings-nft-grants"] = 2700

def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
    filled = {"zone_b": ZONE_B, "zone_request": zone_request(True), "zone_request_gone": zone_request(False, True)}
    source = open(os.path.join(HERE, "..", "qml", "Main.qml")).read()
    marker = "Item {\n    id: root\n"
    assert marker in source, "Main.qml no longer starts the way the harness expects"
    os.makedirs(out_dir, exist_ok=True)
    for shown in wanted:
        out = os.path.join(out_dir, shown + ".png")
        # Any scenario in the light palette: "<scenario>-light" when it has no light one of its own.
        scenario = shown[:-6] if shown.endswith("-light") and shown not in SCENARIOS else shown
        bridge = BRIDGE % {
            "names": json.dumps([{"name": n, "owner": o} for n, o in NAMES]),
            "alice": ALICE, "bob": BOB, "carol": CAROL, "pages": json.dumps(PAGES), "avatar": json.dumps(JEF_PIXELS),
            "wallet": "false" if scenario.startswith("nowallet") or scenario == "cli-nowallet" else "true",
            "job": json.dumps({"id": "hopper-1792000000000", "name": "hopper", "kind": "set_text", "started": 1792000000000, "finished": 1792000190000, "exit": 1, "state": "failed",
                               "log_tail": "proving the change on this machine...\nError: the transaction was refused: out of gas"} if scenario == "cli-failed"
                              else {"id": "hopper-1792000000000", "name": "hopper", "kind": "set_text", "started": 1792000000000, "finished": None, "exit": None, "state": "running", "log_tail": "proving the change on this machine..."}), "records": json.dumps(RECORDS),
            "registry": "null" if scenario == "unset" else json.dumps("DbYLkKwd2b13bQhceVj351mpPw7QmRzEjRT2EfU56CKH"),
            "mock": _ago(json.dumps(MOCKS.get(scenario, {}))).replace('"@SOON@"', str(int(__import__("time").time()) + 1500)).replace('"@HOUR@"', str(int(__import__("time").time()) + 3400)),
            "bridge_key": "" if scenario == "get-tokens" else "8e3b0f4a2c71d95e6a0b4c8d2f1e7a3954c6b0d8e2f4a6c8b0d2e4f6a8c0b2d4",
            "home_channel": HOME_CHANNEL, "shipped_build": SHIPPED_BUILD,
            "bridge_net": json.dumps(NET04),
            "scenario": (SCENARIOS[scenario] % dict(filled, zone_request=zone_request(scenario == "approve-zone"))) if "%(zone" in SCENARIOS[scenario] else SCENARIOS[scenario], "out": out, "height": HEIGHTS[scenario] if scenario in HEIGHTS else 2700 if scenario in ("settings", "release", "page-all", "cli-settings", "cli-saving", "cli-failed", "cli-nowallet", "profile-tab", "start", "settings-tab", "register", "register-net04", "wallet-names-net04", "send-net04", "name", "settings-accounts", "settings-oldcli", "settings-account-new", "settings-network") or scenario.startswith("settings-zones") or scenario.startswith("settings-bridge") or scenario.startswith("backup") else 2000 if scenario.startswith("approve-private") or scenario.startswith("approve-relayed") else 1500 if scenario.startswith("card-sendback") or scenario.startswith("sendback-") else 1100 if scenario.startswith("approve-") and "zone" in scenario else 1500 if scenario.startswith("get-tokens") or scenario.startswith("wallet") or scenario.startswith("claim") or scenario.startswith("card-arriving") else 1100 if scenario.startswith("receive-") else 1700 if scenario.startswith("welcome") else 1000 if scenario.startswith("pay-account") else 1100 if scenario.startswith("inscribe") else 1300 if scenario in ("name", "profile", "source", "page-link", "page-text", "page-long", "nowallet") else 760,
        }
        # Beside Main.qml, so its relative imports (logic.mjs) resolve.
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=os.path.join(HERE, "..", "qml")) as handle:
            # A "-light" scenario reads the light palette directly. Never through Theme.settings:
            # that is design-system.ini, shared by every Logos app and render on the machine.
            themed = source.replace("Theme.palette", "Theme.lightTheme").replace("Theme.isDark", "false") if shown.endswith("-light") else source
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
        print(shown, "ok" if os.path.exists(out) else "FAILED", *problems[:6], sep="\n  " if problems else " ")

main()
