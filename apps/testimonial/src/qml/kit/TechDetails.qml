// A collapsed block for what most readers don't need: ids, hashes, revisions, signing tools.
// Closed it shows its title and a one-line `summary`; open it shows what the app puts inside
// (its default property), spaced 14 apart.
import QtQuick
import QtQuick.Layouts

Rectangle {
    id: tech
    property Look look: Look {}
    property string title: "Technical details"
    property string summary: ""
    property bool open: false
    default property alias content: inner.data
    Layout.fillWidth: true
    implicitHeight: column.implicitHeight + 32
    radius: 8
    color: look.card
    border.width: 1
    border.color: look.line
    ColumnLayout {
        id: column
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
        spacing: 14
        Item {
            Layout.fillWidth: true
            implicitHeight: head.implicitHeight
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: (tech.open ? "Hide " : "Show ") + tech.title
            Keys.onReturnPressed: tech.open = !tech.open
            Keys.onSpacePressed: tech.open = !tech.open
            RowLayout {
                id: head
                width: parent.width
                spacing: 10
                Text { textFormat: Text.PlainText; text: tech.open ? "▾" : "▸"; color: look.dim; font.family: look.sans; font.pixelSize: look.px(16) }
                Text { textFormat: Text.PlainText; text: tech.title; color: look.ink; font.family: look.sans; font.pixelSize: look.px(17); font.weight: Font.DemiBold }
                Text {
                    textFormat: Text.PlainText
                    Layout.fillWidth: true
                    text: tech.open ? "" : tech.summary
                    color: look.dim
                    font.family: look.sans
                    font.pixelSize: look.px(14)
                    elide: Text.ElideRight
                }
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tech.open = !tech.open }
        }
        ColumnLayout {
            id: inner
            visible: tech.open
            Layout.fillWidth: true
            spacing: 14
        }
    }
}
