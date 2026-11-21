// Faucet: free test LEZ for an account the person picks from their Names Wallet.
//
// The wallet side is built only on the Names Wallet Provider SDK
// (docs/wallet/PROVIDER-SDK.md): connect (request_access), account selection
// (granted_accounts), and balances (account_balances) to see the LEZ arrive. Asking a
// faucet signs nothing, so there is no approval step for it; the person approves which
// accounts this app may see.
//
// The faucet is an optional service and the app says which one it contacts before anything is
// sent. Basecamp gives a QML view no network, so this app's small backend
// (src/faucet_ui_backend.cpp) makes the request; it holds no key and sends only the account ID.
//
// The look is the shared kit (./kit): one left-aligned column, at most 680 px. The steps the
// person takes, their help and the plain result words are in logic.mjs, which the Node test runs.
// All wallet and backend calls, and every argument, are unchanged from the earlier app.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic
import "kit"

Item {
    id: root
    property Look look: Look {}

    // ── Settings ─────────────────────────────────────────────────────────────────────
    property string faucetUrl: Logic.DEFAULT_FAUCET_URL
    property bool settingsOpen: false
    property var faucetInfo: null        // what the faucet says it gives: {amount, per_account}

    // ── Wallet (Provider SDK) ────────────────────────────────────────────────────────
    property var granted: []             // [{account, account_kind}]
    property bool connected: false
    property string chosen: ""           // the account to fund: Public/<id>
    property var balances: []            // [{account, balance, updating, error}]
    property string requestId: ""
    property var request: null

    // ── The faucet ───────────────────────────────────────────────────────────────────
    property bool asking: false
    property var outcome: null           // Logic.interpret(...) of the last answer, with {account, at, before}
    property var lastGrant: ({})         // account → when this app saw the faucet pay it (ms)
    property var retryAt: ({})           // account → when asking again can work (ms)
    property string problem: ""

    property real tick: Date.now()
    readonly property var backend: typeof logos !== "undefined" && logos.module ? logos.module("faucet_ui") : null
    property bool backendReady: false
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModuleAsync
    readonly property var payable: Logic.fundable(granted)
    readonly property bool hasPrivate: granted.some(function(a) { return a.account_kind === "private" })
    readonly property string urlProblem: Logic.urlProblem(faucetUrl)
    readonly property double waitUntil: chosen !== "" && retryAt[chosen] ? retryAt[chosen] : 0
    readonly property bool coolingDown: waitUntil > tick
    readonly property bool arriving: outcome !== null && outcome.ok && !arrived
    // Arrived: the balance grew past what it was when we asked (or, if it hadn't been read
    // then, reached the amount sent).
    readonly property bool arrived: outcome !== null && outcome.ok && Logic.balanceOf(balances, outcome.account) !== null
        && (outcome.before !== null ? Logic.above(Logic.balanceOf(balances, outcome.account), outcome.before)
                                    : Logic.atLeast(Logic.balanceOf(balances, outcome.account), outcome.amount || "1"))

    // ── View state ───────────────────────────────────────────────────────────────────
    property bool guideOpen: true
    property bool guideDone: false
    readonly property bool showGuide: guideOpen || !guideDone
    readonly property int step: Logic.faucetStep({ connected: connected, payable: payable,
        asking: asking, waiting: arriving, arrived: arrived })
    property string resultLine: ""
    property string nextLine: ""
    property bool resultBad: false

    readonly property string networkName: "Bridge test network"
    readonly property string networkDetail: Logic.hostOf(faucetUrl)

    // ── The names module (Provider SDK) ──────────────────────────────────────────────

    function names(method, args, done, failed) {
        if (!bridgeReady) {
            (failed || fail)("This app needs to run inside Basecamp, with the Names app installed.")
            return
        }
        logos.callModuleAsync("names", method, args, function(reply) {
            var r = null
            try { r = JSON.parse(reply) } catch (error) {}
            if (r && typeof r === "string") { try { r = JSON.parse(r) } catch (error) {} }
            if (!r || r.success === undefined)
                (failed || fail)("The names module is missing or too old: install Names 0.11.0 or later.")
            else if (!r.success)
                (failed || fail)(r.error)
            else
                done(r.value)
        }, 30000)
    }

    /// One result line, and what happens next. Only a real problem is red.
    function say(message, bad, next) {
        var t = String(message || "").trim()
        resultLine = t === "" ? "" : t.charAt(0).toUpperCase() + t.substr(1) + (/[.?!…]$/.test(t) ? "" : ".")
        resultBad = !!bad
        nextLine = bad ? "" : (next || "")
    }

    function fail(message) {
        var t = String(message).trim()
        problem = t === "" ? "" : t.charAt(0).toUpperCase() + t.substr(1) + (/[.?!]$/.test(t) ? "" : ".")
        asking = false
        say(problem, true)
    }

    /// 1. Connect: the person picks, in the Names app, which accounts this app may see.
    function connect() {
        problem = ""
        say("")
        names("request_access", ["Choose an account to receive free testnet tokens"], function(v) {
            if (v.state === "approved") {
                readGranted()
                say("Connected.", false, Logic.nextLine("connect"))
                return
            }
            requestId = v.request
            request = v
            poll.start()
        })
    }

    function readGranted() {
        names("granted_accounts", [], function(v) {
            connected = v.connected
            granted = v.accounts || []
            var ok = Logic.fundable(granted)
            if (ok.indexOf(chosen) < 0)
                chosen = ok.length > 0 ? ok[0] : ""
            readBalances()
        }, function() { connected = false })
    }

    function readBalances() {
        if (!connected) return
        names("account_balances", [], function(v) { balances = v || [] }, function() {})
    }

    function disconnect() {
        names("drop_access", [], function() { connected = false; granted = []; chosen = ""; balances = [] })
        say("Names Wallet disconnected.")
    }

    Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: root.names("request_status", [root.requestId], function(v) {
            root.request = v
            if (v.state === "pending") return
            poll.stop()
            if (v.state === "approved") { root.readGranted(); root.say("Connected.", false, Logic.nextLine("connect")) }
            else root.fail("Not connected: the request was " + v.state + ".")
        }, function(message) { poll.stop(); root.fail(message) })
    }

    // Balances: the module reads the chain at most every 15 s, so ask no faster.
    Timer {
        interval: 15000
        repeat: true
        running: root.connected && root.payable.length > 0
        onTriggered: root.readBalances()
    }

    Timer { interval: 1000; repeat: true; running: root.coolingDown || root.arriving; onTriggered: root.tick = Date.now() }

    // ── The faucet (this app's backend) ──────────────────────────────────────────────

    function backendCall(pending, done) {
        logos.watch(pending, function(text) {
            var reply = null
            try { reply = JSON.parse(text) } catch (error) {}
            done(reply || { status: 0, error: "the backend sent an unreadable answer" })
        }, function(error) { done({ status: 0, error: String(error) }) })
    }

    function describe() {
        faucetInfo = null
        if (!backend || !backendReady || urlProblem !== "") return
        backendCall(backend.describe(faucetUrl), function(reply) { faucetInfo = Logic.describe(reply) })
    }

    /// 3. Asks the faucet to pay the chosen account; reports what came of it.
    function ask() {
        problem = ""
        say("")
        if (urlProblem !== "") { problem = urlProblem; say(problem, true); return }
        if (chosen === "") { problem = "Choose the account that receives the LEZ."; say(problem, true); return }
        if (!backend || !backendReady) { problem = "This app's faucet client isn't running yet; try again in a moment."; say(problem, true); return }
        var account = chosen
        var before = Logic.balanceOf(balances, account)
        asking = true
        backendCall(backend.requestFunds(faucetUrl, Logic.bare(account)), function(reply) {
            asking = false
            var now = Date.now()
            var o = Logic.interpret(reply, now, lastGrant[account] || 0)
            o.account = account
            o.at = now
            o.before = before
            outcome = o
            var grants = Object.assign({}, lastGrant), waits = Object.assign({}, retryAt)
            if (o.ok) {
                grants[account] = now
                waits[account] = now + 24 * 60 * 60 * 1000   // the faucet pays an account once a day
            } else if (o.retryAt) {
                waits[account] = o.retryAt
            }
            lastGrant = grants
            retryAt = waits
            tick = now
            say(o.message, !o.ok, o.ok ? Logic.nextLine("ask") : "")
            readBalances()
        })
    }

    function finishGuide() {
        guideOpen = false
        guideDone = true
    }

    /// Copy a full id or hash, with a plain result line. The screen shows only the short form.
    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.")
    }

    TextEdit { id: clipboard; visible: false }

    Component.onCompleted: {
        backendReady = backend !== null && logos.isViewModuleReady && logos.isViewModuleReady("faucet_ui")
        readGranted()
        describe()
    }

    Connections {
        target: typeof logos !== "undefined" ? logos : null
        ignoreUnknownSignals: true
        function onViewModuleReadyChanged(moduleName, isReady) {
            if (moduleName === "faucet_ui") {
                root.backendReady = isReady && root.backend !== null
                if (root.backendReady) root.describe()
            }
        }
    }

    // ── Screen ───────────────────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: look.slate }

    Flickable {
        id: page
        anchors.fill: parent
        contentHeight: column.implicitHeight + 80
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: column
            x: root.width >= 600 ? look.edge : 24
            y: 40
            width: Math.min(root.width - 2 * x, look.pageWidth)
            spacing: 16

            RowLayout {
                Layout.fillWidth: true
                spacing: 16
                PageHeader {
                    look: root.look
                    title: "Faucet"
                    sentence: "Get free test LEZ on the Logos network."
                    runsOnPrefix: "Runs on"
                    network: root.networkName
                    networkDetail: root.networkDetail
                }
                Link {
                    look: root.look
                    font.underline: true
                    Layout.alignment: Qt.AlignTop
                    text: root.showGuide ? "Hide steps" : "Steps"
                    onPressed: root.showGuide ? root.finishGuide() : root.guideOpen = true
                }
            }

            // ── The steps, with the current one marked and its help under it ──────────
            Card {
                visible: root.showGuide && !root.settingsOpen
                Heading { text: "How it works" }
                Repeater {
                    model: Logic.guideSteps(root.step)
                    delegate: RowLayout {
                        id: stepRow
                        required property var modelData
                        readonly property bool now: modelData.state === "now"
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 12
                        Rectangle {
                            Layout.alignment: Qt.AlignTop
                            Layout.preferredWidth: look.px(24); Layout.preferredHeight: look.px(24); radius: width / 2
                            color: stepRow.now ? look.brass : "transparent"
                            border.width: 1
                            border.color: stepRow.modelData.state === "later" ? look.line : look.brass
                            Text {
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: stepRow.modelData.state === "done" ? "✓" : String(stepRow.modelData.n)
                                color: stepRow.now ? look.slate : look.ink
                                font.family: look.sans
                                font.pixelSize: look.px(13)
                                font.weight: Font.DemiBold
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                textFormat: Text.PlainText
                                Layout.fillWidth: true
                                text: stepRow.modelData.text
                                color: stepRow.modelData.state === "later" ? look.dim : look.ink
                                font.family: look.sans
                                font.pixelSize: look.px(16)
                                font.weight: stepRow.now ? Font.DemiBold : Font.Normal
                                wrapMode: Text.Wrap
                            }
                            Body {
                                visible: stepRow.now
                                text: stepRow.modelData.n === 1 && root.request !== null && root.request.state === "pending"
                                    ? "Names Wallet is open with the request: tick the accounts you use for Faucet and approve. You come back here after."
                                    : stepRow.modelData.detail
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 1
                                look: root.look; strong: true; changes: true; busy: root.asking
                                label: "Connect"; busyLabel: "Waiting for you in Names…"
                                working: root.request !== null && root.request.state === "pending"
                                enabledNow: !(root.request !== null && root.request.state === "pending")
                                onPressed: root.connect()
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 2 && root.payable.length === 0
                                look: root.look; strong: true
                                label: "Add an account"; busyLabel: "Waiting for you in Names…"
                                working: root.request !== null && root.request.state === "pending"
                                enabledNow: !(root.request !== null && root.request.state === "pending")
                                onPressed: root.connect()
                            }
                            Body {
                                visible: stepRow.now && stepRow.modelData.n === 3 && root.payable.length > 0
                                text: "The account to pay is " + Logic.short(root.chosen) + "."
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 3
                                look: root.look; strong: true; changes: true; busy: root.asking
                                label: root.coolingDown ? "Try again " + Logic.waitText(root.waitUntil - root.tick, root.waitUntil) : "Request test LEZ"
                                busyLabel: "Requesting…"; working: root.asking
                                enabledNow: !root.asking && !root.coolingDown && root.chosen !== "" && root.urlProblem === ""
                                onPressed: root.ask()
                            }
                            Link {
                                visible: stepRow.now && stepRow.modelData.n === 4
                                look: root.look
                                font.underline: true
                                text: root.arriving ? "Not arrived yet? Check the balance" : "Check the balance"
                                onPressed: root.readBalances()
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.topMargin: 4
                    spacing: 20
                    Action { look: root.look; label: "Got it"; visible: root.connected; onPressed: root.finishGuide() }
                }
            }

            // ── Service disclosure ────────────────────────────────────────────────────
            Card {
                visible: !root.settingsOpen
                Heading { text: "An optional service" }
                Body {
                    text: "This app asks the faucet at " + root.faucetUrl + " (" + root.networkDetail + ")"
                        + (root.faucetUrl === Logic.DEFAULT_FAUCET_URL ? ", the Names faucet run by the Logos Names project for the Logos test network." : ", a faucet you entered in Settings.")
                        + " It sends that service only the account ID you choose; the service also sees your internet address."
                        + " Nothing is sent until you press Request test LEZ."
                }
                Body {
                    text: "You don't need it: bridge LEZ from the Logos Blockchain in the Names app, or have someone pay your account."
                        + (root.faucetInfo && root.faucetInfo.amount ? " This faucet gives " + Logic.grouped(root.faucetInfo.amount) + " LEZ, " + (root.faucetInfo.per_account || "once a day") + " per account and per internet address." : " The Names faucet pays each account and each internet address once a day.")
                }
                RowLayout {
                    visible: !!(root.outcome && root.outcome.tx)
                    Layout.fillWidth: true
                    spacing: 16
                    Text {
                        textFormat: Text.PlainText
                        text: "Transaction"
                        color: look.dim
                        font.family: look.sans
                        font.pixelSize: look.px(14)
                        Layout.preferredWidth: look.px(132)
                        Layout.alignment: Qt.AlignTop
                    }
                    Text {
                        textFormat: Text.PlainText
                        text: root.outcome && root.outcome.tx ? Logic.short(root.outcome.tx) : ""
                        color: look.ink
                        font.family: look.mono
                        font.pixelSize: look.px(13)
                    }
                    Link {
                        look: root.look
                        font.underline: true
                        text: "Copy"
                        onPressed: root.copy((root.outcome && root.outcome.tx) || "", "Transaction")
                    }
                }
            }

            // ── Account selection ─────────────────────────────────────────────────────
            Card {
                visible: root.connected && !root.settingsOpen
                Heading { text: "Accounts that may receive LEZ" }
                Body {
                    visible: root.payable.length === 0
                    text: "The faucet pays public accounts. Add a public account in Names Wallet."
                }
                Body {
                    visible: root.hasPrivate
                    text: "Private accounts can't be paid by a faucet. Have a public account paid, then move LEZ to private in the Names app."
                }
                Repeater {
                    model: root.payable
                    delegate: Choice {
                        required property string modelData
                        Layout.fillWidth: true
                        label: modelData
                        detail: {
                            var b = Logic.balanceOf(root.balances, modelData)
                            return b === null ? "balance not read yet" : Logic.grouped(b) + " LEZ"
                        }
                        chosen: root.chosen === modelData
                        onPressed: root.chosen = modelData
                    }
                }
                Action {
                    visible: root.payable.length === 0 && !root.showGuide
                    look: root.look; strong: true; changes: true; busy: root.asking
                    label: "Add an account"; busyLabel: "Waiting for you in Names…"
                    working: root.request !== null && root.request.state === "pending"
                    enabledNow: !(root.request !== null && root.request.state === "pending")
                    onPressed: root.connect()
                }
                Link { look: root.look; font.underline: true; visible: root.connected && root.payable.length > 0; text: "Add an account"; onPressed: root.connect() }
                Link { look: root.look; font.underline: true; visible: root.connected; text: "Disconnect Names Wallet"; onPressed: root.disconnect() }
            }

            // ── Ask (the main action when the steps are hidden) ──────────────────────
            Card {
                visible: root.connected && root.payable.length > 0 && !root.settingsOpen && !root.showGuide
                Heading { text: "Ask for test LEZ" }
                Body { text: "The Names faucet sends free test LEZ to " + Logic.short(root.chosen) + ". It sends only the account ID; nothing is signed." }
                Action {
                    look: root.look; strong: true; changes: true; busy: root.asking
                    label: root.coolingDown ? "Try again " + Logic.waitText(root.waitUntil - root.tick, root.waitUntil) : "Request test LEZ"
                    busyLabel: "Requesting…"; working: root.asking
                    enabledNow: !root.asking && !root.coolingDown && root.chosen !== "" && root.urlProblem === ""
                    onPressed: root.ask()
                }
            }

            // ── Settings ──────────────────────────────────────────────────────────────
            Card {
                visible: root.settingsOpen
                Heading { text: "Faucet address" }
                Body { text: "Any faucet that takes a POST with an account ID works, such as an official Logos faucet once there is one." }
                Field {
                    id: urlField
                    look: root.look
                    Layout.fillWidth: true
                    code: true
                    text: root.faucetUrl
                    hint: Logic.DEFAULT_FAUCET_URL
                    onAccepted: { root.faucetUrl = text.trim(); root.describe() }
                }
                Body {
                    visible: root.urlProblem !== ""
                    text: root.urlProblem
                    color: look.alarm
                }
                RowLayout {
                    spacing: 8
                    Action {
                        look: root.look; strong: true
                        label: "Use this address"; busyLabel: "Checking…"
                        working: false
                        onPressed: { root.faucetUrl = urlField.text.trim(); root.describe() }
                    }
                    Link { look: root.look; font.underline: true; text: "Use the Names faucet"; onPressed: { root.faucetUrl = Logic.DEFAULT_FAUCET_URL; urlField.text = root.faucetUrl; root.describe() } }
                }
            }

            TechDetails {
                visible: !root.settingsOpen
                look: root.look
                title: "Technical details"
                summary: root.faucetUrl ? Logic.hostOf(root.faucetUrl) : ""
                FactRow { look: root.look; key: "Faucet"; value: root.faucetUrl; monospace: true }
                FactRow { look: root.look; key: "What it receives"; value: "Only the public account ID you choose, never a key."; }
                FactRow { look: root.look; key: "Balance reads"; value: "At most once every 15 seconds."; }
            }

            // The plain result of the last step, and what happens next.
            Text {
                textFormat: Text.PlainText
                visible: root.resultLine !== ""
                Layout.fillWidth: true
                text: root.resultLine
                color: root.resultBad ? look.alarm : look.ink
                font.family: look.sans
                font.pixelSize: look.px(16)
                font.weight: Font.Medium
                wrapMode: Text.Wrap
            }
            Text {
                textFormat: Text.PlainText
                visible: root.nextLine !== ""
                Layout.fillWidth: true
                text: "What happens next: " + root.nextLine
                color: look.dim
                font.family: look.sans
                font.pixelSize: look.px(15)
                wrapMode: Text.Wrap
            }
        }
    }

    // ── Parts of this screen (the shared ones are in ./kit) ──────────────────────────

    component Card: Rectangle {
        default property alias content: cardColumn.data
        Layout.fillWidth: true
        implicitHeight: cardColumn.implicitHeight + 36
        radius: 8
        color: root.look.card
        border.width: 1
        border.color: root.look.line
        ColumnLayout {
            id: cardColumn
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 18 }
            spacing: 10
        }
    }

    component Heading: Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        color: root.look.ink
        font.family: root.look.sans
        font.pixelSize: root.look.px(19)
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
    }

    component Body: Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        color: root.look.dim
        font.family: root.look.sans
        font.pixelSize: root.look.px(15)
        lineHeight: 1.2
        wrapMode: Text.Wrap
    }

    component Choice: Rectangle {
        id: choice
        property string label: ""
        property string detail: ""
        property bool chosen: false
        signal pressed()
        implicitHeight: 42
        radius: 6
        color: choiceArea.containsMouse ? root.look.raised : "transparent"
        border.width: 1
        border.color: chosen ? root.look.brass : root.look.line
        activeFocusOnTab: true
        Keys.onSpacePressed: choice.pressed()
        RowLayout {
            anchors { fill: parent; leftMargin: 12; rightMargin: 12 }
            spacing: 10
            Rectangle {
                implicitWidth: 14; implicitHeight: 14; radius: 7
                color: choice.chosen ? root.look.brass : "transparent"
                border.width: 1
                border.color: choice.chosen ? root.look.brass : root.look.dim
            }
            Text { textFormat: Text.PlainText; Layout.fillWidth: true; text: choice.label; color: root.look.ink; font.family: root.look.sans; font.pixelSize: root.look.px(15); elide: Text.ElideRight }
            Text { textFormat: Text.PlainText; text: choice.detail; color: root.look.dim; font.family: root.look.sans; font.pixelSize: root.look.px(14) }
        }
        MouseArea { id: choiceArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: choice.pressed() }
    }
}
