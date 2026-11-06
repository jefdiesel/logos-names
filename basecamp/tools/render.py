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
    property var logos: QtObject {
        function ok(value) { return JSON.stringify({ success: true, value: value, error: null }) }
        function callModuleAsync(module, method, args, callback, timeout) {
            var reply = callModule(module, method, args)
            Qt.callLater(function() { callback(reply) })
        }
        function callModule(module, method, args) {
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
            if (method === "get_config") return JSON.stringify({ registry: %(registry)s, sequencer: "http://127.0.0.1:3040", wallet_config: "/Users/you/wallet/wallet_config.json", wallet_storage: "/Users/you/wallet/storage.json", cli: "/Users/you/.local/bin/names", cli_wallet: "/Users/you/.lee/wallet", bridge_key: "%(bridge_key)s", bridge_node: null, module_version: root.neededModule })
            if (method === "normalize") return String(args[0]).toLowerCase().replace(/\.lez$/, "")
            if (method === "mine") return ok({ names: names.filter(function(entry) { return entry.owner === "%(alice)s" }).map(function(entry) {
                return { name: entry.name, display: entry.name + ".lez", owner: entry.owner, expires: null, never: true, lapsed: false } }), pending: [] })
            if (method === "keep") return ok(String(args[0]))
            // hopper.lez and ada.lez are held privately by the names CLI's wallet.
            if (method === "cli_mine") return ok(["hopper", "ada"].map(function(name) {
                return { name: name, display: name + ".lez", signer: "cli", state: "held by a deposit (no expiry)", lapsed: false, registering: false, failed: false, private: true } }).concat(
                [{ name: "knuth", display: "knuth.lez", signer: "cli", state: "registering, the last step starts in about 9 minutes", lapsed: false, registering: true, failed: false }]))
            if (method === "cli_set_text" || method === "cli_profile" || method === "cli_set_page" || method === "cli_register")
                return ok({ id: args[0] + "-1792000000000", name: args[0], kind: method.substr(4), state: "running", started: Date.now() - 125000 })
            var mock = %(mock)s
            if (method === "cli_balance") {
                if (mock.balance === null) return JSON.stringify({ success: false, value: null, error: "there is no wallet in /Users/you/.lee/wallet yet" })
                return ok({ account: "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", balance: mock.private !== undefined ? mock.private : root.missing === "pauper" ? "5000" : "250000000",
                    public: { account: "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", balance: mock.public !== undefined ? mock.public : "1250000" },
                    waiting: mock.waiting || null })
            }
            if (method === "activity") return ok(mock.activity || [
                { id: "a1", at: Date.now() - 2 * 3600000, kind: "bridge", chain: "logos", from: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", to: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", amount: "1000000", state: "in_block" },
                { id: "a2", at: Date.now() - 2 * 3600000 - 120000, kind: "split", chain: "logos", from: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", amount: "1000000", fee: "2018", state: "in_block" },
                { id: "a3", at: Date.now() - 26 * 3600000, kind: "pay", chain: "lez", name: "ada", amount: "250000", fee: "4088", state: "done", job: "ada-1" },
                { id: "a4", at: Date.now() - 30 * 3600000, kind: "arrived", chain: "lez", to: "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z", amount: "5000000", state: "arrived" },
                { id: "a5", at: Date.now() - 50 * 3600000, kind: "pay_account", chain: "lez", to: "Public/FaPur3GjBrXZoE5Yz6bNjLeqXJzEn6pzXEdE9MCJZsdg", amount: "1000", state: "failed" }])
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
            if (method === "watched_tokens") return ok([{ definition: "S1LVERdef1nition1111111111111111111111111111", name: "Silver" }])
            if (method === "watch_token") return ok([{ definition: "S1LVERdef1nition1111111111111111111111111111", name: "Silver" }, { definition: args[0], name: "Copper" }])
            if (method === "chain_status") return mock.nodeDown ? JSON.stringify({ success: false, value: null, error: "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app." })
                : ok({ height: 66467, slot: 2286493, lib_slot: 2247925, mode: "Online", epoch: 7 })
            if (method === "chain_accounts") return mock.nodeDown ? JSON.stringify({ success: false, value: null, error: "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app." }) : ok([
                { key: "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b", balance: "1191220464" },
                { key: "73058ad5edb0e72b66c3e05ae90471382c3bb4d61314db995ef40e674dbf591e", balance: "25000000" },
                { key: "5f9f8fd1696c6e6d85bb52fc4b61810a3f9c75422612b72a566ff3ef6a68ad12", balance: "0" }])
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

SCENARIOS = {
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
    "wallet-claim": 'root.openWallet("home")',
    "wallet-account-low": 'root.openWallet("home"); Qt.callLater(function() { root.openAccount(root.walletGroups[0].rows[1]) })',
    "wallet-account-main": 'root.openWallet("home"); Qt.callLater(function() { root.openAccount(root.walletGroups[0].rows[0]) })',
    "wallet-account-chain": 'root.openWallet("home"); Qt.callLater(function() { var g = root.walletGroups.filter(function(x) { return x.id === "chain" || (x.rows[0] && x.rows[0].kind === "chain") })[0]; root.openAccount(g.rows[0]) })',
    "wallet-activity-page": 'root.go("activity"); root.loadActivity()',
    "wallet-row-history": 'root.openWallet("home"); root.walletRowOpen = "chain-66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b"',
    "wallet-bridge-private": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeToPrivate = true; bridgeAmountField.text = "1000000" })',
    "wallet-bridge-from-lez": 'root.openWallet("home"); Qt.callLater(function() { root.openBridgeFromLez("Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk") })',
    "wallet-bridge-plain": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens() })',
    "wallet-bridge-open": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeFromOpen = true })',
    "wallet-bridge-open-to": 'root.openWallet("home"); Qt.callLater(function() { root.openGetTokens(); root.bridgeToOpen = true })',
    "settings-network": 'root.section = "settings"; root.settingsOpen = true; sequencerField.text = "https://other.example"; root.networkConfirm = true',
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
    "welcome-register": 'root.cliNames = []; root.openRegister(); root.checkRegister("penisface")',
    "register-check": 'root.openRegister(); root.checkRegister("penisface")',
    "home-thread": 'root.boardShow("thread", "jef/0")',
    "name": 'root.show("jef")',
    "plain": 'root.show("linus")',
    "register": 'root.show("satoshi")',
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
    "approve-verified": 'root.appRequests = [{ id: "r1", kind: "connect", app: "forumer_ui", reason: "to post on the forum as you", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "verified", publisher: "forumer", module: "forumer_ui", version: "1.2.0" } }]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "approve-fake": 'root.appRequests = [{ id: "r1", kind: "connect", app: "forumer_ui", reason: "to post on the forum as you", created: Date.now(), expires: Date.now() + 600000, choices: [{ account: "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk", label: "Names wallet, main public account", granted: false }], publisher: { status: "mismatch", publisher: "forumer", module: "forumer_ui", version: "1.2.0" } }]; root.appChosen = ["Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk"]; root.appChosenFor = "r1"',
    "settings-oldcli": 'root.section = "settings"; root.loadAccounts()',
    "settings-accounts": 'root.section = "settings"; root.loadAccounts(); root.accountRenaming = "Public/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR"',
    "settings-account-new": 'root.section = "settings"; root.loadAccounts(); root.accountNewKind = "private"',
}

