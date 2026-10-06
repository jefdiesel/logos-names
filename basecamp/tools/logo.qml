// Draws the Names app icon and saves it as icon.png in the directory it is run
// from. Run with Qt's `qml` tool and QT_QPA_PLATFORM=offscreen, from src/icons.
// Logos packages require the icon to be a PNG of exactly 256 by 256.
//
// The mark is the app's seal reduced to its idea: rings with a gap in each, so
// none of them can be read all the way round, and the dot of ".LEZ" at the
// centre.

import QtQuick

Item {
    id: root
    width: 256
    height: 256

    Canvas {
        anchors.fill: parent
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            var size = width
            var centre = size / 2

            // The tile.
            var corner = size * 0.225
            ctx.fillStyle = "#0F1115"
            ctx.beginPath()
            ctx.moveTo(corner, 0)
            ctx.arcTo(size, 0, size, size, corner)
            ctx.arcTo(size, size, 0, size, corner)
            ctx.arcTo(0, size, 0, 0, corner)
            ctx.arcTo(0, 0, size, 0, corner)
            ctx.closePath()
            ctx.fill()

            // Two rings, each open at a different place.
            ctx.lineCap = "round"
            ctx.strokeStyle = "#F0C562"
            ctx.lineWidth = size * 0.07
            function ring(radius, openAt, opening) {
                ctx.beginPath()
                ctx.arc(centre, centre, radius, openAt + opening / 2, openAt + Math.PI * 2 - opening / 2)
                ctx.stroke()
            }
            ring(size * 0.325, -Math.PI * 0.25, 1.05)
            ring(size * 0.195, Math.PI * 0.75, 1.3)

            // The dot.
            ctx.fillStyle = "#FFFFFF"
            ctx.beginPath()
            ctx.arc(centre, centre, size * 0.07, 0, Math.PI * 2)
            ctx.fill()
        }
    }

    Timer {
        interval: 400
        running: true
        onTriggered: root.grabToImage(function(result) {
            result.saveToFile("icon.png")
            Qt.quit()
        })
    }
}
