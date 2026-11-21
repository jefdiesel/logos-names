// Testimonial: post a testimonial for a λPrize submission on the testimonial program,
// signed with the person's Names Wallet, and read a submission's testimonials.
//
// Built only on the Names Wallet Provider SDK (docs/wallet/PROVIDER-SDK.md) for
// everything that touches the wallet: connect (request_access), account selection
// (granted_accounts), approval and the transaction (propose_call, request_status). This
// app never sees a key. The person approves the call in the Names app.
//
// A testimonial can also be inscribed on the Logos Blockchain itself
// (propose_inscription): the Names app then asks which of the node's chain accounts
// inscribes it and pays the fee, so that path needs no connection first.
//
// Reads are plain sequencer reads (`getAccountView`), as the `testimonial` client makes
// them. Basecamp gives a QML view no network, so this app's small backend
// (src/testimonial_ui_backend.cpp) makes each read; it holds no key.
//
// The look is the shared kit (./kit): one left-aligned column, at most 680 px. The steps
// the person takes, their help and the plain result words are in logic.mjs, which the Node
// test runs. All wallet and backend calls, and every argument, are unchanged from the earlier app.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic
import "kit"

Item {
    id: root
    property Look look: Look {}

    // ── Settings ─────────────────────────────────────────────────────────────────────
    // The sequencer starts as the one the Names Wallet uses (its get_config), because
    // the wallet sends the post there. The testimonial program is deployed when testnet
    // 0.4 launches; until then its ID is entered here.
    property string sequencer: ""
    property string program: ""
    property string submission: Logic.DEFAULT_SUBMISSION
    property bool settingsOpen: false

    // Where the testimonial is written: "lez" (the testimonial program on the Logos test
    // network) or "chain" (an inscription on the Logos Blockchain).
    property string target: "lez"

    // ── Wallet state (Provider SDK) ──────────────────────────────────────────────────
    property var granted: []            // [{account, account_kind}] this app was given
    property bool connected: false
    property string author: ""          // the account picked in this app: Public/<id>
    property string requestId: ""       // the request being waited on
    property string requestKind: ""     // connect, call or inscribe
    property var request: null          // its last status
    property double postDeadline: 0     // when the post's time window closes (ms)
    property var lastPost: null         // {account, tx, node} of the post this app last sent
    property string askedBody: ""       // the text of the request being waited on, as sent
    property bool statusAsked: false    // a status read is out: the next waits for its answer
    property int statusMisses: 0        // status reads in a row that got no answer

    // ── Compose ──────────────────────────────────────────────────────────────────────
    property string username: ""
    property string body: ""            // the testimonial as typed
    property bool checking: false       // reading the chain before proposing
    property string problem: ""         // why the last step failed, in plain words

    // ── The list ─────────────────────────────────────────────────────────────────────
    property var listing: null          // {head_address, head, items}
    property bool listing_busy: false
    property string listError: ""
    property int shown: 20

    property real tick: Date.now()
    readonly property var backend: typeof logos !== "undefined" && logos.module ? logos.module("testimonial_ui") : null
    property bool backendReady: false
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModuleAsync
    readonly property var postable: Logic.postingAccounts(granted)
    readonly property bool waiting: requestId !== "" && (request === null || request.state === "pending" || request.state === "running")
    readonly property string usernameProblem: Logic.usernameProblem(username)
    readonly property string textProblem: body === "" ? "" : target === "chain" ? Logic.inscriptionProblem(body) : Logic.textProblem(body)
    readonly property string submissionProblem: Logic.submissionProblem(submission)
    readonly property bool programOk: Logic.isAccount(program)
    readonly property int textBytes: Logic.utf8(body).length
    readonly property var summary: listing ? Logic.summarize(listing.items) : null

    // ── View state ───────────────────────────────────────────────────────────────────
    property bool guideOpen: true
    property bool guideDone: false
    readonly property bool showGuide: guideOpen || !guideDone
    readonly property int step: Logic.flowStep(target, {
        connected: connected, postable: postable, body: body,
        requestId: requestId, waiting: waiting })
    property string resultLine: ""
    property string nextLine: ""
    property bool resultBad: false

    readonly property var said: Logic.requestStatus(requestKind, request)
    readonly property string networkName: target === "chain" ? "Logos Blockchain" : "Bridge test network"
    readonly property string networkDetail: target === "chain" ? "" : Logic.hostOf(sequencer)

    // ── The names module (Provider SDK) ──────────────────────────────────────────────

    /// Calls the names module. Every method answers {success, value, error} as JSON
    /// text; exactly one of done(value) and failed(message) runs.
    function names(method, args, done, failed) {
        if (!bridgeReady) {
            (failed || fail)("This app needs to run inside Basecamp, with the Names app installed.")
            return
        }
        logos.callModuleAsync("names", method, args, function(reply) {
            var r = null
            try { r = JSON.parse(reply) } catch (error) {}
            if (r && typeof r === "string") { try { r = JSON.parse(r) } catch (error) {} }
            // No reply at all is a call that waited past its time: the module answers one
            // call at a time. `true` tells the caller so.
            if (reply === undefined || reply === null || String(reply) === "")
                (failed || fail)("The names module is busy and didn't answer in time.", true)
            else if (!r || r.success === undefined)
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
        problem = sentence(message)
        checking = false
        say(problem, true)
    }

    function sentence(message) {
        var t = String(message).trim()
        if (t === "") return ""
        t = t.charAt(0).toUpperCase() + t.substr(1)
        return /[.?!]$/.test(t) ? t : t + "."
    }

    /// 1. Connect: the person picks, in the Names app, which accounts this app may see.
    function connect() {
        problem = ""
        say("")
        names("request_access", ["Post a testimonial signed by one of your public accounts"], function(v) {
            if (v.state === "approved") {
                readGranted()
                say("Connected.", false, "Choose the account that posts the testimonial.")
                return
            }
            follow(v, "connect")
        })
    }

    /// The accounts this app was given (asks nobody).
    function readGranted() {
        names("granted_accounts", [], function(v) {
            connected = v.connected
            granted = v.accounts || []
            var ok = Logic.postingAccounts(granted)
            if (ok.indexOf(author) < 0)
                author = ok.length > 0 ? ok[0] : ""
        }, function(message) { connected = false })
    }

    function disconnect() {
        names("drop_access", [], function() { connected = false; granted = []; author = "" })
        say("Names Wallet disconnected.")
    }

    /// Waits on a request (connect or call) until the person decides.
    function follow(v, kind) {
        requestId = v.request
        requestKind = kind
        request = v
        statusAsked = false
        statusMisses = 0
        poll.start()
    }

    Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: {
            if (root.statusAsked) return
            root.statusAsked = true
            var asked = root.requestId
            root.names("request_status", [asked], function(v) {
                // An answer about a request this app has since replaced says nothing now.
                if (asked !== root.requestId) return
                root.statusAsked = false
                root.statusMisses = 0
                root.request = v
                if (v.state === "pending" || v.state === "running") return
                poll.stop()
                if (root.requestKind === "connect" && v.state === "approved")
                    root.readGranted()
                if (root.requestKind === "call" && v.state === "done") {
                    root.lastPost = { account: v.account, tx: v.tx, node: root.lastPost ? root.lastPost.node : "" }
                    root.clearSent()
                    root.refresh()
                }
                if (root.requestKind === "inscribe" && v.state === "done")
                    root.clearSent()
                root.say(Logic.requestStatus(root.requestKind, v).text, v.state === "failed")
            }, function(message, unanswered) {
                if (asked !== root.requestId) return
                root.statusAsked = false
                // While the wallet sends an approved request (minutes, for an inscription)
                // the module answers nothing else: that is not a failure, so keep asking.
                if (unanswered && ++root.statusMisses <= 20) return
                poll.stop()
                root.fail(message)
            })
        }
    }

    /// Empties the text box once its text was sent, unless something else was typed since.
    function clearSent() {
        if (textArea.text === askedBody) textArea.text = ""
    }

    function cancel() {
        names("cancel_request", [requestId], function(v) { request = v; poll.stop(); say("Request withdrawn.") })
    }

    // ── Reads (this app's backend; plain getAccountView) ─────────────────────────────

    /// `forProgram`'s state on `account`, as bytes: callback(error, bytes).
    function readState(account, forProgram, callback) {
        if (!backend || !backendReady) {
            callback("this app's reader isn't running yet")
            return
        }
        logos.watch(backend.accountView(sequencer, account, forProgram), function(answer) {
            var r = null
            try { r = JSON.parse(answer) } catch (error) {}
            if (!r) return callback("the reader sent an unreadable answer")
            if (!r.ok) return callback(r.error)
            var bytes
            try { bytes = Logic.actorState(r.result, forProgram) } catch (error) { return callback(error.message) }
            callback(null, bytes)
        }, function(error) { callback(String(error)) })
    }

    /// Reads the submission's whole list (one read per testimonial), newest first.
    function refresh() {
        if (!programOk || submissionProblem !== "" || sequencer === "") return
        listing_busy = true
        listError = ""
        var program_ = program, submission_ = submission
        Logic.walk(program_, submission_, function(account, callback) { readState(account, program_, callback) },
            function(partial) { if (program_ === program && submission_ === submission) listing = partial },
            function(error, done) {
                if (program_ !== program || submission_ !== submission) return
                listing_busy = false
                if (error) listError = sentence(error)
                else listing = done
            })
    }

    /// 2-4. Checks the post against the program's rules and the chain, then proposes it.
    /// The person approves it in the Names app, where they must sign with `author`: the
    /// testimonial's address is derived from the author, so another account's signature
    /// is refused when the wallet tries the call, before anything is sent.
    function post() {
        problem = ""
        say("")
        var p = !programOk ? "Enter the testimonial program's ID in Settings."
              : submissionProblem || usernameProblem || Logic.textProblem(body)
              || (author === "" ? "Pick the account that posts it." : "")
        if (p) { problem = p; say(problem, true); return }
        checking = true
        var call = Logic.postCall(program, submission, username, body, 1, author)
        // One testimonial per account and submission: say so before asking anyone.
        readState(call.node, program, function(error, existing) {
            if (error) return fail("Couldn't read the chain: " + error)
            if (existing.length > 0) return fail(author + " already posted a testimonial for " + submission + "; there is one per account.")
            // The post carries the chain's time: the last block's, from the clock account.
            readState(Logic.CLOCK_ACCOUNT, Logic.CLOCK_PROGRAM, function(clockError, clock) {
                var chainTime = 0
                try { if (!clockError && clock.length === 16) chainTime = Logic.clockTime(clock) } catch (e) {}
                var now = Logic.postTime(chainTime, Date.now())
                var made = Logic.postCall(program, submission, username, body, now, author)
                askedBody = body
                names("propose_call", [made.program, made.accounts_json, made.instruction_hex,
                                       Logic.postSummary(submission, author, body)], function(v) {
                    checking = false
                    postDeadline = now + Logic.TIME_WINDOW_MS
                    lastPost = { account: author, tx: "", node: made.node }
                    follow(v, "call")
                })
            })
        })
    }

    /// Asks the Names Wallet to inscribe the testimonial on the Logos Blockchain. The
    /// person picks the chain account that pays and approves it in the Names app; this
    /// app names no account and needs no access for it.
    function inscribe() {
        problem = ""
        say("")
        var p = Logic.inscriptionProblem(body)
        if (p) { problem = p; say(problem, true); return }
        checking = true
        askedBody = body
        names("propose_inscription", [body], function(v) {
            checking = false
            postDeadline = 0
            follow(v, "inscribe")
        })
    }

    /// Copy a full id or hash, with a plain result line. The screen shows only the short form.
    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.")
    }

    function finishGuide() {
        guideOpen = false
        guideDone = true
    }

    TextEdit { id: clipboard; visible: false }

    Component.onCompleted: {
        backendReady = backend !== null && logos.isViewModuleReady && logos.isViewModuleReady("testimonial_ui")
        names("get_config", [], function(config) {
            if (sequencer === "" && config && config.sequencer) sequencer = config.sequencer
            refresh()
        }, function() {})
        readGranted()
    }

    Connections {
        target: typeof logos !== "undefined" ? logos : null
        ignoreUnknownSignals: true
        function onViewModuleReadyChanged(moduleName, isReady) {
            if (moduleName === "testimonial_ui") {
                root.backendReady = isReady && root.backend !== null
                if (root.backendReady && root.listing === null) root.refresh()
            }
        }
    }

    Timer { interval: 1000; repeat: true; running: root.waiting; onTriggered: root.tick = Date.now() }

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
                    title: "Testimonial"
                    sentence: "Vouch for a Logos prize submission. Your Names Wallet signs it; this app never sees a key."
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

            // ── Where it is written ───────────────────────────────────────────────────
            Card {
                visible: !root.settingsOpen
                Heading { text: "Where to write it" }
                Repeater {
                    model: Logic.TARGETS
                    delegate: Choice {
                        required property var modelData
                        Layout.fillWidth: true
                        label: modelData.label
                        sub: modelData.sub
                        plain: true
                        chosen: root.target === modelData.key
                        onPressed: if (!root.waiting && !root.checking) {
                            root.target = modelData.key
                            root.problem = ""
                            root.resultLine = ""
                        }
                    }
                }
                Body {
                    visible: root.target === "chain"
                    text: "No connection is needed for this: when you ask, the Names app shows the words, lets you choose which of your node's Logos Blockchain accounts pays the fee, and inscribes it only once you approve. An inscription is public and can't be changed or removed."
                }
            }

            // ── The steps, with the current one marked and its help under it ──────────
            Card {
                visible: root.showGuide && !root.settingsOpen
                Heading { text: "How it works" }
                Repeater {
                    model: Logic.flowSteps(root.target, root.step)
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
                                text: stepRow.modelData.n === 1 && root.target === "lez" && root.waiting && root.requestKind === "connect"
                                    ? "Names Wallet is open with the request: tick the accounts you use for Testimonial and approve. You come back here after."
                                    : stepRow.modelData.detail
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 1 && root.target === "lez"
                                look: root.look; strong: true; changes: true; busy: root.checking
                                label: "Connect"; busyLabel: "Waiting for you in Names…"
                                working: root.waiting && root.requestKind === "connect"
                                enabledNow: !root.waiting
                                onPressed: root.connect()
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 2 && root.target === "lez" && root.postable.length === 0
                                look: root.look; strong: true; changes: true; busy: root.checking
                                label: "Add an account"; busyLabel: "Waiting for you in Names…"
                                working: root.waiting && root.requestKind === "connect"
                                enabledNow: !root.waiting
                                onPressed: root.connect()
                            }
                            Link {
                                visible: stepRow.now && stepRow.modelData.n === 2 && root.target === "chain"
                                look: root.look; font.underline: true
                                text: "Write it below"
                                onPressed: textArea.forceActiveFocus()
                            }
                            Link {
                                visible: stepRow.now && stepRow.modelData.n === 4 && root.waiting
                                look: root.look; font.underline: true
                                text: "Withdraw the request"
                                onPressed: root.cancel()
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.topMargin: 4
                    spacing: 20
                    Action { look: root.look; label: "Got it"; visible: root.connected || root.target === "chain"; onPressed: root.finishGuide() }
                }
            }

            // ── Accounts that may post ────────────────────────────────────────────────
            Card {
                visible: root.target === "lez" && root.connected && !root.settingsOpen
                Heading { text: "Accounts that may post" }
                Body {
                    visible: root.postable.length === 0
                    text: "Give this app a public account (Add an account). A testimonial comes from a public account: one per account."
                }
                Repeater {
                    model: root.postable
                    delegate: Choice {
                        required property string modelData
                        Layout.fillWidth: true
                        label: modelData
                        chosen: root.author === modelData
                        onPressed: root.author = modelData
                    }
                }
                Link { look: root.look; font.underline: true; visible: root.connected; text: "Add an account"; onPressed: root.connect() }
                Link { look: root.look; font.underline: true; visible: root.connected; text: "Disconnect Names Wallet"; onPressed: root.disconnect() }
            }

            // ── Compose and send ──────────────────────────────────────────────────────
            Card {
                visible: !root.settingsOpen && (root.target === "chain" || (root.connected && root.postable.length > 0))
                Heading { text: "Your testimonial" }
                Field {
                    visible: root.target === "lez"
                    look: root.look; Layout.fillWidth: true
                    text: root.username
                    hint: "Name to show (optional)"
                    onTextChanged: root.username = text
                }
                Body {
                    visible: root.target === "lez" && root.usernameProblem !== ""
                    text: root.usernameProblem
                    color: look.alarm
                }
                Body {
                    text: (root.target === "chain" ? "Testimonial for the Names Wallet" : "Testimonial for " + root.submission)
                        + " · " + root.textBytes + " / " + Logic.MAX_TEXT + " bytes"
                }
                Area { id: textArea; look: root.look; Layout.fillWidth: true; onTextChanged: root.body = text }
                Body {
                    visible: root.textProblem !== ""
                    text: root.textProblem
                    color: look.alarm
                }
                Action {
                    visible: root.target === "lez"
                    look: root.look; strong: true; changes: true; busy: root.checking
                    label: "Post testimonial"; busyLabel: "Checking…"; working: root.checking
                    enabledNow: !root.checking && !root.waiting && root.author !== "" && root.textProblem === ""
                    onPressed: root.post()
                }
                Action {
                    visible: root.target === "chain"
                    look: root.look; strong: true; changes: true; busy: root.checking
                    label: "Ask to inscribe"; busyLabel: "Asking…"; working: root.checking
                    enabledNow: !root.checking && !root.waiting && root.textProblem !== ""
                    onPressed: root.inscribe()
                }
            }

            // ── Approval and the result ───────────────────────────────────────────────
            Card {
                visible: root.requestId !== "" && !root.settingsOpen
                Heading { text: root.requestKind === "connect" ? "Waiting for the Names app" : root.requestKind === "inscribe" ? "Your inscription" : "Approve in the Names app" }
                Body {
                    visible: root.requestKind === "call" && root.request !== null && root.request.state === "pending"
                    text: "Open the Names app and approve the request from Testimonial. Choose " + (root.lastPost ? root.lastPost.account : root.author)
                        + " to sign: the testimonial's address is that account's, so another account is refused before anything is sent."
                        + (root.postDeadline > 0 ? " The post's time window closes in " + Math.max(0, Math.round((root.postDeadline - root.tick) / 60000)) + " min." : "")
                }
                Body {
                    visible: root.requestKind === "inscribe" && root.request !== null && root.request.state === "pending"
                    text: "Open the Names app. It shows these words, your node's Logos Blockchain accounts and the fee your node asks; choose the account that pays and approve. Nothing is inscribed before that."
                }
                Text {
                    textFormat: Text.PlainText
                    Layout.fillWidth: true
                    visible: root.said.text !== ""
                    text: root.said.text
                    color: root.said.tone === "bad" ? look.alarm : look.ink
                    font.family: look.sans
                    font.pixelSize: look.px(16)
                    font.weight: Font.Medium
                    wrapMode: Text.Wrap
                }
                Body {
                    visible: root.requestKind !== "inscribe" && root.request !== null && root.request.state === "failed"
                    text: "If you chose a different account in Names than " + (root.lastPost ? root.lastPost.account : "the one picked here")
                        + ", or the approval came after the time window closed, the wallet refused it before sending: post again. On the Logos test network a sent call that fails still pays its fee, which is why nothing is retried for you."
                }
                RowLayout {
                    visible: !!(root.request && root.request.tx)
                    Layout.fillWidth: true
                    spacing: 16
                    Text { textFormat: Text.PlainText; text: "Transaction"; color: look.dim; font.family: look.sans; font.pixelSize: look.px(14); Layout.preferredWidth: look.px(132) }
                    Text { textFormat: Text.PlainText; text: root.request && root.request.tx ? Logic.short(root.request.tx) : ""; color: look.ink; font.family: look.mono; font.pixelSize: look.px(13) }
                    Link { look: root.look; font.underline: true; text: "Copy"; onPressed: root.copy((root.request && root.request.tx) || "", "Transaction") }
                }
                FactRow {
                    visible: root.requestKind === "call" && !!(root.request && root.request.state === "done" && root.lastPost && root.lastPost.node)
                    look: root.look; key: "Stored at"; value: root.lastPost ? root.lastPost.node : ""; monospace: true
                }
                FactRow {
                    visible: root.requestKind === "inscribe" && !!(root.request && root.request.account)
                    look: root.look; key: root.request && root.request.state === "done" ? "Paid from" : "Account chosen"
                    value: root.request && root.request.account ? root.request.account : ""; monospace: true
                }
                FactRow {
                    visible: root.requestKind === "inscribe" && !!(root.request && root.request.fee)
                    look: root.look; key: "Fee paid"
                    value: root.request && root.request.fee ? Logic.grouped(root.request.fee) : ""
                }
                FactRow {
                    visible: root.requestKind === "inscribe" && !!(root.request && root.request.channel)
                    look: root.look; key: "Channel"; value: root.request && root.request.channel ? root.request.channel : ""; monospace: true
                }
                FactRow {
                    visible: root.requestKind === "inscribe" && !!(root.request && root.request.submission)
                    look: root.look; key: "Filed under"; value: root.request && root.request.submission ? root.request.submission : ""
                }
                Link {
                    visible: root.waiting && root.request && root.request.state === "pending"
                    look: root.look; font.underline: true
                    text: "Withdraw the request"
                    onPressed: root.cancel()
                }
            }

            // ── Reading inscriptions ──────────────────────────────────────────────────
            Card {
                visible: root.target === "chain" && !root.settingsOpen
                Heading { text: "Reading inscriptions" }
                Body {
                    text: "An inscription is read from the Logos Blockchain itself: every account has its own channel, shown here once yours is inscribed, and its transaction shows in the Names app's history. The list of testimonials on the Logos test network is under “Where to write it”."
                }
            }

            // ── The submission's testimonials ─────────────────────────────────────────
            Card {
                visible: root.target === "lez" && !root.settingsOpen
                Heading { text: "Testimonials for " + root.submission }
                Body {
                    visible: !root.programOk
                    text: "Enter the testimonial program's ID in Settings to read the list. It is deployed when testnet 0.4 launches."
                }
                Body {
                    visible: root.programOk
                    text: "Read straight from " + root.sequencer + " with getAccountView, one read per testimonial, and checked: one per account, numbered without gaps."
                }
                Body {
                    visible: root.listError !== ""
                    text: root.listError
                    color: look.alarm
                }
                RowLayout {
                    visible: root.programOk
                    Layout.fillWidth: true
                    spacing: 12
                    Action {
                        look: root.look
                        label: root.listing_busy ? "Reading…" : "Refresh list"
                        busyLabel: "Reading…"; working: root.listing_busy
                        enabledNow: !root.listing_busy
                        onPressed: root.refresh()
                    }
                    Text {
                        visible: root.listing !== null && root.listing.head !== null
                        text: root.listing && root.listing.head ? root.listing.items.length + " of " + root.listing.head.count + " read" : ""
                        color: look.dim
                        font.family: look.sans
                        font.pixelSize: look.px(15)
                    }
                }
                Body {
                    visible: root.listing !== null && root.listing.head === null && !root.listing_busy
                    text: "No testimonials yet. Be the first."
                }

                // Per-month counts (UTC, by each testimonial's on-chain time).
                ColumnLayout {
                    visible: root.summary !== null && root.summary.total > 0
                    Layout.fillWidth: true
                    spacing: 6
                    Text {
                        textFormat: Text.PlainText
                        text: root.summary ? root.summary.total + " testimonials from " + root.summary.distinct_authors + " accounts" : ""
                        color: look.ink
                        font.family: look.sans
                        font.pixelSize: look.px(17)
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        model: root.summary ? root.summary.months : []
                        delegate: RowLayout {
                            required property var modelData
                            spacing: 10
                            Text { text: modelData.month; color: look.dim; font.family: look.mono; font.pixelSize: look.px(15) }
                            Rectangle {
                                implicitHeight: 10
                                implicitWidth: Math.max(4, Math.min(360, modelData.count * 4))
                                radius: 3
                                color: look.brass
                            }
                            Text { text: modelData.count + (modelData.qualifies ? "  ·  30 or more" : ""); color: look.ink; font.family: look.sans; font.pixelSize: look.px(15) }
                        }
                    }
                    Body {
                        text: root.summary && root.summary.meets_adoption
                            ? "Meets the adoption bar: 150 or more testimonials from 150 or more accounts, in 2 or more months with 30 or more each."
                            : "Adoption bar: 150 testimonials from 150 accounts, in 2 or more months with 30 or more each. Not met yet."
                    }
                }

                Repeater {
                    model: root.listing ? root.listing.items.slice(0, root.shown) : []
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: entry.implicitHeight + 24
                        radius: 6
                        color: look.raised
                        ColumnLayout {
                            id: entry
                            anchors.fill: parent
                            anchors.margins: 12
                            spacing: 4
                            Text {
                                Layout.fillWidth: true
                                textFormat: Text.PlainText
                                text: "#" + modelData.seq + "  " + (modelData.username !== null ? modelData.username + "  ·  " : "")
                                    + Logic.short(modelData.author) + "  ·  " + Logic.formatTime(modelData.at)
                                color: look.dim
                                font.family: look.sans
                                font.pixelSize: look.px(14)
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: modelData.text
                                textFormat: Text.PlainText
                                color: look.ink
                                font.family: look.sans
                                font.pixelSize: look.px(16)
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }
                Link {
                    visible: root.listing !== null && root.listing.items.length > root.shown
                    look: root.look; font.underline: true
                    text: "Show more"
                    onPressed: root.shown += 20
                }
            }

            // ── Settings ──────────────────────────────────────────────────────────────
            Card {
                visible: root.settingsOpen
                Heading { text: "Settings" }
                Body { text: "Reads go to this sequencer. The Names Wallet sends the post to its own sequencer, so keep them the same." }
                Field { id: sequencerField; look: root.look; Layout.fillWidth: true; code: true; text: root.sequencer; hint: "https://…" }
                Body { text: "Testimonial program ID" }
                Field { id: programField; look: root.look; Layout.fillWidth: true; code: true; text: root.program; hint: "base58 program ID" }
                Body {
                    visible: programField.text !== "" && !Logic.isAccount(programField.text.trim())
                    text: "That is not an account ID."
                    color: look.alarm
                }
                Body { text: "Submission ID (default " + Logic.DEFAULT_SUBMISSION + ")" }
                Field { id: submissionField; look: root.look; Layout.fillWidth: true; code: true; text: root.submission }
                Body {
                    visible: Logic.submissionProblem(submissionField.text.trim()) !== ""
                    text: Logic.submissionProblem(submissionField.text.trim())
                    color: look.alarm
                }
                Action {
                    look: root.look; strong: true
                    label: "Use these settings"
                    onPressed: {
                        root.sequencer = sequencerField.text.trim()
                        root.program = programField.text.trim()
                        root.submission = submissionField.text.trim()
                        root.listing = null
                        root.refresh()
                        root.settingsOpen = false
                        root.say("Settings saved.")
                    }
                }
            }

            TechDetails {
                visible: !root.settingsOpen
                look: root.look
                title: "Technical details"
                summary: root.program ? Logic.short(root.program) : ""
                FactRow { look: root.look; key: "Program"; value: root.program; monospace: true }
                FactRow { look: root.look; key: "Sequencer"; value: root.sequencer; monospace: true }
                FactRow { look: root.look; key: "Clock program"; value: Logic.CLOCK_PROGRAM; monospace: true }
                FactRow { look: root.look; key: "Clock account"; value: Logic.CLOCK_ACCOUNT; monospace: true }
            }

            Text {
                textFormat: Text.PlainText
                visible: root.problem !== "" && root.resultLine === ""
                Layout.fillWidth: true
                text: root.problem
                color: look.alarm
                font.family: look.sans
                font.pixelSize: look.px(16)
                wrapMode: Text.Wrap
            }
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
        property string sub: ""
        property bool plain: false
        property bool chosen: false
        signal pressed()
        implicitHeight: sub !== "" ? 58 : 42
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
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Text {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: choice.label
                    color: root.look.ink
                    font.family: choice.plain ? root.look.sans : root.look.mono
                    font.pixelSize: choice.plain ? root.look.px(16) : root.look.px(14)
                    font.weight: choice.plain ? Font.DemiBold : Font.Normal
                    elide: choice.plain ? Text.ElideRight : Text.ElideMiddle
                }
                Text {
                    visible: choice.sub !== ""
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    text: choice.sub
                    color: root.look.dim
                    font.family: root.look.sans
                    font.pixelSize: root.look.px(14)
                    wrapMode: Text.Wrap
                    elide: Text.ElideRight
                }
            }
        }
        MouseArea { id: choiceArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: choice.pressed() }
    }

    component Area: Rectangle {
        id: area
        property Look look: Look {}
        property alias text: areaInput.text
        implicitHeight: 130
        radius: 6
        color: look.slate
        border.width: 1
        border.color: areaInput.activeFocus ? look.brass : look.line
        Flickable {
            id: areaFlick
            anchors.fill: parent
            anchors.margins: 12
            contentHeight: areaInput.height
            clip: true
            TextEdit {
                id: areaInput
                width: areaFlick.width
                height: Math.max(areaFlick.height, implicitHeight)
                color: look.ink
                selectionColor: look.brass
                selectedTextColor: look.slate
                font.family: look.sans
                font.pixelSize: look.px(17)
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                selectByMouse: true
            }
        }
        Text {
            anchors.fill: parent
            anchors.margins: 12
            visible: areaInput.text === ""
            text: "What did it do for you?"
            color: look.faint
            font.family: look.sans
            font.pixelSize: look.px(17)
        }
    }
}
