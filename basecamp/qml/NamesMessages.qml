// Messages: chats between LNS names that work with the Names phone app and the Names web app (the names message
// format, version 1). Messages go over Logos Messaging on logos.test, through Basecamp's own messaging module; they
// are not on a chain. Name lookups are LNS reads on the LEZ testnet. The module does the work (its msg_* methods,
// src/messages/ in basecamp-module); this screen only shows it and asks. Logos Chat (Basecamp's Chat app) is the
// other, separate tab.
import QtQuick
import QtQuick.Layouts

Item {
    id: nm
    /// The wallet's Main.qml root: its colours, sizes, msgCall and cliChange.
    property var app
    property var status: null
    property var chats: []
    property var requests: []
    property var blockedList: []
    property var msgs: []
    property var safety: null
    property string view: "list"          // list, chat, requests, blocked, safety
    property string openAddress: ""
    property string openTitle: ""
    property string note: ""
    property bool noteBad: false
    property int seenChanges: -1
    readonly property bool on: status !== null && status.on === true

    function short(a) { return a ? a.substr(0, 8) + "…" + a.substr(a.length - 6) : "" }
    function say(text, bad) { note = text; noteBad = bad === true }
    function call(method, args, done) {
        app.msgCall(method, args, function(r) {
            if (!r || r.success === undefined) { say("The names module is missing or out of date.", true); return }
            if (!r.success) { say(r.error, true); return }
            if (done) done(r.value)
        })
    }
    function refresh() {
        call("msg_status", [], function(v) {
            status = v
            if (!v.on) return
            if (v.changes !== seenChanges || seenChanges < 0) {
                seenChanges = v.changes
                call("msg_list_chats", [], function(x) { chats = x })
                call("msg_requests", [], function(x) { requests = x })
                if (view === "chat" && openAddress !== "") loadChat()
            }
        })
    }
    function loadChat() {
        call("msg_messages", [openAddress], function(x) { msgs = x })
        call("msg_mark_read", [openAddress], null)
    }
    function openChat(address, title) {
        openAddress = address; openTitle = title; view = "chat"; msgs = []; say("", false); loadChat()
    }
    function turnOn() {
        say("Turning Messages on…", false)
        call("msg_turn_on", [], function(v) {
            refresh()
            if (v.published || !v.name) {
                say(v.name ? "Messages is on: " + v.name + " can be messaged here." : "Messages is on. Choose a main name in Settings so people can message it.", false)
                return
            }
            // the address goes in the name's msg entry: a change to the name on the LEZ testnet (a fee)
            app.cliChange("set_text", [v.name, "msg", v.address], v.name, function() {
                var found = app.ask("resolve", [v.name])
                return found !== null && found.texts && found.texts.msg === v.address
            }, "People can now message " + v.name + " from the phone app and the web app.")
        })
    }
    function send(to, text, done) {
        if (text.trim() === "") return
        call("msg_send", [to, text], function(v) { refresh(); if (done) done(v) })
    }

    Timer { interval: 3000; repeat: true; running: nm.visible; triggeredOnStart: true; onTriggered: nm.refresh() }

    // ── small parts, in the wallet's look ──
    component Btn: Rectangle {
        id: b
        property string label: ""
        property bool strong: false
        property bool grave: false
        signal pressed()
        implicitWidth: bt.implicitWidth + 28
        implicitHeight: 38
        radius: 6
        color: strong ? (ba.containsMouse ? Qt.lighter(app.brass, 1.08) : app.brass) : (ba.containsMouse ? app.raised : "transparent")
        border.width: strong ? 0 : 1
        border.color: grave ? app.alarm : app.line
        Text { id: bt; anchors.centerIn: parent; text: b.label; color: b.strong ? app.slate : b.grave ? app.alarm : app.ink; font.family: app.sans; font.pixelSize: app.px(16); font.weight: Font.DemiBold }
        MouseArea { id: ba; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: b.pressed() }
    }
    component Line: Text { Layout.fillWidth: true; color: app.dim; font.family: app.sans; font.pixelSize: app.px(15); wrapMode: Text.Wrap }
    component Input: Rectangle {
        property alias text: ti.text
        property string hint: ""
        signal accepted()
        Layout.fillWidth: true
        implicitHeight: 40
        radius: 6
        color: app.slate
        border.width: 1
        border.color: ti.activeFocus ? app.brass : app.line
        TextInput { id: ti; anchors.fill: parent; anchors.margins: 10; verticalAlignment: TextInput.AlignVCenter; color: app.ink; font.family: app.sans; font.pixelSize: app.px(16); clip: true; selectByMouse: true; onAccepted: parent.accepted() }
        Text { anchors.fill: ti; verticalAlignment: Text.AlignVCenter; visible: ti.text === ""; text: parent.hint; color: app.faint; font.family: app.sans; font.pixelSize: app.px(16); elide: Text.ElideRight }
    }

    Flickable {
        anchors.fill: parent
        contentHeight: col.implicitHeight + 64
        clip: true
        ColumnLayout {
            id: col
            x: 32; y: 32
            width: parent.width - 64
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                Text { text: "Messages"; color: app.ink; font.family: app.sans; font.pixelSize: app.px(28); font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                Text { text: nm.status === null ? "" : nm.on ? "logos.test · " + (nm.status.connection || "") : "off"; color: app.faint; font.family: app.sans; font.pixelSize: app.px(14) }
            }
            Line { text: "Chats with LNS names that work with the Names phone app and the web app, end-to-end encrypted. They go over Logos Messaging on logos.test, not on a chain. Logos Chat is the other tab." }

            // off
            Rectangle {
                visible: nm.status !== null && !nm.on
                Layout.fillWidth: true
                implicitHeight: offRow.implicitHeight + 24
                radius: 8; color: app.card; border.width: 1; border.color: app.line
                RowLayout {
                    id: offRow
                    x: 14; y: 12; width: parent.width - 28
                    Line { text: "Messages is off on this computer. Turning it on makes this computer a messaging address and puts it in " + (app.mainName !== "" ? app.mainName : "your main name") + "'s profile (a change to the name on the public LEZ testnet, with its fee), so people can message the name from their phone or the web." }
                    Btn { label: "Turn on Messages"; strong: true; onPressed: nm.turnOn() }
                }
            }
            // saved chats that can't be read
            Rectangle {
                visible: !!(nm.on && nm.status.unreadable)
                Layout.fillWidth: true
                implicitHeight: badRow.implicitHeight + 24
                radius: 8; color: app.noticeFill
                ColumnLayout {
                    id: badRow
                    x: 14; y: 12; width: parent.width - 28
                    Text { Layout.fillWidth: true; wrapMode: Text.Wrap; color: app.brass; font.family: app.sans; font.pixelSize: app.px(15); text: "The saved chats can't be read (" + (nm.on && nm.status.unreadable ? nm.status.unreadable.why : "") + "). Nothing is saved or sent until you choose. The file is kept either way." }
                    RowLayout {
                        Btn { visible: !!(nm.on && nm.status.unreadable && nm.status.unreadable.last_copy); label: "Use the last saved copy"; onPressed: nm.call("msg_recover", [true], function() { nm.seenChanges = -1; nm.refresh() }) }
                        Btn { label: "Start fresh"; grave: true; onPressed: nm.call("msg_recover", [false], function() { nm.seenChanges = -1; nm.refresh() }) }
                    }
                }
            }
            Text { visible: nm.note !== ""; Layout.fillWidth: true; wrapMode: Text.Wrap; text: nm.note; color: nm.noteBad ? app.alarm : app.dim; font.family: app.sans; font.pixelSize: app.px(14) }

            // ── the list ──
            ColumnLayout {
                visible: nm.on && nm.view === "list"
                Layout.fillWidth: true
                spacing: 10
                RowLayout {
                    Layout.fillWidth: true
                    Input { id: toField; hint: "An LNS name, like alice, or a messaging address" }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Input { id: firstText; hint: "Message"; onAccepted: newSend.pressed() }
                    Btn { id: newSend; label: "Send"; strong: true; onPressed: nm.send(toField.text.trim(), firstText.text, function(v) { firstText.text = ""; nm.openChat(v.to, toField.text.trim()) }) }
                }
                Line { visible: nm.on && nm.status.can_send === false; color: app.brass; text: nm.on && nm.status.send_note ? nm.status.send_note : "" }
                Btn { visible: nm.requests.length > 0; label: nm.requests.length + (nm.requests.length === 1 ? " request" : " requests"); onPressed: nm.view = "requests" }
                Repeater {
                    model: nm.chats
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: chatRow.implicitHeight + 20
                        radius: 8; color: rowArea.containsMouse ? app.hover : app.card; border.width: 1; border.color: app.line
                        RowLayout {
                            id: chatRow
                            x: 14; y: 10; width: parent.width - 28
                            ColumnLayout {
                                Layout.fillWidth: true
                                Text { text: (modelData.name || nm.short(modelData.address)) + (modelData.checked ? "  ✓ checked" : ""); color: app.ink; font.family: app.sans; font.pixelSize: app.px(17); font.weight: Font.DemiBold }
                                Text { Layout.fillWidth: true; text: modelData.last || ""; color: app.dim; font.family: app.sans; font.pixelSize: app.px(14); elide: Text.ElideRight }
                            }
                            Text { visible: modelData.unread > 0; text: modelData.unread; color: app.brass; font.family: app.sans; font.pixelSize: app.px(15); font.weight: Font.DemiBold }
                        }
                        MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: nm.openChat(modelData.address, modelData.name || nm.short(modelData.address)) }
                    }
                }
                Line { visible: nm.on && nm.chats.length === 0; text: "No chats yet. Write to a name above; messages to " + (app.mainName !== "" ? app.mainName : "your name") + " show up here." }
                Btn { label: "Blocked addresses"; onPressed: nm.call("msg_blocked", [], function(v) { nm.blockedList = v; nm.view = "blocked" }) }
                Line { visible: nm.on; font.pixelSize: app.px(13); text: nm.on ? "This computer's messaging address: " + nm.status.address + (nm.status.dropped > 0 ? " · " + nm.status.dropped + " messages from strangers deleted unread (limits or blocks)" : "") : "" }
            }

            // ── one chat ──
            ColumnLayout {
                visible: nm.on && nm.view === "chat"
                Layout.fillWidth: true
                spacing: 10
                RowLayout {
                    Btn { label: "Back"; onPressed: { nm.view = "list"; nm.refresh() } }
                    Text { Layout.fillWidth: true; text: nm.openTitle; color: app.ink; font.family: app.sans; font.pixelSize: app.px(20); font.weight: Font.DemiBold; elide: Text.ElideRight }
                    Btn { label: "Safety code"; onPressed: nm.call("msg_safety", [nm.openAddress], function(v) { nm.safety = v; nm.view = "safety" }) }
                    Btn { label: "Block"; grave: true; onPressed: nm.call("msg_block", [nm.openAddress], function() { nm.view = "list"; nm.refresh() }) }
                }
                Repeater {
                    model: nm.msgs
                    delegate: Rectangle {
                        required property var modelData
                        readonly property bool mine: modelData.dir === "out"
                        Layout.alignment: mine ? Qt.AlignRight : Qt.AlignLeft
                        Layout.maximumWidth: col.width * 0.75
                        implicitWidth: Math.min(bubble.implicitWidth + 24, col.width * 0.75)
                        implicitHeight: bubble.implicitHeight + 18
                        radius: 10
                        color: mine ? app.ownBubble : app.card
                        border.width: mine ? 0 : 1; border.color: app.line
                        ColumnLayout {
                            id: bubble
                            x: 12; y: 9; width: parent.width - 24
                            Text { Layout.fillWidth: true; text: modelData.text; color: app.ink; font.family: app.sans; font.pixelSize: app.px(16); wrapMode: Text.Wrap }
                            Text {
                                text: new Date(modelData.ts).toLocaleString() + (mine ? " · " + ({ queued: "waiting to send", sending: "sending", propagated: "sent", sent: "sent", failed: "not sent" }[modelData.state] || "") : "")
                                color: mine && modelData.state === "failed" ? app.alarm : app.faint; font.family: app.sans; font.pixelSize: app.px(12)
                            }
                        }
                    }
                }
                // sending honestly: when the node refused for RLN, say so in place of the composer's silence
                Line { visible: nm.on && nm.status.can_send === false; color: app.brass; text: nm.on && nm.status.send_note ? nm.status.send_note + "." : "" }
                RowLayout {
                    Layout.fillWidth: true
                    Input { id: replyText; hint: "Message"; onAccepted: replySend.pressed() }
                    Btn { id: replySend; label: "Send"; strong: true; onPressed: nm.send(nm.openAddress, replyText.text, function() { replyText.text = ""; nm.loadChat() }) }
                }
                Line { font.pixelSize: app.px(13); text: "Over Logos Messaging on logos.test, end-to-end encrypted. Not on a chain." }
            }

            // ── requests ──
            ColumnLayout {
                visible: nm.on && nm.view === "requests"
                Layout.fillWidth: true
                spacing: 10
                RowLayout { Btn { label: "Back"; onPressed: nm.view = "list" } Text { text: "Requests"; color: app.ink; font.family: app.sans; font.pixelSize: app.px(20); font.weight: Font.DemiBold } }
                Line { text: "Messages from addresses that aren't in your chats. A name here is only what they say; it's checked against the name's profile when you accept." }
                Repeater {
                    model: nm.requests
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: reqCol.implicitHeight + 20
                        radius: 8; color: app.card; border.width: 1; border.color: app.line
                        ColumnLayout {
                            id: reqCol
                            x: 14; y: 10; width: parent.width - 28
                            Text { text: (modelData.claimed && modelData.claimed.indexOf("msg-") !== 0 ? "Says it's " + modelData.claimed : "No name") + " · " + nm.short(modelData.address); color: app.ink; font.family: app.sans; font.pixelSize: app.px(16); font.weight: Font.DemiBold }
                            Text { Layout.fillWidth: true; text: modelData.last || ""; color: app.dim; font.family: app.sans; font.pixelSize: app.px(14); wrapMode: Text.Wrap }
                            RowLayout {
                                Btn { label: "Accept"; strong: true; onPressed: nm.call("msg_accept", [modelData.address], function() { nm.seenChanges = -1; nm.refresh() }) }
                                Btn { label: "Block"; grave: true; onPressed: nm.call("msg_block", [modelData.address], function() { nm.seenChanges = -1; nm.refresh() }) }
                            }
                        }
                    }
                }
            }

            // ── safety code ──
            ColumnLayout {
                visible: nm.on && nm.view === "safety" && nm.safety !== null
                Layout.fillWidth: true
                spacing: 10
                RowLayout { Btn { label: "Back"; onPressed: nm.view = "chat" } Text { text: "Safety code with " + nm.openTitle; color: app.ink; font.family: app.sans; font.pixelSize: app.px(20); font.weight: Font.DemiBold } }
                Line { text: "Compare it with the code on their phone (Messages, their profile, Safety code). The same 60 digits on both sides means nobody is in between." }
                Text { Layout.fillWidth: true; wrapMode: Text.Wrap; text: nm.safety ? nm.safety.groups.join("  ") : ""; color: app.ink; font.family: app.mono; font.pixelSize: app.px(20) }
                Text { text: nm.safety && nm.safety.checked ? "Checked in person" : "Not checked"; color: nm.safety && nm.safety.checked ? app.good : app.dim; font.family: app.sans; font.pixelSize: app.px(15) }
                RowLayout {
                    Btn { visible: !!(nm.safety && !nm.safety.checked); label: "The codes match"; strong: true; onPressed: nm.call("msg_mark_checked", [nm.openAddress, true], function() { nm.call("msg_safety", [nm.openAddress], function(v) { nm.safety = v }) }) }
                    Btn { visible: !!(nm.safety && nm.safety.checked); label: "Mark as not checked"; onPressed: nm.call("msg_mark_checked", [nm.openAddress, false], function() { nm.call("msg_safety", [nm.openAddress], function(v) { nm.safety = v }) }) }
                }
            }

            // ── blocked ──
            ColumnLayout {
                visible: nm.on && nm.view === "blocked"
                Layout.fillWidth: true
                spacing: 10
                RowLayout { Btn { label: "Back"; onPressed: nm.view = "list" } Text { text: "Blocked"; color: app.ink; font.family: app.sans; font.pixelSize: app.px(20); font.weight: Font.DemiBold } }
                Line { visible: nm.blockedList.length === 0; text: "No one is blocked." }
                Repeater {
                    model: nm.blockedList
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        Text { Layout.fillWidth: true; text: modelData; color: app.ink; font.family: app.mono; font.pixelSize: app.px(13); elide: Text.ElideMiddle }
                        Btn { label: "Unblock"; onPressed: nm.call("msg_unblock", [modelData], function() { nm.call("msg_blocked", [], function(v) { nm.blockedList = v }) }) }
                    }
                }
            }
        }
    }
}
