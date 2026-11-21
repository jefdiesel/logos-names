// A heading in an index ("Bridges", "Tokens", or a letter): a small spaced-caps label; the space
// above it is the caller's.
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: heading
    property Look look: Look {}
    property string text: ""
    property string count: ""              // optional, quiet, after the heading
    spacing: 6
    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        Text {
            textFormat: Text.PlainText
            text: heading.text.toUpperCase()
            color: look.dim
            font.family: look.sans
            font.pixelSize: look.px(13)
            font.weight: Font.Bold
            font.letterSpacing: 1.4
        }
        Text {
            textFormat: Text.PlainText
            visible: heading.count !== ""
            text: heading.count
            color: look.dim
            font.family: look.sans
            font.pixelSize: look.px(13)
        }
        Item { Layout.fillWidth: true }
    }
}
