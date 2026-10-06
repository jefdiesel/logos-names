// Logos Names for Basecamp: the register of .lez names.
//
// A front end on the `names` module, which reads the chain itself, so browsing
// needs no wallet. Paying goes through `lez_core` (the LEZ wallet) and messaging
// through `chat_module`; each is called only when its button is pressed.
//
// Controls are drawn here rather than taken from Qt Quick Controls so the app
// looks the same whatever style its host applies.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    // ── State ───────────────────────────────────────────────────────────

    property var register: []          // [{name, owner}], newest first
    property int totalNames: 0
    property bool moreNames: false
    property var record: null          // the name on the plate
    property var page: null
    property string missing: ""        // a looked-up name that is not registered
    property var accounts: []          // the wallet's public accounts
    property int fromIndex: 0
    property string registry: ""
    property string sequencer: ""
    property string notice: ""
    property bool noticeIsError: false
    property bool settingsOpen: false
    property bool paying: false
    property string lastReply: ""

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property string chatAddress: record !== null && record.texts && /^[0-9a-f]{64}$/.test(record.texts.chat || "") ? record.texts.chat : ""

    // ── Palette and type ────────────────────────────────────────────────

    readonly property color slate: "#15171C"
    readonly property color panel: "#1C1F26"
    readonly property color raised: "#242832"
    readonly property color line: "#2B303B"
    readonly property color ink: "#EEF0F4"
    readonly property color dim: "#8A91A0"
    readonly property color brass: "#D9B15A"
    readonly property color alarm: "#F08C7E"
    readonly property string serif: "Charter"
    readonly property string sans: "Avenir Next"
    readonly property string mono: "Menlo"

    // ── The Logos bridge ────────────────────────────────────────────────

    /// Calls a module and returns its reply parsed as far as it will go. Modules
    /// answer with JSON text, sometimes wrapped in a JSON string.
    function call(module, method, args) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var reply = logos.callModule(module, method, args)
        lastReply = module + "." + method + " → " + String(reply).substr(0, 400)
        for (var depth = 0; depth < 2 && typeof reply === "string"; depth++) {
            try {
                reply = JSON.parse(reply)
            } catch (error) {
                break
            }
        }
        return reply
    }

    /// Asks the names module a question, returning its value or throwing its error.
    function ask(method, args) {
        var reply = call("names", method, args)
        if (!reply || reply.success === undefined)
            throw new Error("The names module did not answer. Is it installed?")
        if (!reply.success)
            throw new Error(reply.error)
        return reply.value
    }

    function say(message, isError) {
        notice = message
        noticeIsError = !!isError
    }

    // ── Actions ─────────────────────────────────────────────────────────

    function loadConfig() {
        try {
            var config = call("names", "get_config", [])
            registry = config && config.registry ? config.registry : ""
            sequencer = config && config.sequencer ? config.sequencer : ""
            settingsOpen = registry === ""
        } catch (error) {
            say(error.message, true)
        }
    }

    function saveConfig(newSequencer, newRegistry) {
        try {
            if (newSequencer !== sequencer && !call("names", "set_sequencer", [newSequencer]))
                say("That is not an http or https address.", true)
            if (newRegistry !== registry && !call("names", "set_registry", [newRegistry]))
                say("That is not a program ID.", true)
        } catch (error) {
            say(error.message, true)
        }
        loadConfig()
        loadRegister(false)
    }

    /// Loads the register from the newest name, or the next names when `more` is set.
    function loadRegister(more) {
        if (registry === "")
            return
        try {
            var before = more && register.length > 0 ? register[register.length - 1].name : ""
            var listed = ask("list", [40, before])
            register = more ? register.concat(listed.names) : listed.names
            totalNames = listed.total
            moreNames = listed.more
        } catch (error) {
            say(error.message, true)
        }
    }

    /// Shows what was typed: a name, or the name an account ID goes by.
    function find(typed) {
        var text = String(typed).trim()
        if (text === "")
            return
        say("", false)
        try {
            var looksLikeAccount = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(text) && !/\.lez$/i.test(text) && /[A-Z0-9]/.test(text)
            if (looksLikeAccount) {
                var known = ask("reverse", [text])
                if (known === null) {
                    record = null
                    page = null
                    missing = ""
                    say("That account has no name.", false)
                    return
                }
                text = known.name
            }
            show(text)
        } catch (error) {
            say(error.message, true)
        }
    }

    function show(name) {
        paying = false
        try {
            var found = ask("resolve", [name])
            if (found === null) {
                record = null
                page = null
                missing = String(call("names", "normalize", [name])) || name
                return
            }
            missing = ""
            record = found
            page = ask("page", [found.name])
        } catch (error) {
            say(error.message, true)
        }
    }

    function loadAccounts() {
        try {
            var listed = call("lez_core", "list_accounts", [])
            var out = []
            for (var i = 0; listed && i < listed.length; i++) {
                if (!listed[i].is_public)
                    continue
                var id = listed[i].account_id
                out.push({
                    hex: id,
                    shown: String(call("lez_core", "account_id_to_base58", [id])),
                    balance: String(call("lez_core", "get_balance", [id, true]))
                })
            }
            accounts = out
            fromIndex = 0
            return out.length > 0
        } catch (error) {
            accounts = []
            return false
        }
    }

    function openPay() {
        if (!loadAccounts()) {
            say("Paying from here needs the LEZ Wallet app, installed and with a wallet open.", true)
            return
        }
        say("", false)
        paying = true
    }

    /// A whole amount as the 16-byte little-endian hex the wallet expects, or "".
    function amountHex(text) {
        if (!/^[0-9]{1,15}$/.test(text.trim()))
            return ""
        var value = parseInt(text.trim(), 10)
        if (value <= 0)
            return ""
        var hex = ""
        for (var i = 0; i < 16; i++) {
            var low = value % 256
            hex += (low < 16 ? "0" : "") + low.toString(16)
            value = Math.floor(value / 256)
        }
        return hex
    }

    function pay(usePrivateKeys) {
        var amount = amountHex(amountInput.text)
        if (amount === "") {
            say("Enter a whole amount.", true)
            return
        }
        try {
            // Ask again at the moment of paying, so a name changed since it was shown is not paid stale.
            var to = ask("recipient", [record.name])
            if (to === null)
                throw new Error(record.display + " is no longer registered.")
            var from = accounts[fromIndex].hex
            var reply
            if (usePrivateKeys) {
                if (to.private === null)
                    throw new Error(record.display + " does not take private payments.")
                reply = call("lez_core", "transfer_shielded", [from, JSON.stringify(to.private), amount])
            } else {
                if (to.public_hex === null)
                    throw new Error(record.display + " does not take public payments.")
                reply = call("lez_core", "transfer_public", [from, to.public_hex, amount])
            }
            if (reply && reply.success) {
                paying = false
                say("Sent " + amountInput.text.trim() + " to " + record.display + ".", false)
            } else {
                say("The wallet refused: " + (reply && reply.error ? reply.error : "no reason given") + ".", true)
            }
        } catch (error) {
            say(error.message, true)
        }
    }

    function message() {
        try {
            var reply = call("chat_module", "create_conversation", [chatAddress])
            if (reply && reply.success === false) {
                say("Chat could not start the conversation: " + reply.error, true)
                return
            }
            say("Conversation with " + record.display + " started. Opening Chat.", false)
            if (logos.request)
                logos.request("basecamp.apps.launch", { app: "chat_ui" }, function() {})
        } catch (error) {
            say(error.message, true)
        }
    }

    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.", false)
    }

    function shorten(id) {
        return id.length > 16 ? id.substr(0, 8) + "…" + id.substr(id.length - 6) : id
    }

    function linkFor(name) {
        return "basecamp://intent/names.open?p=" + Qt.btoa(JSON.stringify({ name: name + ".lez" })).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
    }

    /// Digits grouped in threes, so a balance can be read at a glance.
    function grouped(digits) {
        return String(digits).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    /// The readable words of a page. Markup is removed, never rendered: a page is
    /// its owner's words, and nothing in it should be able to act inside this app.
    function pageWords() {
        if (page === null || page.text === null)
            return ""
        var words = page.text
        if (/html/i.test(page.content_type)) {
            words = words.replace(/<(script|style|title|head)[^>]*>[\s\S]*?<\/\1>/gi, " ")
                         .replace(/<\/(p|div|h[1-6]|li|tr|section|article)>|<br\s*\/?>/gi, "\n")
                         .replace(/<[^>]*>/g, "")
                         .replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&#39;|&apos;/g, "'").replace(/&nbsp;/g, " ")
        }
        return words.replace(/[ \t]+/g, " ").replace(/\s*\n\s*/g, "\n").trim().substr(0, 2400)
    }

    function textKeys() {
        return record !== null && record.texts ? Object.keys(record.texts) : []
    }

    /// A stable number from a string, for drawing an owner's seal.
    function seedOf(text) {
        var hash = 2166136261
        for (var i = 0; i < text.length; i++) {
            hash ^= text.charCodeAt(i)
            hash = (hash * 16777619) >>> 0
        }
        return hash
    }

    Component.onCompleted: {
        if (!bridgeReady)
            return
        loadConfig()
        loadRegister(false)
    }

    // `names.open` puts a name on screen for another app, or for a
    // `basecamp://intent/names.open?p=<base64url of {"name": "jef.lez"}>` link.
    // The name arrives from outside, so it is only ever shown, never acted on.
    Connections {
        target: root.bridgeReady ? logos : null
        ignoreUnknownSignals: true
        function onIntentRequested(requestId, intent, params, requesterName) {
            if (intent !== "names.open")
                return
            var wanted = String(params && params.name ? params.name : "")
            searchField.text = wanted
            root.find(wanted)
            logos.respond(requestId, !root.noticeIsError, { name: wanted, registered: root.record !== null }, root.noticeIsError ? root.notice : "")
        }
    }

    TextEdit { id: clipboard; visible: false }

    // ── Parts ───────────────────────────────────────────────────────────

    /// An owner's seal: rings cut from the owner's account ID, so every name one
    /// owner holds wears the same seal and a look-alike name wears another.
    component Seal: Canvas {
        id: seal
        property string owner: ""
        property string letter: ""
        onOwnerChanged: requestPaint()
        onWidthChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            if (owner === "")
                return
            var state = root.seedOf(owner)
            function next() {
                state ^= state << 13; state >>>= 0
                state ^= state >>> 17
                state ^= state << 5; state >>>= 0
                return state / 4294967296
            }
            var size = Math.min(width, height)
            var centre = size / 2
            var hue = Math.floor(next() * 360)
            ctx.lineCap = "butt"
            for (var ring = 0; ring < 4; ring++) {
                var radius = centre * (0.94 - ring * 0.17)
                var cuts = 3 + Math.floor(next() * 9)
                var turn = next() * Math.PI * 2
                var gap = 0.10 + next() * 0.22
                ctx.lineWidth = Math.max(1.5, size * (0.035 + next() * 0.05))
                ctx.strokeStyle = Qt.hsla(((hue + ring * 24) % 360) / 360, 0.52, 0.66, 0.95 - ring * 0.17)
                for (var cut = 0; cut < cuts; cut++) {
                    var from = turn + (cut / cuts) * Math.PI * 2
                    ctx.beginPath()
                    ctx.arc(centre, centre, radius - ctx.lineWidth / 2, from, from + (Math.PI * 2 / cuts) * (1 - gap))
                    ctx.stroke()
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: seal.letter !== "" && seal.width >= 60
            text: seal.letter.toUpperCase()
            color: root.ink
            font.family: root.serif
            font.pixelSize: seal.width * 0.22
        }
    }

    component Action: Rectangle {
        id: action
        property string label: ""
        property bool strong: false
        property bool enabledNow: true
        signal pressed()
        implicitWidth: actionText.implicitWidth + 28
        implicitHeight: 34
        radius: 6
        color: !enabledNow ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(root.brass, 1.08) : root.brass) : (actionArea.containsMouse ? root.raised : "transparent")
        border.width: strong && enabledNow ? 0 : 1
        border.color: activeFocus ? root.brass : root.line
        opacity: enabledNow ? 1 : 0.45
        activeFocusOnTab: enabledNow
        Keys.onReturnPressed: if (enabledNow) action.pressed()
        Keys.onSpacePressed: if (enabledNow) action.pressed()
        Text {
            id: actionText
            anchors.centerIn: parent
            text: action.label
            color: action.strong && action.enabledNow ? root.slate : root.ink
            font.family: root.sans
            font.pixelSize: 13
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

    component Field: Rectangle {
        id: field
        property alias text: input.text
        property string hint: ""
        property bool numbersOnly: false
        property int size: 14
        signal accepted()
        signal edited()
        implicitHeight: size + 22
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
            font.family: root.sans
            font.pixelSize: field.size
            clip: true
            selectByMouse: true
            validator: field.numbersOnly ? digits : null
            onAccepted: field.accepted()
            onEditingFinished: field.edited()
        }
        RegularExpressionValidator { id: digits; regularExpression: /[0-9]{0,15}/ }
        Text {
            anchors.fill: input
            verticalAlignment: Text.AlignVCenter
            visible: input.text === ""
            text: field.hint
            color: root.dim
            font.family: root.sans
            font.pixelSize: field.size
            elide: Text.ElideRight
        }
    }

    /// One labelled fact on the plate, with a button to copy the full value.
    component Fact: RowLayout {
        id: fact
        property string label: ""
        property string value: ""
        property string full: ""
        property bool code: false
        Layout.fillWidth: true
        spacing: 12
        Text {
            Layout.preferredWidth: 110
            text: fact.label
            color: root.dim
            font.family: root.sans
            font.pixelSize: 13
        }
        Text {
            Layout.fillWidth: true
            text: fact.value
            color: root.ink
            font.family: fact.code ? root.mono : root.sans
            font.pixelSize: 13
            wrapMode: Text.WrapAnywhere
        }
        Text {
            visible: fact.full !== ""
            text: "Copy"
            color: copyArea.containsMouse ? root.brass : root.dim
            font.family: root.sans
            font.pixelSize: 12
            MouseArea {
                id: copyArea
                anchors.fill: parent
                anchors.margins: -6
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.copy(fact.full, fact.label)
            }
        }
    }

    // ── Layout ──────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // The register.
        Rectangle {
            Layout.preferredWidth: 264
            Layout.fillHeight: true
            color: root.panel

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 18
                spacing: 14

                Field {
                    id: searchField
                    Layout.fillWidth: true
                    hint: "Find a name or account"
                    onAccepted: root.find(searchField.text)
                }

                Text {
                    text: root.registry === "" ? "No register chosen" : root.totalNames === 1 ? "1 name" : root.totalNames + " names"
                    color: root.dim
                    font.family: root.sans
                    font.pixelSize: 12
                }

                ListView {
                    id: registerList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 2
                    model: root.register
                    delegate: Rectangle {
                        id: row
                        required property var modelData
                        readonly property bool current: root.record !== null && root.record.name === modelData.name
                        width: registerList.width
                        height: 40
                        radius: 6
                        color: current ? root.raised : rowArea.containsMouse ? "#20232B" : "transparent"
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            spacing: 10
                            Seal {
                                Layout.preferredWidth: 24
                                Layout.preferredHeight: 24
                                owner: row.modelData.owner
                            }
                            Text {
                                Layout.fillWidth: true
                                textFormat: Text.StyledText
                                text: row.modelData.name + "<font color=\"" + root.dim + "\">.lez</font>"
                                color: root.ink
                                font.family: root.serif
                                font.pixelSize: 16
                                elide: Text.ElideRight
                            }
                        }
                        MouseArea {
                            id: rowArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.say("", false)
                                root.show(row.modelData.name)
                            }
                        }
                    }
                    footer: Item {
                        width: registerList.width
                        height: root.moreNames ? 44 : 0
                        visible: root.moreNames
                        Action {
                            anchors.centerIn: parent
                            label: "Show older names"
                            onPressed: root.loadRegister(true)
                        }
                    }
                }

                Text {
                    text: root.settingsOpen ? "Close settings" : "Settings"
                    color: settingsArea.containsMouse ? root.brass : root.dim
                    font.family: root.sans
                    font.pixelSize: 12
                    MouseArea {
                        id: settingsArea
                        anchors.fill: parent
                        anchors.margins: -6
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.settingsOpen = !root.settingsOpen
                    }
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }

        // The plate.
        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: plate.implicitHeight + 72
            clip: true

            ColumnLayout {
                id: plate
                x: 40
                y: 36
                width: Math.min(parent.width - 80, 640)
                spacing: 22

                // Settings.
                ColumnLayout {
                    visible: root.settingsOpen
                    Layout.fillWidth: true
                    spacing: 10
                    Text {
                        text: "Which register"
                        color: root.ink
                        font.family: root.serif
                        font.pixelSize: 22
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "A register is one program on the Logos Execution Zone. Everyone who uses the same program ID sees the same names."
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                    }
                    Field {
                        id: registryField
                        Layout.fillWidth: true
                        size: 13
                        text: root.registry
                        hint: "Program ID of the register"
                        onEdited: root.saveConfig(root.sequencer, registryField.text.trim())
                    }
                    Field {
                        id: sequencerField
                        Layout.fillWidth: true
                        size: 13
                        text: root.sequencer
                        hint: "https://testnet.lez.logos.co"
                        onEdited: root.saveConfig(sequencerField.text.trim(), root.registry)
                    }
                    Text {
                        visible: root.lastReply !== ""
                        Layout.fillWidth: true
                        text: root.lastReply
                        color: root.dim
                        font.family: root.mono
                        font.pixelSize: 10
                        wrapMode: Text.WrapAnywhere
                    }
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: 8; height: 1; color: root.line }
                }

                Text {
                    visible: root.notice !== ""
                    Layout.fillWidth: true
                    text: root.notice
                    color: root.noticeIsError ? root.alarm : root.brass
                    font.family: root.sans
                    font.pixelSize: 13
                    wrapMode: Text.Wrap
                }

                // Nothing chosen yet.
                ColumnLayout {
                    visible: root.record === null && root.missing === "" && !root.settingsOpen
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 10
                    Text {
                        text: root.registry === "" ? "Choose a register to begin" : "The register of .lez names"
                        color: root.ink
                        font.family: root.serif
                        font.pixelSize: 30
                    }
                    Text {
                        Layout.fillWidth: true
                        text: root.registry === "" ? "Open Settings and enter the register's program ID." : "Pick a name on the left, or type one to see who holds it, where it pays and how to reach them."
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 14
                        wrapMode: Text.Wrap
                    }
                }

                // A name nobody holds.
                ColumnLayout {
                    visible: root.missing !== ""
                    Layout.fillWidth: true
                    Layout.topMargin: 24
                    spacing: 12
                    Text {
                        textFormat: Text.StyledText
                        text: root.missing + "<font color=\"" + root.dim + "\">.lez</font>"
                        color: root.dim
                        font.family: root.serif
                        font.pixelSize: 46
                    }
                    Text {
                        text: "Nobody holds this name."
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: 15
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Names go to whoever registers them first. Claiming from this screen is not built yet; from a terminal it is one command."
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                    }
                    Action {
                        label: "Copy the command"
                        onPressed: root.copy("names register " + root.missing + ".lez --owner <your account>", "Command")
                    }
                }

                // A registered name.
                ColumnLayout {
                    visible: root.record !== null
                    Layout.fillWidth: true
                    spacing: 22

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 22
                        Seal {
                            Layout.preferredWidth: 104
                            Layout.preferredHeight: 104
                            owner: root.record !== null ? root.record.owner : ""
                            letter: root.record !== null ? root.record.name.charAt(0) : ""
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                Layout.fillWidth: true
                                textFormat: Text.StyledText
                                text: root.record !== null ? root.record.name + "<font color=\"" + root.dim + "\">.lez</font>" : ""
                                color: root.ink
                                font.family: root.serif
                                font.pixelSize: 46
                                elide: Text.ElideRight
                            }
                            Text {
                                text: root.record === null ? "" : root.record.npk !== null && root.record.account !== null ? "Takes public and private payments" : root.record.npk !== null ? "Takes private payments" : root.record.account !== null ? "Takes public payments" : "Does not take payments yet"
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: 14
                            }
                        }
                    }

                    RowLayout {
                        spacing: 8
                        Action {
                            label: "Send"
                            strong: true
                            enabledNow: root.record !== null && (root.record.account !== null || root.record.npk !== null)
                            onPressed: root.openPay()
                        }
                        Action {
                            label: "Message"
                            enabledNow: root.chatAddress !== ""
                            onPressed: root.message()
                        }
                        Action {
                            label: "Copy link"
                            onPressed: root.copy(root.linkFor(root.record.name), "Link")
                        }
                    }

                    // Paying.
                    Rectangle {
                        visible: root.paying
                        Layout.fillWidth: true
                        implicitHeight: payColumn.implicitHeight + 32
                        radius: 8
                        color: root.panel
                        border.width: 1
                        border.color: root.line
                        ColumnLayout {
                            id: payColumn
                            anchors.fill: parent
                            anchors.margins: 16
                            spacing: 12
                            Text {
                                text: root.record !== null ? "Send to " + root.record.display : ""
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: 15
                                font.weight: Font.DemiBold
                            }
                            Repeater {
                                model: root.accounts
                                Rectangle {
                                    id: accountRow
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true
                                    height: 34
                                    radius: 6
                                    color: root.fromIndex === index ? root.raised : "transparent"
                                    border.width: 1
                                    border.color: root.fromIndex === index ? root.brass : root.line
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 12
                                        anchors.rightMargin: 12
                                        Text { text: root.shorten(accountRow.modelData.shown); color: root.ink; font.family: root.mono; font.pixelSize: 12 }
                                        Item { Layout.fillWidth: true }
                                        Text { text: root.grouped(accountRow.modelData.balance); color: root.dim; font.family: root.sans; font.pixelSize: 12 }
                                    }
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.fromIndex = accountRow.index }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Field {
                                    id: amountInput
                                    Layout.preferredWidth: 160
                                    hint: "Amount"
                                    numbersOnly: true
                                }
                                Action {
                                    label: "Send publicly"
                                    strong: true
                                    enabledNow: root.record !== null && root.record.account !== null
                                    onPressed: root.pay(false)
                                }
                                Action {
                                    label: "Send privately"
                                    enabledNow: root.record !== null && root.record.npk !== null
                                    onPressed: root.pay(true)
                                }
                                Item { Layout.fillWidth: true }
                                Action { label: "Cancel"; onPressed: root.paying = false }
                            }
                            Text {
                                Layout.fillWidth: true
                                text: "A private payment hides the amount and where it went. It takes a few minutes to prepare."
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: 12
                                wrapMode: Text.Wrap
                            }
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: root.line }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Fact {
                            label: "Held by"
                            code: true
                            value: root.record !== null ? root.shorten(root.record.owner) : ""
                            full: root.record !== null ? root.record.owner : ""
                        }
                        Fact {
                            visible: root.record !== null && root.record.account !== null
                            label: "Pays to"
                            code: true
                            value: root.record !== null && root.record.account !== null ? root.shorten(root.record.account) : ""
                            full: root.record !== null && root.record.account !== null ? root.record.account : ""
                        }
                        Repeater {
                            model: root.textKeys()
                            Fact {
                                required property string modelData
                                label: modelData
                                value: root.record !== null ? root.record.texts[modelData] : ""
                                full: value
                            }
                        }
                        Fact {
                            visible: root.record !== null && root.record.previous !== null
                            label: "Registered after"
                            value: root.record !== null && root.record.previous !== null ? root.record.previous + ".lez" : ""
                        }
                    }

                    // The name's page.
                    ColumnLayout {
                        visible: root.page !== null
                        Layout.fillWidth: true
                        spacing: 8
                        Rectangle { Layout.fillWidth: true; height: 1; color: root.line }
                        Text {
                            text: root.record !== null ? "On " + root.record.name + "'s page" : ""
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: 13
                        }
                        Text {
                            Layout.fillWidth: true
                            Layout.maximumHeight: 300
                            clip: true
                            textFormat: Text.PlainText
                            text: root.pageWords() !== "" ? root.pageWords() : "The page holds no readable text."
                            color: root.ink
                            font.family: root.serif
                            font.pixelSize: 16
                            lineHeight: 1.35
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }
    }
}
