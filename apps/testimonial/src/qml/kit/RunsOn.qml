// Which network a page reads or writes, in one line: "Reads <network> · <detail>".
// Plain text only (the network's name is anyone's words).
import QtQuick
import QtQuick.Layouts

Flow {
    id: runsOn
    property Look look: Look {}
    property string prefix: "Runs on"
    property string network: ""
    property string detail: ""
    Layout.fillWidth: true
    spacing: 0
    Text { textFormat: Text.PlainText; text: runsOn.prefix + " "; color: look.dim; font.family: look.sans; font.pixelSize: look.px(14) }
    Text { textFormat: Text.PlainText; text: runsOn.network; color: look.dim; font.family: look.sans; font.pixelSize: look.px(14); font.weight: Font.Bold }
    Text { textFormat: Text.PlainText; visible: runsOn.detail !== ""; text: " · " + runsOn.detail; color: look.dim; font.family: look.sans; font.pixelSize: look.px(14) }
}
