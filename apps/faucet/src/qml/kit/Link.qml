// A small thing to do beside text: semibold ink; underlined, in the accent, on hover or focus.
import QtQuick

Text {
    textFormat: Text.PlainText
    id: link
    property Look look: Look {}
    signal pressed()
    color: linkArea.containsMouse || activeFocus ? look.brass : look.ink
    font.family: look.sans
    font.pixelSize: look.px(15)
    font.weight: Font.DemiBold
    font.underline: linkArea.containsMouse || activeFocus
    activeFocusOnTab: true
    Accessible.role: Accessible.Link
    Accessible.name: text
    Keys.onReturnPressed: link.pressed()
    Keys.onSpacePressed: link.pressed()
    MouseArea { id: linkArea; anchors.fill: parent; anchors.margins: -6; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: link.pressed() }
}
