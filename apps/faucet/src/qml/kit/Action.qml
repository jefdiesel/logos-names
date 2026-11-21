// A button. `strong`: the one primary on a screen (accent fill); otherwise a 1 px outline.
// `grave` marks a step that can't be undone: it looks the same (no red buttons) and must
// always go through a confirm that says so. `changes`: it changes something, so it rests
// while the app is `busy`; `working` shows `busyLabel` on this button while its own step runs.
// Labels are a verb and an object ("Update entry", "View source"), never OK or Submit.
import QtQuick

Rectangle {
    id: action
    property Look look: Look {}
    property string label: ""
    property string busyLabel: ""
    property bool strong: false
    property bool grave: false
    property bool changes: false
    property bool busy: false
    property bool working: false
    property bool enabledNow: true
    readonly property bool live: enabledNow && !working && !(changes && busy)
    signal pressed()
    implicitWidth: actionText.implicitWidth + 28
    implicitHeight: 38
    radius: 6
    color: !live ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(look.brass, 1.08) : look.brass) : (actionArea.containsMouse ? look.raised : "transparent")
    border.width: strong && live && !activeFocus ? 0 : 1
    border.color: activeFocus ? look.brass : look.line
    opacity: live ? 1 : 0.55
    activeFocusOnTab: live
    Accessible.role: Accessible.Button
    Accessible.name: actionText.text
    Keys.onReturnPressed: if (live) action.pressed()
    Keys.onSpacePressed: if (live) action.pressed()
    Text {
        textFormat: Text.PlainText
        id: actionText
        anchors.centerIn: parent
        text: action.working && action.busyLabel !== "" ? action.busyLabel : action.label
        color: action.strong && action.live ? look.slate : look.ink
        font.family: look.sans
        font.pixelSize: look.px(18)
        font.weight: Font.DemiBold
    }
    MouseArea {
        id: actionArea
        anchors.fill: parent
        hoverEnabled: true
        enabled: action.live
        cursorShape: Qt.PointingHandCursor
        onClicked: action.pressed()
    }
}
