// Logos Names for Basecamp: look up a .lez name, then pay or message it.
//
// A Basecamp view has no network access, so every chain read and transfer goes
// through the `lez_core` module (the LEZ wallet), which must be installed and
// have a wallet open. SHA-256, base58 and borsh are implemented here because the
// QML engine provides none of them, and it has no BigInt or TextDecoder either.

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    // Account ID (base58) of the registry program on the network the wallet uses.
    property string programId: ""
    property var record: null
    property var page: null
    property var accounts: []
    property var recentNames: []
    property string status: ""
    property bool failed: false
    property string details: ""

    readonly property string nameSuffix: ".lez"
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule

    // ── Bytes, hex and text ─────────────────────────────────────────────

    function hexToBytes(hex) {
        var out = []
        for (var i = 0; i + 1 < hex.length; i += 2)
            out.push(parseInt(hex.substr(i, 2), 16))
        return out
    }

    function bytesToHex(bytes) {
        var hex = ""
        for (var i = 0; i < bytes.length; i++)
            hex += (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16)
        return hex
    }

    function asciiBytes(text) {
        var out = []
        for (var i = 0; i < text.length; i++)
            out.push(text.charCodeAt(i) & 0xff)
        return out
    }

    function utf8Decode(bytes) {
        var encoded = ""
        for (var i = 0; i < bytes.length; i++)
            encoded += "%" + (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16)
        try {
            return decodeURIComponent(encoded)
        } catch (error) {
            return "(not valid text)"
        }
    }

    // ── SHA-256 ─────────────────────────────────────────────────────────

    readonly property var sha256Constants: [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
    ]

    function sha256(bytes) {
        function rotr(value, bits) { return (value >>> bits) | (value << (32 - bits)) }

        var message = bytes.slice()
        var bitLength = bytes.length * 8
        message.push(0x80)
        while (message.length % 64 !== 56)
            message.push(0)
        // Messages here are far below 2^32 bits, so the high word of the length is zero.
        message.push(0, 0, 0, 0,
                     (bitLength >>> 24) & 0xff, (bitLength >>> 16) & 0xff, (bitLength >>> 8) & 0xff, bitLength & 0xff)

        var h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
        var k = sha256Constants
        var w = new Array(64)
        for (var offset = 0; offset < message.length; offset += 64) {
            for (var t = 0; t < 16; t++) {
                var at = offset + t * 4
                w[t] = (message[at] << 24) | (message[at + 1] << 16) | (message[at + 2] << 8) | message[at + 3]
            }
            for (t = 16; t < 64; t++) {
                var s0 = rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >>> 3)
                var s1 = rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >>> 10)
                w[t] = (w[t - 16] + s0 + w[t - 7] + s1) | 0
            }
            var a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7]
            for (t = 0; t < 64; t++) {
                var e1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)
                var temp1 = (hh + e1 + ((e & f) ^ (~e & g)) + k[t] + w[t]) | 0
                var a0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)
                var temp2 = (a0 + ((a & b) ^ (a & c) ^ (b & c))) | 0
                hh = g; g = f; f = e; e = (d + temp1) | 0
                d = c; c = b; b = a; a = (temp1 + temp2) | 0
            }
            h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0
            h[4] = (h[4] + e) | 0; h[5] = (h[5] + f) | 0; h[6] = (h[6] + g) | 0; h[7] = (h[7] + hh) | 0
        }
        var out = []
        for (var i = 0; i < 8; i++)
            out.push((h[i] >>> 24) & 0xff, (h[i] >>> 16) & 0xff, (h[i] >>> 8) & 0xff, h[i] & 0xff)
        return out
    }

    // ── Base58 ──────────────────────────────────────────────────────────

    readonly property string base58Alphabet: "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

    function base58Encode(bytes) {
        var digits = [0]
        for (var i = 0; i < bytes.length; i++) {
            var carry = bytes[i]
            for (var j = 0; j < digits.length; j++) {
                carry += digits[j] << 8
                digits[j] = carry % 58
                carry = (carry / 58) | 0
            }
            while (carry > 0) {
                digits.push(carry % 58)
                carry = (carry / 58) | 0
            }
        }
        // Leading zero bytes are written as '1's, so drop the number's own leading zero digit.
        while (digits.length > 0 && digits[digits.length - 1] === 0)
            digits.pop()
        var text = ""
        for (i = 0; i < bytes.length && bytes[i] === 0; i++)
            text += "1"
        for (i = digits.length - 1; i >= 0; i--)
            text += base58Alphabet[digits[i]]
        return text
    }

    function base58Decode(text) {
        var bytes = [0]
        for (var i = 0; i < text.length; i++) {
            var carry = base58Alphabet.indexOf(text[i])
            if (carry < 0)
                return null
            for (var j = 0; j < bytes.length; j++) {
                carry += bytes[j] * 58
                bytes[j] = carry & 0xff
                carry >>= 8
            }
            while (carry > 0) {
                bytes.push(carry & 0xff)
                carry >>= 8
            }
        }
        for (i = 0; i < text.length && text[i] === "1"; i++)
            bytes.push(0)
        return bytes.reverse()
    }

    /// Hex of a base58 account ID, or "" unless it is a well-formed 32-byte ID.
    function accountHex(base58) {
        var bytes = base58Decode(base58)
        if (bytes === null)
            return ""
        while (bytes.length > 32 && bytes[0] === 0)
            bytes.shift()
        while (bytes.length < 32)
            bytes.unshift(0)
        return bytes.length === 32 ? bytesToHex(bytes) : ""
    }

    // ── Names and where they live (must match core/src/lib.rs) ──────────

    function normalizeName(typed) {
        var lowered = String(typed).trim().toLowerCase()
        var cut = lowered.length - nameSuffix.length
        return cut >= 0 && lowered.substr(cut) === nameSuffix ? lowered.substr(0, cut) : lowered
    }

    function isValidName(name) {
        return /^[a-z0-9-]{1,32}$/.test(name) && name[0] !== "-" && name[name.length - 1] !== "-"
    }

    function derivedAddressHex(seedText) {
        var prefix = asciiBytes("/LEE/v0.2/AccountId/PDA/")
        while (prefix.length < 32)
            prefix.push(0)
        var seed = sha256(asciiBytes(seedText))
        return bytesToHex(sha256(prefix.concat(hexToBytes(accountHex(programId)), seed)))
    }

    function recordAddressHex(name) { return derivedAddressHex("logos-names/v1/name/" + name) }
    function pageAddressHex(name) { return derivedAddressHex("logos-names/v1/page/" + name) }
    function directoryAddressHex() { return derivedAddressHex("logos-names/v1/directory") }

    // ── Borsh decoding ──────────────────────────────────────────────────

    function reader(bytes) {
        var at = 0
        var self = {
            take: function(length) {
                if (at + length > bytes.length)
                    throw new Error("record is truncated")
                var slice = bytes.slice(at, at + length)
                at += length
                return slice
            },
            u8: function() { return self.take(1)[0] },
            u32: function() {
                var b = self.take(4)
                return b[0] + b[1] * 256 + b[2] * 65536 + b[3] * 16777216
            },
            u64: function() {
                var low = self.u32()
                return self.u32() * 4294967296 + low
            },
            byteVec: function() { return self.take(self.u32()) },
            string: function() { return utf8Decode(self.byteVec()) },
            option: function(read) { return self.u8() === 0 ? null : read() }
        }
        return self
    }

    function decodeRecord(bytes) {
        var r = reader(bytes)
        var out = { version: r.u8(), name: r.string(), owner: base58Encode(r.take(32)) }
        out.account = r.option(function() { return base58Encode(r.take(32)) })
        out.npk = r.option(function() { return bytesToHex(r.take(32)) })
        out.vpk = bytesToHex(r.byteVec())
        out.texts = []
        var count = r.u32()
        for (var i = 0; i < count; i++) {
            var key = r.string()
            out.texts.push({ key: key, value: r.string() })
        }
        out.previous = r.option(function() { return r.string() })
        return out
    }

    function decodePage(bytes) {
        var r = reader(bytes)
        return { version: r.u8(), contentType: r.string(), body: r.byteVec() }
    }

    function decodeDirectory(bytes) {
        var r = reader(bytes)
        return { version: r.u8(), count: r.u64(), latest: r.option(function() { return r.string() }) }
    }

    // ── The Logos bridge ────────────────────────────────────────────────

    /// Calls a module and returns its reply parsed as far as it will go. Modules
    /// answer with JSON text, sometimes wrapped in a JSON string.
    function call(module, method, args) {
        if (!bridgeReady)
            throw new Error("the Logos bridge is not available")
        var reply = logos.callModule(module, method, args)
        details = module + "." + method + " -> " + String(reply).substr(0, 600)
        for (var depth = 0; depth < 2 && typeof reply === "string"; depth++) {
            try {
                reply = JSON.parse(reply)
            } catch (error) {
                break
            }
        }
        return reply
    }

    /// The registry's shard on an account, as bytes; empty when it holds none.
    function readShard(accountIdHex) {
        var account = call("lez_core", "get_account_public", [accountIdHex])
        if (!account || !account.shards)
            throw new Error("the LEZ wallet did not return the account; is the wallet open?")
        var program = accountHex(programId)
        for (var i = 0; i < account.shards.length; i++) {
            if (String(account.shards[i].program).toLowerCase() === program)
                return hexToBytes(account.shards[i].data)
        }
        return []
    }

    function resolve(name) {
        var bytes = readShard(recordAddressHex(name))
        return bytes.length === 0 ? null : decodeRecord(bytes)
    }

    function report(message, isError) {
        status = message
        failed = !!isError
    }

    function requireProgram() {
        if (accountHex(programId) === "") {
            report("Enter the registry's program ID first.", true)
            return false
        }
        return true
    }

    // ── Actions ─────────────────────────────────────────────────────────

    function lookUp() {
        record = null
        page = null
        var name = normalizeName(nameField.text)
        if (!isValidName(name)) {
            report("A name is 1-32 characters of a-z, 0-9 or '-', such as jef.lez.", true)
            return
        }
        if (!requireProgram())
            return
        try {
            var found = resolve(name)
            if (found === null) {
                report(name + nameSuffix + " is not registered.", false)
                return
            }
            record = found
            var pageBytes = readShard(pageAddressHex(name))
            page = pageBytes.length === 0 ? null : decodePage(pageBytes)
            report("", false)
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
                var balance = call("lez_core", "get_balance", [id, true])
                out.push({
                    hex: id,
                    label: base58Encode(hexToBytes(id)).substr(0, 10) + "…  " + String(balance)
                })
            }
            accounts = out
            if (out.length === 0)
                report("The LEZ wallet has no public accounts to pay from.", true)
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
        var bytes = []
        for (var i = 0; i < 16; i++) {
            bytes.push(value % 256)
            value = Math.floor(value / 256)
        }
        return bytesToHex(bytes)
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
            var reply
            if (usePrivateKeys) {
                var keys = JSON.stringify({ nullifier_public_key: record.npk, viewing_public_key: record.vpk })
                report("Generating the proof; this can take a few minutes.", false)
                reply = call("lez_core", "transfer_shielded", [from, keys, amount])
            } else {
                reply = call("lez_core", "transfer_public", [from, accountHex(record.account), amount])
            }
            if (reply && reply.success)
                report("Sent to " + record.name + nameSuffix + ". Transaction " + reply.tx_hash, false)
            else
                report("The wallet refused the transfer: " + (reply && reply.error ? reply.error : "no reason given"), true)
        } catch (error) {
            report(error.message, true)
        }
    }

    function textEntry(key) {
        if (record === null)
            return ""
        for (var i = 0; i < record.texts.length; i++) {
            if (record.texts[i].key === key)
                return record.texts[i].value
        }
        return ""
    }

    readonly property string chatAddress: record !== null && /^[0-9a-f]{64}$/.test(textEntry("chat")) ? textEntry("chat") : ""

    function message() {
        try {
            var reply = call("chat_module", "create_conversation", [chatAddress])
            if (reply && reply.success === false) {
                report("Chat could not start the conversation: " + reply.error, true)
                return
            }
            report("Conversation started with " + record.name + nameSuffix + ". Opening Chat.", false)
            if (logos.request)
                logos.request("basecamp.apps.launch", { app: "chat_ui" }, function() {})
        } catch (error) {
            report(error.message, true)
        }
    }

    function listRecent() {
        if (!requireProgram())
            return
        try {
            var bytes = readShard(directoryAddressHex())
            var next = bytes.length === 0 ? null : decodeDirectory(bytes).latest
            var names = []
            while (next !== null && names.length < 20) {
                var found = resolve(next)
                if (found === null)
                    break
                names.push(found.name)
                next = found.previous
            }
            recentNames = names
            report(names.length === 0 ? "No names are registered yet." : "", false)
        } catch (error) {
            report(error.message, true)
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

            Flow {
                visible: root.recentNames.length > 0
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: root.recentNames
                    Button {
                        text: modelData + root.nameSuffix
                        flat: true
                        onClicked: {
                            nameField.text = modelData + root.nameSuffix
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
                        text: root.record !== null ? root.record.name + root.nameSuffix : ""
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
                            text: root.page !== null ? root.page.contentType + ", " + root.page.body.length + " bytes" : "none"
                            color: "#d5d9e2"; font.pixelSize: 12
                        }
                    }

                    Repeater {
                        model: root.record !== null ? root.record.texts : []
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Text { text: modelData.key; color: "#8b93a3"; font.pixelSize: 12 }
                            Text {
                                Layout.fillWidth: true
                                text: modelData.value
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
                            displayText: root.accounts.length === 0 ? "Load accounts" : currentText
                            onPressedChanged: if (pressed && root.accounts.length === 0) root.loadAccounts()
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

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text { text: "Registry"; color: "#8b93a3"; font.pixelSize: 12 }
                TextField {
                    Layout.fillWidth: true
                    text: root.programId
                    placeholderText: "program ID of the names registry"
                    font.pixelSize: 12
                    onEditingFinished: root.programId = text.trim()
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
