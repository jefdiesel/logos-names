// The Names Wallet SDK's QML side: Basecamp's bridge, one timer per wait, and this app's
// remembered choice of wallet, around NamesWalletSDK.mjs (where everything else is, and which
// node tests). Copy both files into your app's qml/ folder unchanged; then, in Main.qml:
//
//     NamesWallet { id: wallet; app: "my_app_ui"; bridge: typeof logos !== "undefined" ? logos : null }
//
// and bind to wallet.status, .problem, .accounts, .features, .wallets, .current. The calls are
// the SDK's (NamesWalletSDK.mjs documents each): listWallets, getWallet, chooseWallet,
// forgetWallet, check, connect, disconnect, balances, proposeCall, proposeTransfer, propose,
// watch, withdraw, resolve, reverse, call, has, open. Your app never names a wallet module.
// Your metadata.json declares `"uses": [{ "intent": "basecamp.apps.launch" }]`, so the wallet
// can be brought forward.

import QtQuick
import "NamesWalletSDK.mjs" as SDK

Item {
    id: wallet
    visible: false

    /// Basecamp's bridge (`logos`). Set it from your Main.qml, where `logos` is in scope.
    property var bridge: null
    /// Your app's module name (metadata.json `name`): the key of its remembered wallet.
    property string app: ""
    /// How long one call may take before it counts as no answer (ms).
    property int patience: 30000

    /// SDK.STATES: checking, none, choose, missing, unsupported, otherZone, disconnected, asking, connected.
    property string status: "checking"
    property string problem: ""
    property var config: null
    property var features: []
    property var accounts: []
    property var balancesRead: []
    /// The installed wallets found (`[{module, display_name, app}]`); the person picks one
    /// when `status` is "choose".
    property var wallets: []
    /// The wallet in use: `{module, display_name, app, sdk_version}`, or null.
    property var current: null
    readonly property string version: SDK.VERSION

    /// After status, current, wallets, accounts, features or config change.
    signal updated()

    property var core: null
    property var chosenNow: ({})
    // QtQuick.LocalStorage, made only when a choice is read or kept (several wallets), so an
    // engine without it still loads this file: the choice then lasts until the app closes.
    property var store: null
    function memory() {
        try {
            if (!store) store = Qt.createQmlObject('import QtQuick; import QtQuick.LocalStorage; QtObject { function db() { return LocalStorage.openDatabaseSync("NamesWalletSDK", "1", "The wallet each app uses", 4096) } }', wallet, "NamesWalletMemory")
            return store.db()
        } catch (error) { return null }
    }
    function sdk() {
        if (core) return core
        core = SDK.create({
            call: function(module, method, args, callback, ms) {
                var b = wallet.bridge
                if (!b) return false
                if (b.callModuleAsync) { b.callModuleAsync(module, method, args, callback, ms || wallet.patience); return true }
                if (!b.callModule) return false
                Qt.callLater(function() {
                    var raw
                    try { raw = b.callModule(module, method, args) } catch (error) { raw = "" }
                    callback(raw)
                })
                return true
            },
            later: function(f, ms) {
                var t = oneShot.createObject(wallet, { interval: Math.max(1, ms) })
                t.then = f
                t.start()
            },
            open: function(appName) {
                var b = wallet.bridge
                if (b && b.request && appName) b.request("basecamp.apps.launch", { app: appName }, function() {})
            },
            recall: function(key) {
                var k = wallet.app + "/" + key
                if (wallet.chosenNow[k] !== undefined) return wallet.chosenNow[k]
                var db = wallet.memory()
                var found = ""
                if (db) try {
                    db.transaction(function(tx) {
                        tx.executeSql("CREATE TABLE IF NOT EXISTS kept(k TEXT PRIMARY KEY, v TEXT)")
                        var rows = tx.executeSql("SELECT v FROM kept WHERE k = ?", [k]).rows
                        if (rows.length) found = String(rows.item(0).v)
                    })
                } catch (error) {}
                return found
            },
            remember: function(key, value) {
                var k = wallet.app + "/" + key
                var now = wallet.chosenNow
                now[k] = String(value)
                wallet.chosenNow = now
                var db = wallet.memory()
                if (db) try {
                    db.transaction(function(tx) {
                        tx.executeSql("CREATE TABLE IF NOT EXISTS kept(k TEXT PRIMARY KEY, v TEXT)")
                        tx.executeSql("INSERT OR REPLACE INTO kept(k, v) VALUES(?, ?)", [k, String(value)])
                    })
                } catch (error) {}
            },
            changed: function(w) {
                wallet.status = w.state
                wallet.problem = w.problem
                wallet.config = w.config
                wallet.features = w.features
                wallet.accounts = w.accounts
                wallet.balancesRead = w.lastBalances
                wallet.wallets = w.wallets
                wallet.current = w.current
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

    function listWallets(done) { sdk().listWallets(done) }
    function getWallet(done) { sdk().getWallet(done) }
    function chooseWallet(module) { return sdk().chooseWallet(module) }
    function forgetWallet() { sdk().forgetWallet() }
    function call(method, args, done, failed, options) { sdk().call(method, args, done, failed, options) }
    function has(feature) { return sdk().has(feature) }
    function check(options, done) { sdk().check(options, done) }
    function balances(done, failed) { sdk().balances(done, failed) }
    function open() { sdk().open() }
    function connect(reason, options, handlers) { sdk().connect(reason, options, handlers) }
    function disconnect(done, failed) { sdk().disconnect(done, failed) }
    function watch(request, handlers, options) { return sdk().watch(request, handlers, options) }
    function withdraw(request, done, failed, cancelMethod) { sdk().withdraw(request, done, failed, cancelMethod) }
    function propose(method, args, handlers, options) { sdk().propose(method, args, handlers, options) }
    function proposeCall(program, accountsJson, instructionHex, summary, handlers, options) { sdk().proposeCall(program, accountsJson, instructionHex, summary, handlers, options) }
    function proposeTransfer(asset, amount, destination, handlers, options) { sdk().proposeTransfer(asset, amount, destination, handlers, options) }
    function resolve(name, done) { sdk().resolve(name, done) }
    function reverse(account, done) { sdk().reverse(account, done) }
}
