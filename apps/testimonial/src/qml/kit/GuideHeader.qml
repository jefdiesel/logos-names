// The top of a guide page: Back when nested, a quiet section word, the title, one plain
// sentence, an optional context line ("Part of Bridge"), who published it (italics when it is their own unconfirmed words, with `publisherNote`
// saying so), an optional warning that matters to a reader, then what you can do.
// `actions`: [{key, label, strong}] → actionPressed(key). Plain text throughout.
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: guide
    property Look look: Look {}
    property string backLabel: ""
    property string section: ""
    property string title: ""
    property string sentence: ""
    property string publisher: ""
    property bool proven: true
    property string publisherNote: ""
    property string warning: ""
    // A quiet line under the title: `contextLabel` then `contextLink` as a link ("Part of" Bridge).
    property string contextLabel: ""
    property string contextLink: ""
    property var actions: []
    property string workingKey: ""
    signal back()
    signal actionPressed(string key)
    signal contextPressed()
    Layout.fillWidth: true
    spacing: 8
    BackButton { visible: guide.backLabel !== ""; look: guide.look; label: guide.backLabel; onPressed: guide.back(); Layout.bottomMargin: 8 }
    Text {
        textFormat: Text.PlainText
        visible: guide.section !== ""
        text: guide.section.toUpperCase()
        color: look.dim
        font.family: look.sans
        font.pixelSize: look.px(13)
        font.weight: Font.Bold
        font.letterSpacing: 1.4
    }
    Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        text: guide.title
        color: look.ink
        font.family: look.sans
        font.pixelSize: look.px(28)
        font.weight: Font.DemiBold
        wrapMode: Text.WordWrap
    }
    Flow {
        visible: guide.contextLabel !== "" || guide.contextLink !== ""
        Layout.fillWidth: true
        spacing: 6
        Text { textFormat: Text.PlainText; text: guide.contextLabel; color: look.dim; font.family: look.sans; font.pixelSize: look.px(15) }
        Link { look: guide.look; visible: guide.contextLink !== ""; text: guide.contextLink; font.pixelSize: look.px(15); onPressed: guide.contextPressed() }
    }
    Text {
        textFormat: Text.PlainText
        visible: guide.sentence !== ""
        Layout.fillWidth: true
        text: guide.sentence
        color: look.ink
        font.family: look.sans
        font.weight: Font.Medium
        font.pixelSize: look.px(18)
        lineHeight: 1.25
        wrapMode: Text.Wrap
    }
    Flow {
        visible: guide.publisher !== ""
        Layout.fillWidth: true
        spacing: 0
        Text { textFormat: Text.PlainText; text: "by "; color: look.dim; font.family: look.sans; font.pixelSize: look.px(15) }
        Text { textFormat: Text.PlainText; text: guide.publisher; color: look.ink; font.family: look.sans; font.pixelSize: look.px(15); font.weight: Font.DemiBold; font.italic: !guide.proven }
        Text { textFormat: Text.PlainText; visible: guide.publisherNote !== ""; text: "  ·  " + guide.publisherNote; color: look.dim; font.family: look.sans; font.pixelSize: look.px(15) }
    }
    Text {
        textFormat: Text.PlainText
        visible: guide.warning !== ""
        Layout.fillWidth: true
        Layout.topMargin: 4
        text: guide.warning
        color: look.alarm
        font.family: look.sans
        font.pixelSize: look.px(16)
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
    }
    Flow {
        visible: guide.actions.length > 0
        Layout.fillWidth: true
        Layout.topMargin: 8
        spacing: 8
        Repeater {
            model: guide.actions
            Action {
                required property var modelData
                look: guide.look
                label: modelData.label
                busyLabel: modelData.busyLabel || ""
                strong: !!modelData.strong
                working: guide.workingKey === modelData.key
                onPressed: guide.actionPressed(modelData.key)
            }
        }
    }
}
