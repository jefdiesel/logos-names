// The top of every page: Back when the page is nested, the title (28 px), one plain sentence,
// then the network line.
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: pageHeader
    property Look look: Look {}
    property string title: ""
    property string sentence: ""
    property string backLabel: ""          // "" = not nested, no Back
    property string runsOnPrefix: "Runs on"
    property string network: ""            // "" = no network line
    property string networkDetail: ""
    signal back()
    Layout.fillWidth: true
    spacing: 8
    BackButton { visible: pageHeader.backLabel !== ""; look: pageHeader.look; label: pageHeader.backLabel; onPressed: pageHeader.back(); Layout.bottomMargin: 8 }
    Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        text: pageHeader.title
        color: look.ink
        font.family: look.sans
        font.pixelSize: look.px(28)
        font.weight: Font.DemiBold
        wrapMode: Text.WordWrap
    }
    Text {
        textFormat: Text.PlainText
        visible: pageHeader.sentence !== ""
        Layout.fillWidth: true
        text: pageHeader.sentence
        color: look.dim
        font.family: look.sans
        font.weight: Font.Medium
        font.pixelSize: look.px(17)
        lineHeight: 1.25
        wrapMode: Text.Wrap
    }
    RunsOn { visible: pageHeader.network !== ""; look: pageHeader.look; prefix: pageHeader.runsOnPrefix; network: pageHeader.network; detail: pageHeader.networkDetail }
}
