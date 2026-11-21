// A one-line text field. `glyph: "search"` draws a magnifier at its right end that accepts it.
import QtQuick

Rectangle {
    id: field
    property Look look: Look {}
    property alias text: input.text
    property string hint: ""
    property bool code: false
    property string glyph: ""
    property int size: 17
    signal accepted()
    implicitHeight: size + 21
    radius: 6
    color: look.slate
    border.width: 1
    border.color: input.activeFocus ? look.brass : look.line
    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: field.glyph !== "" ? 36 : 12
        verticalAlignment: TextInput.AlignVCenter
        color: look.ink
        selectionColor: look.brass
        selectedTextColor: look.slate
        font.family: field.code ? look.mono : look.sans
        font.pixelSize: field.code ? field.size - 1 : field.size
        clip: true
        selectByMouse: true
        onAccepted: field.accepted()
        onTextChanged: if (!activeFocus) cursorPosition = 0
    }
    Text {
        textFormat: Text.PlainText
        anchors.fill: input
        verticalAlignment: TextInput.AlignVCenter
        visible: input.text === ""
        text: field.hint
        color: look.faint
        font.family: look.sans
        font.pixelSize: field.size
        elide: Text.ElideRight
    }
    Canvas {
        id: glyphCanvas
        visible: field.glyph === "search"
        anchors { right: parent.right; rightMargin: 11; verticalCenter: parent.verticalCenter }
        width: 16; height: 16
        property color tint: glyphArea.containsMouse ? look.brass : look.dim
        onTintChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            ctx.strokeStyle = tint
            ctx.lineCap = "round"
            ctx.lineWidth = 1.5
            ctx.beginPath()
            ctx.arc(6.75, 6.75, 4.5, 0, Math.PI * 2)
            ctx.moveTo(10.2, 10.2)
            ctx.lineTo(14.2, 14.2)
            ctx.stroke()
        }
        MouseArea { id: glyphArea; anchors.fill: parent; anchors.margins: -8; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: field.accepted() }
    }
}
