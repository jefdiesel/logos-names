// Faucet: free LEZ testnet tokens for an account the person picks from their Names Wallet.
//
// The wallet side is built only on the Names Wallet Provider SDK
// (docs/wallet/PROVIDER-SDK.md): connect (request_access), account selection
// (granted_accounts), and balances (account_balances) to see the tokens arrive. Asking a
// faucet signs nothing, so there is no approval step for it; the person approves which
// accounts this app may see.
//
// The faucet is an optional third-party service, and the app says which one it contacts
// before anything is sent. Basecamp gives a QML view no network, so this app's small
// backend (src/faucet_ui_backend.cpp) makes the request; it holds no key and sends only
// the account ID.
//
// Controls are drawn here rather than taken from Qt Quick Controls, as in the Names app.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic

Item {
    id: root

    // ── Settings ────────────────────────────────────────────────────────────
    property string faucetUrl: Logic.DEFAULT_FAUCET_URL
    property bool settingsOpen: false
    property var faucetInfo: null        // what the faucet says it gives: {amount, per_account}

    // ── Wallet (Provider SDK) ───────────────────────────────────────────────
    property var granted: []             // [{account, account_kind}]
    property bool connected: false
    property string chosen: ""           // the account to fund: Public/<id>
    property var balances: []            // [{account, balance, updating, error}]
    property string requestId: ""
    property var request: null

    // ── The faucet ──────────────────────────────────────────────────────────
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
    readonly property bool arrived: outcome !== null && outcome.ok && outcome.before !== null
        && Logic.balanceOf(balances, outcome.account) !== null && Logic.above(Logic.balanceOf(balances, outcome.account), outcome.before)

    // ── Palette (the shared Logos theme, as the Names app uses it) ─────────
    readonly property color slate: Theme.palette.background
    readonly property color card: Theme.isDark ? Theme.palette.backgroundSecondary : Theme.palette.surfaceRaised
    readonly property color raised: Theme.isDark ? Theme.palette.surface : Theme.palette.backgroundButton
    readonly property color line: Theme.palette.border
    readonly property color ink: Theme.palette.text
    readonly property color dim: Theme.palette.textSecondary
    readonly property color faint: Theme.palette.textPlaceholder
    readonly property color brass: Theme.isDark ? Theme.palette.warning : Theme.palette.primary
    readonly property color alarm: Theme.palette.error
    readonly property color good: Theme.palette.success
    readonly property string sans: "Avenir Next"
    readonly property string mono: "Menlo"
    function px(size) { return Math.round(size * (Theme.typography.scale || 1)) }

    // ── The names module (Provider SDK) ─────────────────────────────────────

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

    function fail(message) {
        var t = String(message).trim()
        problem = t === "" ? "" : t.charAt(0).toUpperCase() + t.substr(1) + (/[.?!]$/.test(t) ? "" : ".")
        asking = false
    }

    /// 1. Connect: the person picks, in the Names app, which accounts this app may see.
    function connect() {
        problem = ""
        names("request_access", ["Choose an account to receive free testnet tokens"], function(v) {
            if (v.state === "approved") {
                readGranted()
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
    }

    Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: root.names("request_status", [root.requestId], function(v) {
            root.request = v
            if (v.state === "pending") return
            poll.stop()
            if (v.state === "approved") root.readGranted()
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

    // ── The faucet (this app's backend) ────────────────────────────────────

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
        if (urlProblem !== "") { problem = urlProblem; return }
        if (chosen === "") { problem = "Choose the account that receives the tokens."; return }
        if (!backend || !backendReady) { problem = "This app's faucet client isn't running yet; try again in a moment."; return }
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
            readBalances()
        })
    }

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

    // ── View ────────────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    Flickable {
        id: page
        anchors.fill: parent
        contentHeight: column.implicitHeight + 64
        clip: true

        ColumnLayout {
            id: column
            width: Math.min(page.width - 48, 720)
            x: (page.width - width) / 2
            y: 32
            spacing: 18

            RowLayout {
                Layout.fillWidth: true
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Text {
                        text: "Faucet"
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(28)
                        font.weight: Font.Bold
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Free LEZ testnet tokens, paid to an account you choose from your Names Wallet."
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(16)
                        wrapMode: Text.Wrap
                    }
                }
                Link { label: root.settingsOpen ? "Close settings" : "Settings"; onPressed: root.settingsOpen = !root.settingsOpen }
            }

            // Which service this app contacts, before anything is sent.
            Panel {
                title: "An optional service"
                Hint {
                    text: "This app asks the faucet at " + root.faucetUrl + " (" + Logic.hostOf(root.faucetUrl) + ")"
                        + (root.faucetUrl === Logic.DEFAULT_FAUCET_URL ? ", the Names faucet run by the Logos Names project for the LEZ testnet." : ", a faucet you entered in Settings.")
                        + " It sends that service only the account ID you choose; the service also sees your internet address."
                        + " Nothing is sent until you press the button below."
                }
                Hint {
                    text: "You don't need it: you can bridge tokens from the Logos Blockchain in the Names app, or have someone pay your account."
                        + (root.faucetInfo && root.faucetInfo.amount ? " This faucet gives " + Logic.grouped(root.faucetInfo.amount) + " LEZ, " + (root.faucetInfo.per_account || "once a day") + " per account and per internet address." : " The Names faucet pays each account and each internet address once a day.")
                }
            }

            Panel {
                visible: root.settingsOpen
                title: "Settings"
                Hint { text: "Faucet address. Any faucet that takes POST {\"account\": \"<id>\"} works, such as an official LEZ faucet once there is one." }
                Field { Layout.fillWidth: true; text: root.faucetUrl; code: true; onEdited: { root.faucetUrl = text.trim(); root.describe() } }
                Bad { text: root.urlProblem }
                Link { label: "Use the Names faucet"; onPressed: { root.faucetUrl = Logic.DEFAULT_FAUCET_URL; root.describe() } }
            }

            // 1. Connect.
            Panel {
                title: "1. Connect your Names Wallet"
                Hint {
                    text: root.connected
                        ? "Connected. This app may see: " + root.granted.map(function(a) { return Logic.short(a.account) }).join(", ") + "."
                        : "The Names app asks you which accounts this app may see. Nothing is signed or sent."
                }
                Status {
                    visible: root.request !== null && root.request.state === "pending"
                    text: "Waiting for you to choose accounts in the Names app…"
                }
                Bad { text: root.request && /rejected|expired|cancelled/.test(root.request.state) ? "Not connected: the request was " + root.request.state + "." : "" }
                RowLayout {
                    spacing: 12
                    Action {
                        label: root.connected ? "Add an account" : "Connect"
                        strong: !root.connected
                        enabledNow: !(root.request !== null && root.request.state === "pending")
                        onPressed: root.connect()
                    }
                    Link { visible: root.connected; label: "Disconnect"; onPressed: root.disconnect() }
                }
            }

            // 2. Account selection.
            Panel {
                title: "2. Choose the account that receives them"
                visible: root.connected
                Hint {
                    visible: root.payable.length === 0
                    text: "Give this app a public account (Add an account): the faucet pays public accounts."
                }
                Hint {
                    visible: root.hasPrivate
                    text: "Private accounts can't be paid by a faucet. Have a public one paid, then move tokens to private in the Names app."
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
            }

            // 3. Ask.
            Panel {
                title: "3. Ask the faucet"
                visible: root.connected && root.payable.length > 0
                RowLayout {
                    spacing: 12
                    Action {
                        label: root.asking ? "Asking…" : root.coolingDown ? "Try again " + Logic.waitText(root.waitUntil - root.tick, root.waitUntil) : "Get test tokens for " + Logic.short(root.chosen)
                        strong: true
                        enabledNow: !root.asking && !root.coolingDown && root.chosen !== "" && root.urlProblem === ""
                        onPressed: root.ask()
                    }
                }
                Status {
                    visible: root.outcome !== null
                    text: root.outcome ? root.outcome.message : ""
                    tone: root.outcome && root.outcome.ok ? root.good : root.alarm
                }
                Hint {
                    visible: root.outcome !== null && !root.outcome.ok && !root.outcome.final && root.outcome.retryText !== ""
                    text: root.outcome ? "You can ask again " + root.outcome.retryText + "." : ""
                }
                Fact { visible: !!(root.outcome && root.outcome.tx); label: "Transaction"; value: root.outcome && root.outcome.tx ? root.outcome.tx : "" }
                Status {
                    visible: root.outcome !== null && root.outcome.ok
                    text: root.arrived
                        ? "Arrived: " + Logic.short(root.outcome.account) + " now holds " + Logic.grouped(Logic.balanceOf(root.balances, root.outcome.account)) + " LEZ."
                        : root.tick - (root.outcome ? root.outcome.at : 0) > 600000
                            ? "Not arrived after 10 minutes. The faucet forgets a grant that never arrives, so you can ask again."
                            : "Waiting for it to arrive (the balance is read every 15 s)…"
                    tone: root.arrived ? root.good : root.ink
                }
                Bad { text: root.problem }
            }
        }
    }

    // ── Controls ────────────────────────────────────────────────────────────

    component Panel: Rectangle {
        default property alias content: panelColumn.data
        property string title: ""
        Layout.fillWidth: true
        implicitHeight: panelColumn.implicitHeight + 36
        radius: 8
        color: root.card
        border.width: 1
        border.color: root.line
        ColumnLayout {
            id: panelColumn
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            Text {
                text: parent.parent.title
                color: root.ink
                font.family: root.sans
                font.pixelSize: root.px(19)
                font.weight: Font.DemiBold
            }
        }
    }

    component Hint: Text {
        Layout.fillWidth: true
        color: root.dim
        font.family: root.sans
        font.pixelSize: root.px(15)
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    component Bad: Text {
        Layout.fillWidth: true
        visible: text !== ""
        color: root.alarm
        font.family: root.sans
        font.pixelSize: root.px(15)
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    component Status: Text {
        property color tone: root.ink
        Layout.fillWidth: true
        color: tone
        font.family: root.sans
        font.pixelSize: root.px(17)
        font.weight: Font.Medium
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    component Fact: RowLayout {
        property string label: ""
        property string value: ""
        Layout.fillWidth: true
        spacing: 10
        Text { text: parent.label; color: root.dim; font.family: root.sans; font.pixelSize: root.px(15) }
        TextEdit {
            Layout.fillWidth: true
            text: parent.value
            readOnly: true
            selectByMouse: true
            color: root.ink
            font.family: root.mono
            font.pixelSize: root.px(14)
            wrapMode: TextEdit.WrapAnywhere
        }
    }

    component Action: Rectangle {
        id: action
        property string label: ""
        property bool strong: false
        property bool enabledNow: true
        signal pressed()
        implicitWidth: actionText.implicitWidth + 28
        implicitHeight: 38
        radius: 6
        color: !enabledNow ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(root.brass, 1.08) : root.brass) : (actionArea.containsMouse ? root.raised : "transparent")
        border.width: strong && enabledNow ? 0 : 1
        border.color: activeFocus ? root.brass : root.line
        opacity: enabledNow ? 1 : 0.55
        activeFocusOnTab: enabledNow
        Keys.onReturnPressed: if (enabledNow) action.pressed()
        Keys.onSpacePressed: if (enabledNow) action.pressed()
        Text {
            id: actionText
            anchors.centerIn: parent
            text: action.label
            color: action.strong && action.enabledNow ? root.slate : root.ink
            font.family: root.sans
            font.pixelSize: root.px(17)
            font.weight: Font.DemiBold
        }
        MouseArea {
            id: actionArea
            anchors.fill: parent
            hoverEnabled: true
            enabled: action.enabledNow
            cursorShape: Qt.PointingHandCursor
            onClicked: action.pressed()
        }
    }

    component Choice: Rectangle {
        id: choice
        property string label: ""
        property string detail: ""
        property bool chosen: false
        signal pressed()
        implicitHeight: 40
        radius: 6
        color: choiceArea.containsMouse ? root.raised : "transparent"
        border.width: 1
        border.color: chosen ? root.brass : root.line
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 10
            Rectangle {
                implicitWidth: 14
                implicitHeight: 14
                radius: 7
                color: choice.chosen ? root.brass : "transparent"
                border.width: 1
                border.color: choice.chosen ? root.brass : root.dim
            }
            Text {
                Layout.fillWidth: true
                text: choice.label
                color: root.ink
                font.family: root.mono
                font.pixelSize: root.px(14)
                elide: Text.ElideMiddle
            }
            Text { text: choice.detail; color: root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
        }
        MouseArea { id: choiceArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: choice.pressed() }
    }

    component Link: Text {
        id: link
        property string label: ""
        signal pressed()
        text: label
        color: linkArea.containsMouse ? root.brass : root.ink
        font.family: root.sans
        font.pixelSize: root.px(16)
        font.underline: true
        MouseArea { id: linkArea; anchors.fill: parent; anchors.margins: -6; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: link.pressed() }
    }

    component Field: Rectangle {
        id: field
        property alias text: input.text
        property bool code: false
        signal edited()
        implicitHeight: 40
        radius: 6
        color: root.slate
        border.width: 1
        border.color: input.activeFocus ? root.brass : root.line
        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            verticalAlignment: TextInput.AlignVCenter
            color: root.ink
            selectionColor: root.brass
            selectedTextColor: root.slate
            font.family: field.code ? root.mono : root.sans
            font.pixelSize: field.code ? root.px(15) : root.px(17)
            clip: true
            selectByMouse: true
            onEditingFinished: field.edited()
        }
    }
}
