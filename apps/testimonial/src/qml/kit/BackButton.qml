// The way back from a nested page: bordered, at the same place on every page.
import QtQuick

Rectangle {
    id: back
    property Look look: Look {}
    property string label: ""
    signal pressed()
    implicitWidth: backText.implicitWidth + look.px(28)
    implicitHeight: look.px(36)
    radius: 6
    color: backArea.pressed ? look.hover : backArea.containsMouse ? look.raised : "transparent"
    border.width: 1
    border.color: activeFocus ? look.brass : look.line
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: "Back to " + label
    Keys.onReturnPressed: back.pressed()
    Keys.onSpacePressed: back.pressed()
    Text {
        textFormat: Text.PlainText
        id: backText
        anchors.centerIn: parent
        text: "‹  " + back.label
        color: look.ink
        font.family: look.sans
        font.pixelSize: look.px(17)
        font.weight: Font.DemiBold
    }
    MouseArea {
        id: backArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: back.pressed()
    }
}
