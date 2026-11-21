// An ordinary tick box with plain words. Never red, never coloured before it is ticked.
import QtQuick
import QtQuick.Layouts

RowLayout {
    id: tick
    property Look look: Look {}
    property bool checked: false
    property string text: ""
    signal toggled()
    Layout.fillWidth: true
    spacing: 10
    Rectangle {
        Layout.preferredWidth: look.px(20)
        Layout.preferredHeight: look.px(20)
        Layout.alignment: Qt.AlignTop
        Layout.topMargin: look.px(1)
        radius: 4
        color: tick.checked ? look.brass : "transparent"
        border.width: 2
        border.color: activeFocus ? look.ink : tick.checked ? look.brass : look.dim
        activeFocusOnTab: true
        Accessible.role: Accessible.CheckBox
        Accessible.name: tick.text
        Accessible.checked: tick.checked
        Keys.onSpacePressed: tick.toggled()
        Text { textFormat: Text.PlainText; anchors.centerIn: parent; visible: tick.checked; text: "✓"; color: look.slate; font.family: look.sans; font.pixelSize: look.px(14); font.weight: Font.Bold }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tick.toggled() }
    }
    Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        text: tick.text
        color: look.ink
        font.family: look.sans
        font.weight: Font.Medium
        font.pixelSize: look.px(16)
        lineHeight: 1.25
        wrapMode: Text.Wrap
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tick.toggled() }
    }
}
