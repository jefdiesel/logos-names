// One fact in a details block: a label and its value (plain text; a monospace value such as an
// id wraps anywhere, so it never pushes the page wider).
import QtQuick
import QtQuick.Layouts

RowLayout {
    id: fact
    property Look look: Look {}
    property string key: ""
    property string value: ""
    property bool monospace: false
    property color tone: look.ink
    Layout.fillWidth: true
    spacing: 16
    Text { textFormat: Text.PlainText; text: fact.key; color: look.dim; font.family: look.sans; font.pixelSize: look.px(14); Layout.preferredWidth: look.px(132); Layout.alignment: Qt.AlignTop; wrapMode: Text.Wrap }
    Text { textFormat: Text.PlainText; text: fact.value; color: fact.tone; font.family: fact.monospace ? look.mono : look.sans; font.pixelSize: look.px(fact.monospace ? 13 : 14); wrapMode: fact.monospace ? Text.WrapAnywhere : Text.Wrap; Layout.fillWidth: true }
}