# What the module's stand-in says in each Welcome state (see `mock` in BRIDGE).
NO_WALLET = {"status": {"wallet": False, "public": None, "private": None}, "balance": None}
MOCKS = {
    "welcome-install": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-terminal": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-installing": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None},
    "welcome-install-failed": {"status": {"cli": None, "wallet": False, "public": None, "private": None}, "balance": None,
                               "installJob": {"id": "-install-1792000000000", "name": "-install", "kind": "install", "state": "failed", "exit": 6,
                                              "started": 1792000000000, "finished": 1792000004000, "log_tail": "Downloading names for macos-arm64...\ncurl: (6) Could not resolve host: github.com",
                                              "problem": "Couldn't reach the internet. Check your connection, then try again."}},
    "welcome-wallet": NO_WALLET,
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
    "wallet-claim": {"waiting": {"count": "2", "total": "4000000"}},
    "wallet-bridge-exact": {"exact": True},
    "wallet-bridge-nochannel": {"exact": True, "noChannel": True},
    "wallet-bridge-confirm": {"exact": True},
    "wallet-bridge-feeshort": {"exact": True, "feeShort": True},
    "settings-oldcli": {"status": {"outdated": True}, "accountsError": "Your names program is older than this app and can't do this yet: it comes with the LEZ 0.4 update. The rest of the app still works."},
    "wallet-waiting": {"tokenPending": [{"position": 21, "definition": "GoLDdef1nition11111111111111111111111111111", "name": "Gold", "amount": "50", "to": "Private/yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z"}]},
    "wallet-sending": {"private": "250000000", "public": "1250000"},
}

def main():
    qml, out_dir = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(SCENARIOS)
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
            "scenario": SCENARIOS[scenario], "out": out, "height": 2700 if scenario in ("settings", "release", "page-all", "cli-settings", "cli-saving", "cli-failed", "cli-nowallet", "profile-tab", "start", "settings-tab", "register", "name", "settings-accounts", "settings-oldcli", "settings-account-new", "settings-network") else 1500 if scenario.startswith("get-tokens") or scenario.startswith("wallet") else 1700 if scenario.startswith("welcome") else 1000 if scenario.startswith("pay-account") else 1300 if scenario in ("name", "profile", "source", "page-link", "page-text", "page-long", "nowallet") else 760,
        }
        # Beside Main.qml, so its relative imports (logic.mjs) resolve.
        with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=os.path.join(HERE, "..", "qml")) as handle:
            handle.write(source.replace(marker, marker + bridge, 1))
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
