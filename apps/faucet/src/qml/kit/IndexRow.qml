// One entry in an index: a title, and optionally one plain sentence of what it is and who
// published it (leave them empty for an index of names only).
// `proven: false` sets the publisher in italics (their own words, not confirmed): quiet, no
// badge; the index explains italics once, under the list. `note` is a quiet line ("v7, the
// newest of 3"). `secondary` is an older or lesser member of a family, drawn under its lead,
// indented and smaller.
import QtQuick
import QtQuick.Layouts

Rectangle {
    id: row
    property Look look: Look {}
    property string title: ""
    property string sentence: ""
    property string publisher: ""
    property bool proven: true
    property string note: ""
    property bool secondary: false
    property bool selected: false
    signal pressed()
    // A title alone (an index of names) is one tight line; with a sentence, a roomier card.
    readonly property bool titleOnly: sentence === "" && publisher === "" && note === ""
    implicitHeight: column.implicitHeight + (titleOnly ? (secondary ? 8 : 12) : (secondary ? 12 : 20))
    radius: 6
    color: selected ? look.raised : rowArea.containsMouse ? Qt.rgba(look.raised.r, look.raised.g, look.raised.b, 0.6) : "transparent"
    border.width: activeFocus ? 1 : 0
    border.color: look.brass
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: title + ". " + sentence
    Keys.onReturnPressed: row.pressed()
    Keys.onSpacePressed: row.pressed()
    // The chosen row: a bar in the accent at its left edge.
    Rectangle { visible: row.selected; width: 3; radius: 1.5; color: look.brass; anchors { left: parent.left; top: parent.top; bottom: parent.bottom; topMargin: 6; bottomMargin: 6 } }
    // A family's later members hang from a thin line, like an index's sub-entries.
    Rectangle { visible: row.secondary; width: 1; color: look.line; anchors { left: parent.left; leftMargin: 14; top: parent.top; bottom: parent.bottom } }
    ColumnLayout {
        id: column
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: row.secondary ? 28 : 12; rightMargin: 12 }
        spacing: row.secondary ? 2 : 4
        Text {
            textFormat: Text.PlainText
            Layout.fillWidth: true
            text: row.title
            color: row.secondary ? look.dim : look.ink
            font.family: look.sans
            font.pixelSize: look.px(row.secondary ? 15 : 17)
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            textFormat: Text.PlainText
            visible: row.sentence !== ""
            Layout.fillWidth: true
            text: row.sentence
            color: look.dim
            font.family: look.sans
            font.pixelSize: look.px(row.secondary ? 13 : 15)
            lineHeight: 1.15
            wrapMode: Text.Wrap
            maximumLineCount: row.secondary ? 1 : 2
            elide: Text.ElideRight
        }
        Text {
            textFormat: Text.PlainText
            visible: !row.secondary && (row.publisher !== "" || row.note !== "")
            Layout.fillWidth: true
            text: (row.publisher !== "" ? "by " + row.publisher : "") + (row.note !== "" ? (row.publisher !== "" ? "  ·  " : "") + row.note : "")
            color: look.dim
            font.family: look.sans
            font.pixelSize: look.px(13)
            font.italic: !row.proven
            elide: Text.ElideRight
        }
    }
    MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: row.pressed() }
}
