// Logos Names for Basecamp: look up a .lez name, then pay or message it.
//
// A thin front end. Lookups go through the `names` module, which reads the
// chain itself, so this view needs no wallet to show a name. Paying goes through
// `lez_core` (the LEZ wallet) and messaging through `chat_module`; each is used
// only when its button is pressed, and the view says so if one is missing.

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    property var record: null
    property var page: null
    property var accounts: []
    property var recentNames: []
    property int totalNames: 0
    property string registry: ""
    property string sequencer: ""
    property string status: ""
    property bool failed: false
    property string details: ""

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property string chatAddress: record !== null && record.texts && /^[0-9a-f]{64}$/.test(record.texts.chat || "") ? record.texts.chat : ""

    // ── The Logos bridge ────────────────────────────────────────────────

    /// Calls a module and returns its reply parsed as far as it will go. Modules
    /// answer with JSON text, sometimes wrapped in a JSON string.
    function call(module, method, args) {
        if (!bridgeReady)
            throw new Error("the Logos bridge is not available")
        var reply = logos.callModule(module, method, args)
        details = module + "." + method + " -> " + String(reply).substr(0, 500)
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
            throw new Error("the names module did not answer; is it installed?")
        if (!reply.success)
            throw new Error(reply.error)
        return reply.value
    }

    function report(message, isError) {
        status = message
        failed = !!isError
    }

    // ── Actions ─────────────────────────────────────────────────────────

    function loadConfig() {
        try {
            var config = call("names", "get_config", [])
            registry = config && config.registry ? config.registry : ""
            sequencer = config && config.sequencer ? config.sequencer : ""
            if (registry === "")
                report("Set the registry's program ID below to start.", false)
        } catch (error) {
            report(error.message, true)
        }
    }

    function saveConfig(newSequencer, newRegistry) {
        try {
            if (newSequencer !== sequencer && !call("names", "set_sequencer", [newSequencer]))
                report("That is not an http or https URL.", true)
            if (newRegistry !== registry && !call("names", "set_registry", [newRegistry]))
                report("That is not a program ID.", true)
        } catch (error) {
            report(error.message, true)
        }
        loadConfig()
    }

    function lookUp() {
        record = null
        page = null
        try {
            var found = ask("resolve", [nameField.text])
            if (found === null) {
                report(nameField.text.trim() + " is not registered.", false)
                return
            }
            record = found
            page = ask("page", [found.name])
            report("", false)
        } catch (error) {
            report(error.message, true)
        }
    }

    function listRecent() {
        try {
            var listed = ask("list", [20, ""])
            recentNames = listed.names
            totalNames = listed.total
            report(listed.names.length === 0 ? "No names are registered yet." : "", false)
        } catch (error) {
            report(error.message, true)
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
                var shown = String(call("lez_core", "account_id_to_base58", [id]))
                var balance = call("lez_core", "get_balance", [id, true])
                out.push({ hex: id, label: shown.substr(0, 10) + "…  " + String(balance) })
            }
            accounts = out
            if (out.length === 0)
                report("The LEZ wallet has no public accounts to pay from. Is it installed and open?", true)
        } catch (error) {
            report(error.message, true)
        }
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
        if (record === null)
            return
        var amount = amountHex(amountField.text)
        if (amount === "") {
            report("Enter a whole amount in base units.", true)
            return
        }
        if (fromBox.currentIndex < 0 || accounts.length === 0) {
            report("Choose an account to pay from.", true)
            return
        }
        var from = accounts[fromBox.currentIndex].hex
        try {
            // Ask again at the moment of paying, so a name changed since the lookup is not paid stale.
            var to = ask("recipient", [record.name])
            if (to === null)
                throw new Error(record.display + " is no longer registered")
            var reply
            if (usePrivateKeys) {
                if (to.private === null)
                    throw new Error(record.display + " does not accept private payments")
                report("Generating the proof; this can take a few minutes.", false)
                reply = call("lez_core", "transfer_shielded", [from, JSON.stringify(to.private), amount])
            } else {
                if (to.public_hex === null)
                    throw new Error(record.display + " does not accept public payments")
                reply = call("lez_core", "transfer_public", [from, to.public_hex, amount])
            }
            if (reply && reply.success)
                report("Sent to " + record.display + ". Transaction " + reply.tx_hash, false)
            else
                report("The wallet refused the transfer: " + (reply && reply.error ? reply.error : "no reason given"), true)
        } catch (error) {
            report(error.message, true)
        }
    }

    function message() {
        try {
            var reply = call("chat_module", "create_conversation", [chatAddress])
            if (reply && reply.success === false) {
                report("Chat could not start the conversation: " + reply.error, true)
                return
            }
            report("Conversation started with " + record.display + ". Opening Chat.", false)
            if (logos.request)
                logos.request("basecamp.apps.launch", { app: "chat_ui" }, function() {})
        } catch (error) {
            report(error.message, true)
        }
    }

    function textKeys() {
        return record !== null && record.texts ? Object.keys(record.texts) : []
    }

    Component.onCompleted: if (bridgeReady) loadConfig()

    // `names.open` puts a name on screen for another app, or for a
    // `basecamp://intent/names.open?p=<base64url of {"name": "jef.lez"}>` link.
    // The name arrives from outside, so it is only ever looked up, never acted on.
    Connections {
        target: root.bridgeReady ? logos : null
        ignoreUnknownSignals: true
        function onIntentRequested(requestId, intent, params, requesterName) {
            if (intent !== "names.open")
                return
            nameField.text = String(params && params.name ? params.name : "")
            root.lookUp()
            var found = root.record !== null
            console.log("names_ui: names.open", nameField.text, found ? "found" : "not shown:", root.status)
            logos.respond(requestId, !root.failed, { name: nameField.text, registered: found }, root.failed ? root.status : "")
        }
    }

    // ── View ────────────────────────────────────────────────────────────

    Rectangle {
        anchors.fill: parent
        color: "#0f1115"
    }

    ScrollView {
        anchors.fill: parent
        anchors.margins: 24
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: 14

            Text {
                text: "Names"
                color: "#f2f4f8"
                font.pixelSize: 26
                font.bold: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                TextField {
                    id: nameField
                    Layout.fillWidth: true
                    placeholderText: "jef.lez"
                    font.pixelSize: 16
                    onAccepted: root.lookUp()
                }
                Button {
                    text: "Look up"
                    onClicked: root.lookUp()
                }
                Button {
                    text: "Recent"
                    onClicked: root.listRecent()
                }
            }

            Text {
                visible: root.status.length > 0
                Layout.fillWidth: true
                text: root.status
                color: root.failed ? "#ff8b8b" : "#9ad1a5"
                wrapMode: Text.Wrap
                font.pixelSize: 13
            }

            Text {
                visible: root.recentNames.length > 0
                text: root.totalNames + " names registered"
                color: "#8b93a3"
                font.pixelSize: 12
            }

            Flow {
                visible: root.recentNames.length > 0
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: root.recentNames
                    Button {
                        required property var modelData
                        text: modelData.name + ".lez"
                        flat: true
                        onClicked: {
                            nameField.text = modelData.name + ".lez"
                            root.lookUp()
                        }
                    }
                }
            }

            Rectangle {
                visible: root.record !== null
                Layout.fillWidth: true
                implicitHeight: recordColumn.implicitHeight + 28
                radius: 10
                color: "#171a21"
                border.color: "#262b36"

                ColumnLayout {
                    id: recordColumn
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 8

                    Text {
                        text: root.record !== null ? root.record.display : ""
                        color: "#f2f4f8"
                        font.pixelSize: 20
                        font.bold: true
                    }

                    GridLayout {
                        Layout.fillWidth: true
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 4

                        Text { text: "Owner"; color: "#8b93a3"; font.pixelSize: 12 }
                        Text {
                            Layout.fillWidth: true
                            text: root.record !== null ? root.record.owner : ""
                            color: "#d5d9e2"; font.pixelSize: 12; font.family: "Menlo"; wrapMode: Text.WrapAnywhere
                        }
                        Text { text: "Pays to"; color: "#8b93a3"; font.pixelSize: 12 }
                        Text {
                            Layout.fillWidth: true
                            text: root.record === null ? "" : (root.record.account !== null ? root.record.account : "no public account")
                            color: "#d5d9e2"; font.pixelSize: 12; font.family: "Menlo"; wrapMode: Text.WrapAnywhere
                        }
                        Text { text: "Private"; color: "#8b93a3"; font.pixelSize: 12 }
                        Text {
                            text: root.record !== null && root.record.npk !== null ? "accepts private payments" : "no private keys"
                            color: "#d5d9e2"; font.pixelSize: 12
                        }
                        Text { text: "Page"; color: "#8b93a3"; font.pixelSize: 12 }
                        Text {
                            text: root.page !== null ? root.page.content_type + ", " + root.page.bytes + " bytes" : "none"
                            color: "#d5d9e2"; font.pixelSize: 12
                        }
                    }

                    Repeater {
                        model: root.textKeys()
                        RowLayout {
                            required property string modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { text: modelData; color: "#8b93a3"; font.pixelSize: 12 }
                            Text {
                                Layout.fillWidth: true
                                text: root.record !== null ? root.record.texts[modelData] : ""
                                color: "#d5d9e2"; font.pixelSize: 12; wrapMode: Text.WrapAnywhere
                            }
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: "#262b36" }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        ComboBox {
                            id: fromBox
                            Layout.preferredWidth: 260
                            model: root.accounts
                            textRole: "label"
                            displayText: root.accounts.length === 0 ? "Load accounts" : fromBox.currentText
                            onPressedChanged: if (fromBox.pressed && root.accounts.length === 0) root.loadAccounts()
                        }
                        TextField {
                            id: amountField
                            Layout.preferredWidth: 140
                            placeholderText: "Amount"
                            validator: RegularExpressionValidator { regularExpression: /[0-9]{0,15}/ }
                        }
                        Button {
                            text: "Send"
                            enabled: root.record !== null && root.record.account !== null
                            onClicked: root.pay(false)
                        }
                        Button {
                            text: "Send privately"
                            enabled: root.record !== null && root.record.npk !== null
                            onClicked: root.pay(true)
                        }
                        Item { Layout.fillWidth: true }
                        Button {
                            text: "Message"
                            enabled: root.chatAddress.length > 0
                            onClicked: root.message()
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: settingsColumn.implicitHeight + 24
                radius: 10
                color: "#12151b"
                border.color: "#1f2430"

                GridLayout {
                    id: settingsColumn
                    anchors.fill: parent
                    anchors.margins: 12
                    columns: 2
                    columnSpacing: 10
                    rowSpacing: 6

                    Text { text: "Registry"; color: "#8b93a3"; font.pixelSize: 12 }
                    TextField {
                        id: registryField
                        Layout.fillWidth: true
                        text: root.registry
                        placeholderText: "program ID of the names registry"
                        font.pixelSize: 12
                        onEditingFinished: root.saveConfig(root.sequencer, registryField.text.trim())
                    }
                    Text { text: "Sequencer"; color: "#8b93a3"; font.pixelSize: 12 }
                    TextField {
                        id: sequencerField
                        Layout.fillWidth: true
                        text: root.sequencer
                        placeholderText: "https://testnet.lez.logos.co"
                        font.pixelSize: 12
                        onEditingFinished: root.saveConfig(sequencerField.text.trim(), root.registry)
                    }
                }
            }

            Text {
                visible: !root.bridgeReady
                text: "Running outside Basecamp: lookups need the Logos bridge."
                color: "#ffcf7a"
                font.pixelSize: 12
            }

            Text {
                visible: root.details.length > 0
                Layout.fillWidth: true
                text: root.details
                color: "#5c6473"
                font.pixelSize: 10
                font.family: "Menlo"
                wrapMode: Text.WrapAnywhere
            }
        }
    }
}
