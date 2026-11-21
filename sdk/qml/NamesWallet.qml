// The Names Wallet SDK's QML side: Basecamp's bridge and one timer per wait, around
// NamesWalletSDK.mjs (where everything else is, and which node tests). Copy both files into your
// app's qml/ folder unchanged; then, in Main.qml:
//
//     NamesWallet { id: names; bridge: typeof logos !== "undefined" ? logos : null }
//
// and bind to names.status, names.problem, names.accounts, names.features. The calls are the
// SDK's (NamesWalletSDK.mjs documents each): check, connect, disconnect, balances, proposeCall,
// proposeTransfer, propose, watch, withdraw, resolve, reverse, call, has, open.
// Your metadata.json lists "names" in dependencies (or optional_dependencies) and
// `"uses": [{ "intent": "basecamp.apps.launch" }]`, so Names can be brought forward.

import QtQuick
import "NamesWalletSDK.mjs" as SDK

Item {
    id: wallet
    visible: false

    /// Basecamp's bridge (`logos`). Set it from your Main.qml, where `logos` is in scope.
    property var bridge: null
    /// How long one call may take before it counts as no answer (ms).
    property int patience: 30000

    /// SDK.STATES: checking, missing, otherZone, disconnected, asking, connected.
    property string status: "checking"
    property string problem: ""
    property var config: null
    property var features: []
    property var accounts: []
    property var balancesRead: []
    readonly property string version: SDK.VERSION

    /// After status, accounts, features or config change.
    signal updated()

    property var core: null
    function sdk() {
        if (core) return core
        core = SDK.create({
            call: function(method, args, callback, ms) {
                var b = wallet.bridge
                if (!b) return false
                if (b.callModuleAsync) { b.callModuleAsync("names", method, args, callback, ms || wallet.patience); return true }
                if (!b.callModule) return false
                Qt.callLater(function() {
                    var raw
                    try { raw = b.callModule("names", method, args) } catch (error) { raw = "" }
                    callback(raw)
                })
                return true
            },
            later: function(f, ms) {
                var t = oneShot.createObject(wallet, { interval: Math.max(1, ms) })
                t.then = f
                t.start()
            },
            open: function() {
                var b = wallet.bridge
                if (b && b.request) b.request("basecamp.apps.launch", { app: "names_ui" }, function() {})
            },
            changed: function(w) {
                wallet.status = w.state
                wallet.problem = w.problem
                wallet.config = w.config
                wallet.features = w.features
                wallet.accounts = w.accounts
                wallet.balancesRead = w.balances
                wallet.updated()
            }
        })
        return core
    }

    Component {
        id: oneShot
        Timer {
            property var then: null
            onTriggered: { var f = then; then = null; destroy(); if (f) f() }
        }
    }

    function call(method, args, done, failed, options) { sdk().call(method, args, done, failed, options) }
    function has(feature) { return sdk().has(feature) }
    function check(options, done) { sdk().check(options, done) }
    function balances(done, failed) { sdk().balances(done, failed) }
    function open() { sdk().open() }
    function connect(reason, options, handlers) { sdk().connect(reason, options, handlers) }
    function disconnect(done, failed) { sdk().disconnect(done, failed) }
    function watch(request, handlers) { return sdk().watch(request, handlers) }
    function withdraw(request, done, failed) { sdk().withdraw(request, done, failed) }
    function propose(method, args, handlers, options) { sdk().propose(method, args, handlers, options) }
    function proposeCall(program, accountsJson, instructionHex, summary, handlers, options) { sdk().proposeCall(program, accountsJson, instructionHex, summary, handlers, options) }
    function proposeTransfer(asset, amount, destination, handlers, options) { sdk().proposeTransfer(asset, amount, destination, handlers, options) }
    function resolve(name, done) { sdk().resolve(name, done) }
    function reverse(account, done) { sdk().reverse(account, done) }
}
