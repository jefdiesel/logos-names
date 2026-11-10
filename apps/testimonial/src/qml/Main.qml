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
// Controls are drawn here rather than taken from Qt Quick Controls, as in the Names app,
// so the app looks the same whatever style its host applies.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic

Item {
    id: root

    // ── Settings ────────────────────────────────────────────────────────────
    // The sequencer starts as the one the Names Wallet uses (its get_config), because
    // the wallet sends the post there. The testimonial program is deployed when testnet
    // 0.4 launches; until then its ID is entered here.
    property string sequencer: ""
    property string program: ""
    property string submission: Logic.DEFAULT_SUBMISSION
    property bool settingsOpen: false

    // Where the testimonial is written: "lez" (the testimonial program on the LEZ zone)
    // or "chain" (an inscription on the Logos Blockchain).
    property string target: "lez"

    // ── Wallet state (Provider SDK) ─────────────────────────────────────────
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

    // ── Compose ─────────────────────────────────────────────────────────────
    property string username: ""
    property string body: ""            // the testimonial as typed
    property bool checking: false       // reading the chain before proposing
    property string problem: ""         // why the last step failed, in plain words

    // ── The list ────────────────────────────────────────────────────────────
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

    function fail(message) {
        problem = sentence(message)
        checking = false
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
        names("request_access", ["Post a testimonial signed by one of your public accounts"], function(v) {
            if (v.state === "approved") {
                readGranted()
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
        names("cancel_request", [requestId], function(v) { request = v; poll.stop() })
    }

    // ── Reads (this app's backend; plain getAccountView) ───────────────────

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
        var p = !programOk ? "Enter the testimonial program's ID in Settings."
              : submissionProblem || usernameProblem || Logic.textProblem(body)
              || (author === "" ? "Pick the account that posts it." : "")
        if (p) { problem = p; return }
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
        var p = Logic.inscriptionProblem(body)
        if (p) { problem = p; return }
        checking = true
        askedBody = body
        names("propose_inscription", [body], function(v) {
            checking = false
            postDeadline = 0
            follow(v, "inscribe")
        })
    }

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

    // ── View ────────────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    Flickable {
        id: page
        anchors.fill: parent
        contentHeight: column.implicitHeight + 64
        clip: true

        ColumnLayout {
            id: column
            width: Math.min(page.width - 48, 760)
            x: (page.width - width) / 2
            y: 32
            spacing: 18

            RowLayout {
                Layout.fillWidth: true
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Text {
                        text: "Testimonial"
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(28)
                        font.weight: Font.Bold
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Vouch for a λPrize submission on chain. Your Names Wallet signs it; this app never sees a key."
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(16)
                        wrapMode: Text.Wrap
                    }
                }
                Link { label: root.settingsOpen ? "Close settings" : "Settings"; onPressed: root.settingsOpen = !root.settingsOpen }
            }

            // Settings: the chain, the program and the submission.
            Panel {
                visible: root.settingsOpen
                title: "Settings"
                Hint { text: "Sequencer (reads go here; the Names Wallet sends the post to its own sequencer, so keep them the same)" }
                Field { Layout.fillWidth: true; text: root.sequencer; code: true; hint: "https://…"; onEdited: root.sequencer = text.trim() }
                Hint { text: "Testimonial program ID" }
                Field { Layout.fillWidth: true; text: root.program; code: true; hint: "base58 program ID"; onEdited: root.program = text.trim() }
                Bad { text: root.program !== "" && !root.programOk ? "That is not an account ID." : "" }
                Hint { text: "Submission ID (default " + Logic.DEFAULT_SUBMISSION + ")" }
                Field { Layout.fillWidth: true; text: root.submission; code: true; onEdited: root.submission = text.trim() }
                Bad { text: root.submissionProblem }
                Action { label: "Use these"; strong: true; onPressed: { root.listing = null; root.refresh(); root.settingsOpen = false } }
            }

            // Where it's written.
            Panel {
                title: "Where to write it"
                Repeater {
                    model: Logic.TARGETS
                    delegate: Choice {
                        required property var modelData
                        Layout.fillWidth: true
                        label: modelData.label
                        sub: modelData.sub
                        plain: true
                        chosen: root.target === modelData.key
                        onPressed: if (!root.waiting && !root.checking) { root.target = modelData.key; root.problem = "" }
                    }
                }
                Hint {
                    visible: root.target === "chain"
                    text: "No connection is needed for this: when you ask, the Names app shows the words, lets you choose which of your node's Logos Blockchain accounts pays the fee, and inscribes it only once you approve. An inscription is public and can't be changed or removed."
                }
            }

            // 1. Connect.
            Panel {
                title: "1. Connect your Names Wallet"
                visible: root.target === "lez"
                Hint {
                    text: root.connected
                        ? "Connected. This app may see: " + root.granted.map(function(a) { return Logic.short(a.account) }).join(", ") + "."
                        : "The Names app asks you which accounts this app may see. Nothing is signed yet."
                }
                RowLayout {
                    spacing: 12
                    Action {
                        label: root.connected ? "Add an account" : "Connect"
                        strong: !root.connected
                        enabledNow: !root.waiting
                        onPressed: root.connect()
                    }
                    Link { visible: root.connected; label: "Disconnect"; onPressed: root.disconnect() }
                }
            }

            // 2. Account selection.
            Panel {
                title: "2. Choose the account that posts"
                visible: root.target === "lez" && root.connected
                Hint {
                    text: root.postable.length === 0
                        ? "Give this app a public account (Add an account). A testimonial comes from a public account: one per account, and the program refuses a private sender's one-time alias."
                        : "One testimonial per account. The account pays the fee (about one block's gas); the program can't move its LEZ."
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
            }

            // 3. Write it.
            Panel {
                title: root.target === "chain" ? "Write your testimonial" : "3. Write your testimonial"
                visible: root.target === "chain" || (root.connected && root.postable.length > 0)
                Hint { visible: root.target === "lez"; text: "Name to show (optional, up to 32 bytes; anyone can type any name, so it isn't proof of who you are)" }
                Field { visible: root.target === "lez"; Layout.fillWidth: true; text: root.username; onEdited: root.username = text; onTextChanged: root.username = text }
                Bad { text: root.target === "lez" ? root.usernameProblem : "" }
                Hint {
                    text: (root.target === "chain" ? "Testimonial for the Names Wallet" : "Testimonial for " + root.submission)
                        + " (" + root.textBytes + " / " + Logic.MAX_TEXT + " bytes)"
                }
                Area { id: textArea; Layout.fillWidth: true; onTextChanged: root.body = text }
                Bad { text: root.textProblem }
                RowLayout {
                    spacing: 12
                    Action {
                        visible: root.target === "lez"
                        label: root.checking ? "Checking…" : "Post with " + Logic.short(root.author)
                        strong: true
                        enabledNow: !root.checking && !root.waiting && root.author !== ""
                        onPressed: root.post()
                    }
                    Action {
                        visible: root.target === "chain"
                        label: root.checking ? "Asking…" : "Ask the Names app to inscribe it"
                        strong: true
                        enabledNow: !root.checking && !root.waiting && root.body !== "" && root.textProblem === ""
                        onPressed: root.inscribe()
                    }
                }
                Bad { text: root.problem }
            }

            // 4. Approval and the transaction.
            Panel {
                title: root.requestKind === "connect" ? "Waiting for the Names app" : root.requestKind === "inscribe" ? (root.waiting ? "Approve in the Names app" : "Your inscription") : "4. Approve in the Names app"
                visible: root.requestId !== ""
                Hint {
                    visible: root.requestKind === "call" && root.request !== null && root.request.state === "pending"
                    text: "Open the Names app and approve the request from Testimonial. Choose " + (root.lastPost ? root.lastPost.account : root.author)
                        + " to sign: the testimonial's address is that account's, so another account is refused before anything is sent."
                        + (root.postDeadline > 0 ? " The post's time window closes in " + Math.max(0, Math.round((root.postDeadline - root.tick) / 60000)) + " min." : "")
                }
                Hint {
                    visible: root.requestKind === "inscribe" && root.request !== null && root.request.state === "pending"
                    text: "Open the Names app. It shows these words, your node's Logos Blockchain accounts and the fee your node asks; choose the account that pays and approve. Nothing is inscribed before that."
                }
                Status {
                    readonly property var said: Logic.requestStatus(root.requestKind, root.request)
                    text: said.text
                    tone: said.tone === "good" ? root.good : said.tone === "bad" ? root.alarm : root.ink
                }
                Hint {
                    visible: root.requestKind !== "inscribe" && root.request !== null && root.request.state === "failed"
                    text: "If you chose a different account in Names than " + (root.lastPost ? root.lastPost.account : "the one picked here")
                        + ", or the approval came after the time window closed, the wallet refused it before sending: post again. On LEZ a sent call that fails still pays its fee, which is why nothing is retried for you."
                }
                Fact { visible: !!(root.request && root.request.tx); label: "Transaction"; value: root.request && root.request.tx ? root.request.tx : "" }
                Fact { visible: root.requestKind === "call" && !!(root.request && root.request.state === "done" && root.lastPost && root.lastPost.node); label: "Stored at"; value: root.lastPost ? root.lastPost.node : "" }
                Fact { visible: root.requestKind === "inscribe" && !!(root.request && root.request.account); label: root.request && root.request.state === "done" ? "Paid from" : "Account chosen"; value: root.request && root.request.account ? root.request.account : "" }
                Fact { visible: root.requestKind === "inscribe" && !!(root.request && root.request.fee); label: "Fee paid"; value: root.request && root.request.fee ? Logic.grouped(root.request.fee) : "" }
                Fact { visible: root.requestKind === "inscribe" && !!(root.request && root.request.channel); label: "Channel"; value: root.request && root.request.channel ? root.request.channel : "" }
                Fact { visible: root.requestKind === "inscribe" && !!(root.request && root.request.submission); label: "Filed under"; value: root.request && root.request.submission ? root.request.submission : "" }
                Link { visible: root.waiting && root.request && root.request.state === "pending"; label: "Withdraw the request"; onPressed: root.cancel() }
            }

            // The submission's testimonials, read from the chain.
            Panel {
                visible: root.target === "chain"
                title: "Reading inscriptions"
                Hint {
                    text: "An inscription is read from the Logos Blockchain itself: every account has its own channel, shown here once yours is inscribed, and its transaction shows in the Names app's history. The list of testimonials on the LEZ zone is under “LEZ zone” above."
                }
            }
            Panel {
                visible: root.target === "lez"
                title: "Testimonials for " + root.submission
                Hint {
                    visible: !root.programOk
                    text: "Enter the testimonial program's ID in Settings to read the list. It is deployed when testnet 0.4 launches."
                }
                Hint {
                    visible: root.programOk
                    text: "Read straight from " + root.sequencer + " with getAccountView, one read per testimonial, and checked: one per account, numbered without gaps."
                }
                RowLayout {
                    visible: root.programOk
                    spacing: 12
                    Action { label: root.listing_busy ? "Reading…" : "Refresh"; enabledNow: !root.listing_busy; onPressed: root.refresh() }
                    Text {
                        visible: root.listing !== null && root.listing.head !== null
                        text: root.listing && root.listing.head ? root.listing.items.length + " of " + root.listing.head.count + " read" : ""
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(15)
                    }
                }
                Bad { text: root.listError }
                Hint { visible: root.listing !== null && root.listing.head === null && !root.listing_busy; text: "No testimonials yet. Be the first." }

                // Per-month counts (UTC, by each testimonial's on-chain time).
                ColumnLayout {
                    visible: root.summary !== null && root.summary.total > 0
                    Layout.fillWidth: true
                    spacing: 6
                    Text {
                        text: root.summary ? root.summary.total + " testimonials from " + root.summary.distinct_authors + " accounts" : ""
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(17)
                        font.weight: Font.DemiBold
                    }
                    Repeater {
                        model: root.summary ? root.summary.months : []
                        delegate: RowLayout {
                            required property var modelData
                            spacing: 10
                            Text { text: modelData.month; color: root.dim; font.family: root.mono; font.pixelSize: root.px(15) }
                            Rectangle {
                                implicitHeight: 10
                                implicitWidth: Math.max(4, Math.min(360, modelData.count * 4))
                                radius: 3
                                color: modelData.qualifies ? root.good : root.brass
                            }
                            Text { text: modelData.count + (modelData.qualifies ? "  ✓ 30+" : ""); color: root.ink; font.family: root.sans; font.pixelSize: root.px(15) }
                        }
                    }
                    Hint {
                        text: root.summary && root.summary.meets_adoption
                            ? "Meets the LP-0021 adoption bar: 150+ testimonials from 150+ accounts, in 2+ months with 30+ each."
                            : "LP-0021 adoption bar: 150 testimonials from 150 accounts, in 2+ months with 30+ each. Not met yet."
                    }
                }

                Repeater {
                    model: root.listing ? root.listing.items.slice(0, root.shown) : []
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: entry.implicitHeight + 24
                        radius: 6
                        color: root.raised
                        ColumnLayout {
                            id: entry
                            anchors.fill: parent
                            anchors.margins: 12
                            spacing: 4
                            Text {
                                Layout.fillWidth: true
                                text: "#" + modelData.seq + "  " + (modelData.username !== null ? modelData.username + "  ·  " : "")
                                    + Logic.short(modelData.author) + "  ·  " + Logic.formatTime(modelData.at)
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: root.px(14)
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: modelData.text
                                textFormat: Text.PlainText
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: root.px(16)
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }
                Link {
                    visible: root.listing !== null && root.listing.items.length > root.shown
                    label: "Show more"
                    onPressed: root.shown += 20
                }
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
        property string sub: ""
        property bool plain: false       // words, not an account ID
        property bool chosen: false
        signal pressed()
        implicitHeight: sub !== "" ? 58 : 40
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
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Text {
                    Layout.fillWidth: true
                    text: choice.label
                    color: root.ink
                    font.family: choice.plain ? root.sans : root.mono
                    font.pixelSize: choice.plain ? root.px(16) : root.px(14)
                    font.weight: choice.plain ? Font.DemiBold : Font.Normal
                    elide: choice.plain ? Text.ElideRight : Text.ElideMiddle
                }
                Text {
                    visible: choice.sub !== ""
                    Layout.fillWidth: true
                    text: choice.sub
                    color: root.dim
                    font.family: root.sans
                    font.pixelSize: root.px(14)
                    elide: Text.ElideRight
                }
            }
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
        property string hint: ""
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
        Text {
            anchors.fill: input
            verticalAlignment: Text.AlignVCenter
            visible: input.text === ""
            text: field.hint
            color: root.faint
            font.family: root.sans
            font.pixelSize: root.px(16)
            elide: Text.ElideRight
        }
    }

    component Area: Rectangle {
        id: area
        property alias text: areaInput.text
        implicitHeight: 130
        radius: 6
        color: root.slate
        border.width: 1
        border.color: areaInput.activeFocus ? root.brass : root.line
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
                color: root.ink
                selectionColor: root.brass
                selectedTextColor: root.slate
                font.family: root.sans
                font.pixelSize: root.px(17)
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
            color: root.faint
            font.family: root.sans
            font.pixelSize: root.px(17)
        }
    }
}
