// LNS Board for Basecamp: posts, replies, likes and follows, made as names.
//
// A front end on the `names` module's board_* calls. Every post, reply and
// like is made from the private account that holds one of your names, through
// your names CLI, so it is proved on this machine and the chain sees the
// name, never the account. Reading the board needs no wallet.
//
// Main.qml is generated: edit Main.qml.in, then run tools/shared.py, which
// copies the parts shared with the Names app (palette, buttons, avatars).

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    // ── State ───────────────────────────────────────────────────────────

    property var mine: []              // names the CLI's wallet holds privately: [{name}]
    property string me: ""             // the name posts are made as
    property string view: "all"        // all, following or thread
    property string threadOf: ""       // the post a thread view shows, <name>/<n>
    property var posts: []             // what the view shows, from board_feed / board_thread
    property bool loading: false
    property string loadError: ""
    property var follows: []           // names this device follows
    property var people: ({})          // name → {display, color, pixels}, as looked up
    property string replyTo: ""        // the post the composer answers, <name>/<n>
    property var job: null             // the change being proved: {id, kind, started, label}
    property real clockTick: Date.now()
    property string notice: ""
    property bool noticeIsError: false
    property string lastReply: ""

    readonly property bool busy: job !== null

    // ── Shared with the Names app (copied by tools/shared.py; edit there) ──

    readonly property color slate: "#0F1115"

    readonly property color panel: "#171A20"

    readonly property color raised: "#2B303B"

    readonly property color hover: "#20242D"

    readonly property color line: "#434A59"

    readonly property color ink: "#FFFFFF"

    readonly property color dim: "#E6EAF1"

    readonly property color faint: "#9AA3B3"   // placeholders only

    readonly property color brass: "#F0C562"

    readonly property color alarm: "#FF9C8C"

    readonly property string sans: "Avenir Next"

    readonly property string mono: "Menlo"

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule

    /// A module's reply parsed as far as it will go. Modules answer with JSON
    /// text, sometimes wrapped in a JSON string.
    function parsed(reply) {
        for (var depth = 0; depth < 2 && typeof reply === "string"; depth++) {
            try {
                reply = JSON.parse(reply)
            } catch (error) {
                break
            }
        }
        return reply
    }

    /// Calls a module and waits for its reply. For quick reads only: the
    /// screen stands still until the module answers.
    function call(module, method, args) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var reply = logos.callModule(module, method, args)
        lastReply = module + "." + method + " → " + String(reply).substr(0, 400)
        return parsed(reply)
    }

    /// Asks the names module, returning its value or throwing its error.
    function ask(method, args) {
        var reply = call("names", method, args)
        if (!reply || reply.success === undefined)
            throw new Error("The names module did not answer. Is it installed?")
        if (!reply.success)
            throw new Error(reply.error)
        return reply.value
    }

    /// Asks the names module without holding the screen still, for anything
    /// that waits on the wallet. Exactly one of `done(value)` and
    /// `failed(message)` runs, later.
    function askLater(method, args, done, failed, patience) {
        function settle(reply) {
            lastReply = "names." + method + " → " + String(reply).substr(0, 400)
            reply = parsed(reply)
            if (!reply || reply.success === undefined)
                failed("The names module did not answer. Is it installed?")
            else if (!reply.success)
                failed(reply.error)
            else
                done(reply.value)
        }
        if (bridgeReady && logos.callModuleAsync)
            logos.callModuleAsync("names", method, args, settle, patience || 30000)
        else
            Qt.callLater(function() {
                try {
                    settle(logos.callModule("names", method, args))
                } catch (error) {
                    failed(error.message)
                }
            })
    }

    /// A stable number from a string, for drawing an owner's seal.
    function seedOf(text) {
        var hash = 2166136261
        for (var i = 0; i < text.length; i++) {
            hash ^= text.charCodeAt(i)
            hash = (hash * 16777619) >>> 0
        }
        return hash
    }

    /// The bytes of base64 text, as numbers.
    function base64Bytes(text) {
        var table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        var out = []
        var bits = 0, count = 0
        for (var i = 0; i < text.length; i++) {
            var value = table.indexOf(text.charAt(i))
            if (value < 0)
                continue
            bits = (bits << 6) | value
            count += 6
            if (count >= 8) {
                count -= 8
                out.push((bits >> count) & 255)
            }
        }
        return out
    }

    /// A `#rrggbb` colour bright enough to read on this app's near-black and
    /// to carry dark text: a dark one is mixed with white until it does.
    function readable(hex) {
        var r = parseInt(hex.substr(1, 2), 16), g = parseInt(hex.substr(3, 2), 16), b = parseInt(hex.substr(5, 2), 16)
        function luminance(r, g, b) {
            function channel(v) { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
            return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
        }
        for (var t = 0; t < 1 && luminance(r, g, b) < 0.3; t += 0.1) {
            r = r + (255 - r) * 0.1
            g = g + (255 - g) * 0.1
            b = b + (255 - b) * 0.1
        }
        return Qt.rgba(r / 255, g / 255, b / 255, 1)
    }

    /// A chain time in milliseconds as a time of day.
    function clock(milliseconds) {
        return Qt.formatTime(new Date(milliseconds), "HH:mm")
    }

    /// A time in milliseconds as a date.
    function dated(milliseconds) {
        return Qt.formatDate(new Date(milliseconds), "d MMMM yyyy")
    }

    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.", false)
    }

    function lastLogLine(text) {
        var lines = String(text || "").split("\n")
        for (var i = lines.length - 1; i >= 0; i--)
            if (lines[i].trim() !== "")
                return lines[i].trim()
        return "it stopped without saying why"
    }

    /// An owner's seal: rings cut from the owner's account ID, so every name one
    /// owner holds wears the same seal and a look-alike name wears another.
    component Seal: Canvas {
        id: seal
        property string owner: ""
        property string letter: ""
        onOwnerChanged: requestPaint()
        onWidthChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            if (owner === "")
                return
            var state = root.seedOf(owner)
            function next() {
                state ^= state << 13; state >>>= 0
                state ^= state >>> 17
                state ^= state << 5; state >>>= 0
                return state / 4294967296
            }
            var size = Math.min(width, height)
            var centre = size / 2
            var hue = Math.floor(next() * 360)
            ctx.lineCap = "butt"
            for (var ring = 0; ring < 4; ring++) {
                var radius = centre * (0.94 - ring * 0.17)
                var cuts = 3 + Math.floor(next() * 9)
                var turn = next() * Math.PI * 2
                var gap = 0.10 + next() * 0.22
                ctx.lineWidth = Math.max(1.5, size * (0.035 + next() * 0.05))
                ctx.strokeStyle = Qt.hsla(((hue + ring * 24) % 360) / 360, 0.52, 0.66, 0.95 - ring * 0.17)
                for (var cut = 0; cut < cuts; cut++) {
                    var from = turn + (cut / cuts) * Math.PI * 2
                    ctx.beginPath()
                    ctx.arc(centre, centre, radius - ctx.lineWidth / 2, from, from + (Math.PI * 2 / cuts) * (1 - gap))
                    ctx.stroke()
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: seal.letter !== "" && seal.width >= 60
            text: seal.letter.toUpperCase()
            color: root.ink
            font.family: root.sans
            font.weight: Font.Medium
            font.pixelSize: seal.width * 0.22
        }
    }

    /// A button. `changes` marks one that sends a change to the chain: it rests
    /// while another change is waiting to be confirmed. `grave` is for the one
    /// that cannot be undone.
    component Action: Rectangle {
        id: action
        property string label: ""
        property bool strong: false
        property bool grave: false
        property bool changes: false
        property bool enabledNow: true
        property color tint: root.brass
        readonly property bool live: enabledNow && !(changes && root.busy)
        readonly property color fill: grave ? root.alarm : tint
        signal pressed()
        implicitWidth: actionText.implicitWidth + 28
        implicitHeight: 38
        radius: 6
        color: !live ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(fill, 1.08) : fill) : (actionArea.containsMouse ? root.raised : "transparent")
        border.width: strong && live ? 0 : 1
        border.color: activeFocus ? root.brass : grave ? root.alarm : root.line
        opacity: live ? 1 : 0.55
        activeFocusOnTab: live
        Keys.onReturnPressed: if (live) action.pressed()
        Keys.onSpacePressed: if (live) action.pressed()
        Text {
            id: actionText
            anchors.centerIn: parent
            text: action.label
            color: action.strong && action.live ? root.slate : action.grave ? root.alarm : root.ink
            font.family: root.sans
            font.pixelSize: 18
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

    /// A small button that is only a drawing: "search", "gear" or "trash".
    /// `boxed` gives it the outline of a field, to sit beside one.
    component IconButton: Rectangle {
        id: iconButton
        property string kind: ""
        property bool boxed: false
        property bool grave: false
        readonly property color tint: iconArea.containsMouse || activeFocus ? (grave ? root.alarm : root.brass) : root.ink
        signal pressed()
        implicitWidth: boxed ? 36 : 26
        implicitHeight: boxed ? 36 : 26
        radius: 6
        color: iconArea.containsMouse ? root.raised : boxed ? root.slate : "transparent"
        border.width: boxed || activeFocus ? 1 : 0
        border.color: activeFocus ? root.brass : root.line
        activeFocusOnTab: true
        Keys.onReturnPressed: iconButton.pressed()
        Keys.onSpacePressed: iconButton.pressed()
        onTintChanged: glyph.requestPaint()
        Canvas {
            id: glyph
            anchors.centerIn: parent
            width: 16
            height: 16
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                ctx.strokeStyle = iconButton.tint
                ctx.lineCap = "round"
                ctx.lineJoin = "round"
                ctx.lineWidth = 1.5
                ctx.beginPath()
                if (iconButton.kind === "search") {
                    ctx.arc(6.75, 6.75, 4.5, 0, Math.PI * 2)
                    ctx.moveTo(10.2, 10.2)
                    ctx.lineTo(14.2, 14.2)
                } else if (iconButton.kind === "gear") {
                    for (var tooth = 0; tooth < 8; tooth++) {
                        var angle = tooth * Math.PI / 4
                        ctx.moveTo(8 + Math.cos(angle) * 5.2, 8 + Math.sin(angle) * 5.2)
                        ctx.lineTo(8 + Math.cos(angle) * 7, 8 + Math.sin(angle) * 7)
                    }
                    ctx.moveTo(12.6, 8)
                    ctx.arc(8, 8, 4.6, 0, Math.PI * 2)
                    ctx.moveTo(9.8, 8)
                    ctx.arc(8, 8, 1.8, 0, Math.PI * 2)
                } else if (iconButton.kind === "trash") {
                    ctx.moveTo(2.5, 4.5)
                    ctx.lineTo(13.5, 4.5)
                    ctx.moveTo(6, 4.5)
                    ctx.lineTo(6, 2.5)
                    ctx.lineTo(10, 2.5)
                    ctx.lineTo(10, 4.5)
                    ctx.moveTo(4, 4.5)
                    ctx.lineTo(4.8, 14)
                    ctx.lineTo(11.2, 14)
                    ctx.lineTo(12, 4.5)
                    ctx.moveTo(6.8, 7.2)
                    ctx.lineTo(6.9, 11.3)
                    ctx.moveTo(9.2, 7.2)
                    ctx.lineTo(9.1, 11.3)
                }
                ctx.stroke()
            }
        }
        MouseArea {
            id: iconArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: iconButton.pressed()
        }
    }

    /// A quiet text button, for the small things beside the main actions.
    component Link: Text {
        id: link
        signal pressed()
        color: linkArea.containsMouse ? root.brass : root.ink
        font.family: root.sans
        font.pixelSize: 17
        font.underline: true
        MouseArea {
            id: linkArea
            anchors.fill: parent
            anchors.margins: -6
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: link.pressed()
        }
    }

    component Field: Rectangle {
        id: field
        property alias text: input.text
        property string hint: ""
        property bool numbersOnly: false
        property bool code: false
        property int size: 18
        signal accepted()
        signal edited()
        implicitHeight: size + 22
        radius: 6
        color: root.slate
        border.width: 1
        border.color: input.activeFocus ? root.brass : root.line
        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            verticalAlignment: TextInput.AlignVCenter
            color: root.ink
            selectionColor: root.brass
            selectedTextColor: root.slate
            font.family: field.code ? root.mono : root.sans
            font.pixelSize: field.code ? field.size - 1 : field.size
            clip: true
            selectByMouse: true
            validator: field.numbersOnly ? digits : null
            onAccepted: field.accepted()
            onEditingFinished: field.edited()
            // A long value set from outside shows its start, not its end.
            onTextChanged: if (!activeFocus) cursorPosition = 0
        }
        RegularExpressionValidator { id: digits; regularExpression: /[0-9]{0,15}/ }
        Text {
            anchors.fill: input
            verticalAlignment: Text.AlignVCenter
            visible: input.text === ""
            text: field.hint
            color: root.faint
            font.family: root.sans
            font.pixelSize: field.size
            elide: Text.ElideRight
        }
    }

    /// A field for many lines, for writing a page.
    component Area: Rectangle {
        id: area
        property alias text: areaInput.text
        property string hint: ""
        property bool code: true
        implicitHeight: 150
        radius: 6
        color: root.slate
        border.width: 1
        border.color: areaInput.activeFocus ? root.brass : root.line
        Flickable {
            id: areaFlick
            anchors.fill: parent
            anchors.margins: 12
            contentHeight: areaInput.height
            clip: true
            TextEdit {
                id: areaInput
                width: areaFlick.width
                height: Math.max(areaFlick.height, implicitHeight)
                color: root.ink
                selectionColor: root.brass
                selectedTextColor: root.slate
                font.family: area.code ? root.mono : root.sans
                font.pixelSize: area.code ? 16 : 18
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                selectByMouse: true
            }
        }
        Text {
            anchors.fill: parent
            anchors.margins: 12
            visible: areaInput.text === ""
            text: area.hint
            color: root.faint
            font.family: root.sans
            font.pixelSize: 17
            wrapMode: Text.Wrap
        }
    }

    /// A rounded label in a row of facts; `link` makes it a button.
    component Chip: Rectangle {
        id: chip
        property string label: ""
        property bool link: false
        signal pressed()
        implicitHeight: 34
        implicitWidth: chipText.implicitWidth + 30
        radius: 17
        color: chip.link && chipArea.containsMouse ? root.hover : "transparent"
        border.width: 1
        border.color: root.line
        Text {
            id: chipText
            anchors.centerIn: parent
            text: chip.label
            color: chip.link ? root.brass : root.dim
            font.family: root.sans
            font.pixelSize: 15
        }
        MouseArea {
            id: chipArea
            anchors.fill: parent
            enabled: chip.link
            hoverEnabled: true
            cursorShape: chip.link ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: chip.pressed()
        }
    }

    /// One tab of the Edit screen.
    component Tab: Rectangle {
        id: tab
        property string label: ""
        property bool current: false
        signal pressed()
        implicitHeight: 44
        implicitWidth: tabText.implicitWidth + 32
        radius: 8
        color: tab.current ? root.raised : tabArea.containsMouse ? root.hover : "transparent"
        Text {
            id: tabText
            anchors.centerIn: parent
            text: tab.label
            color: tab.current ? root.ink : root.dim
            font.family: root.sans
            font.pixelSize: 17
            font.weight: tab.current ? Font.DemiBold : Font.Normal
        }
        MouseArea {
            id: tabArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: tab.pressed()
        }
    }

    /// A box with no heading of its own, for side-by-side parts.
    component Card: Rectangle {
        default property alias content: cardColumn.data
        implicitHeight: cardColumn.implicitHeight + 52
        radius: 12
        color: root.panel
        border.width: 1
        border.color: root.line
        ColumnLayout {
            id: cardColumn
            anchors.fill: parent
            anchors.margins: 26
            spacing: 14
        }
    }

    /// A line of explanation under a control.
    component Hint: Text {
        Layout.fillWidth: true
        color: root.dim
        font.family: root.sans
        font.weight: Font.Medium
        font.pixelSize: 17
        lineHeight: 1.25
        wrapMode: Text.Wrap
    }

    /// The heading of one part of the edit panel.
    component Part: Text {
        Layout.topMargin: 10
        color: root.ink
        font.family: root.sans
        font.pixelSize: 18
        font.weight: Font.Bold
    }

    /// The name's picture: the holder's own image when the profile has one,
    /// painted block by block from the pixels the module decoded (never from a
    /// URL), so pixel art stays pixel art; otherwise the owner's seal. The
    /// plate behind it is plain slate, so the round cut is painted over the
    /// corners rather than masked, which needs no extra Qt module.
    component Avatar: Item {
        id: avatar
        property var pixels: null   // {width, height, pixels: base64 RGBA8}
        property string owner: ""
        property string letter: ""
        property color ring: root.brass
        readonly property var bytes: pixels !== null ? root.base64Bytes(pixels.pixels) : []
        readonly property bool showsImage: pixels !== null && bytes.length === pixels.width * pixels.height * 4
        Seal {
            anchors.fill: parent
            visible: !avatar.showsImage
            owner: avatar.owner
            letter: avatar.letter
        }
        Canvas {
            id: cut
            anchors.fill: parent
            visible: avatar.showsImage
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var size = Math.min(width, height)
                var centre = size / 2
                if (avatar.showsImage) {
                    // Fill the circle as an image cropped to a square would,
                    // each source pixel a block with whole-pixel edges, and
                    // runs of one colour in a row drawn as one block.
                    var w = avatar.pixels.width, h = avatar.pixels.height, data = avatar.bytes
                    var scale = size / Math.min(w, h)
                    var left = (size - w * scale) / 2, top = (size - h * scale) / 2
                    for (var y = 0; y < h; y++) {
                        var y0 = Math.round(top + y * scale), y1 = Math.round(top + (y + 1) * scale)
                        var x = 0
                        while (x < w) {
                            var at = (y * w + x) * 4
                            var run = x + 1
                            while (run < w && data[at] === data[(y * w + run) * 4] && data[at + 1] === data[(y * w + run) * 4 + 1]
                                   && data[at + 2] === data[(y * w + run) * 4 + 2] && data[at + 3] === data[(y * w + run) * 4 + 3])
                                run++
                            if (data[at + 3] > 0) {
                                ctx.fillStyle = "rgba(" + data[at] + "," + data[at + 1] + "," + data[at + 2] + "," + (data[at + 3] / 255) + ")"
                                var x0 = Math.round(left + x * scale), x1 = Math.round(left + run * scale)
                                ctx.fillRect(x0, y0, x1 - x0, y1 - y0)
                            }
                            x = run
                        }
                    }
                }
                ctx.fillStyle = root.slate
                ctx.beginPath()
                ctx.rect(0, 0, size, size)
                ctx.arc(centre, centre, centre - 1, 0, Math.PI * 2, true)
                ctx.fill()
                ctx.lineWidth = 3
                ctx.strokeStyle = avatar.ring
                ctx.beginPath()
                ctx.arc(centre, centre, centre - 2.5, 0, Math.PI * 2)
                ctx.stroke()
            }
        }
        onRingChanged: cut.requestPaint()
        onBytesChanged: cut.requestPaint()
        onShowsImageChanged: cut.requestPaint()
        onWidthChanged: cut.requestPaint()
    }

    /// A name, bare (SPEC 2). The `.lez` suffix is kept here, hidden, for the
    /// long form; it follows at the same size, in a quieter
    /// colour.
    component NameLabel: Item {
        id: label
        property string name: ""
        property int size: 17
        property int weight: Font.Medium
        implicitWidth: bare.implicitWidth + suffix.implicitWidth
        implicitHeight: bare.implicitHeight
        Text {
            id: bare
            width: Math.min(implicitWidth, Math.max(0, label.width - suffix.implicitWidth))
            text: label.name
            color: root.ink
            font.family: root.sans
            font.weight: label.weight
            font.pixelSize: label.size
            elide: Text.ElideRight
        }
        Text {
            id: suffix
            anchors.left: bare.right
            anchors.baseline: bare.baseline
            visible: false
            text: ".lez"
            color: root.dim
            font.family: root.sans
            font.weight: label.weight
            font.pixelSize: label.size
        }
    }

    // ── Small helpers ───────────────────────────────────────────────────

    function say(message, isError) {
        notice = String(message)
        noticeIsError = !!isError
    }

    /// How long ago `time` (milliseconds) was, in a few characters.
    function ago(time) {
        var seconds = Math.max(0, Math.floor((clockTick - time) / 1000))
        if (seconds < 60) return seconds + "s"
        if (seconds < 3600) return Math.floor(seconds / 60) + "m"
        if (seconds < 86400) return Math.floor(seconds / 3600) + "h"
        return dated(time)
    }

    function nameOf(post) {
        return String(post).split("/")[0]
    }

    function follows_(name) {
        return follows.indexOf(name) >= 0
    }

    // ── Loading ─────────────────────────────────────────────────────────

    function load() {
        loading = true
        loadError = ""
        var method = view === "thread" ? "board_thread" : "board_feed"
        var args = view === "thread" ? [threadOf] : [view === "following"]
        askLater(method, args, function(value) {
            loading = false
            posts = value || []
            for (var i = 0; i < posts.length; i++)
                lookUp(posts[i].name)
        }, function(message) {
            loading = false
            loadError = message
        }, 150000)
    }

    /// Fetches a name's display name, colour and avatar once, for its posts.
    function lookUp(name) {
        if (people[name] !== undefined)
            return
        var known = Object.assign({}, people)
        known[name] = { display: "", color: "", pixels: null }
        people = known
        askLater("resolve", [name], function(found) {
            if (found === null)
                return
            var texts = found.texts || {}
            var next = Object.assign({}, people)
            next[name] = Object.assign({}, next[name], {
                display: typeof texts.display === "string" ? texts.display.trim() : "",
                color: /^#[0-9a-fA-F]{6}$/.test(texts.color || "") ? readable(texts.color) : ""
            })
            people = next
            if (typeof texts.avatar === "string" && texts.avatar.trim() !== "")
                askLater("avatar", [name], function(value) {
                    var ok = value && value.width > 0 && value.height > 0 && value.width <= 128 && value.height <= 128
                    if (!ok) return
                    var more = Object.assign({}, people)
                    more[name] = Object.assign({}, more[name], { pixels: { width: value.width, height: value.height, pixels: value.pixels } })
                    people = more
                }, function() {})
        }, function() {})
    }

    function loadMine() {
        askLater("cli_mine", [], function(value) {
            var held = (value || []).filter(function(entry) { return !entry.registering && !entry.failed && entry.private !== false })
            mine = held
            if (me === "" || !held.some(function(entry) { return entry.name === me }))
                me = held.length > 0 ? held[0].name : ""
        }, function(message) {
            mine = []
            console.log("board_ui: names:", message)
        }, 30000)
    }

    function loadFollows() {
        askLater("board_follows", [], function(value) { follows = value || [] }, function() {})
    }

    function show(which, post) {
        view = which
        threadOf = post || ""
        posts = []
        load()
    }

    // ── Changes ─────────────────────────────────────────────────────────

    /// Starts a change as a CLI job, and polls it until it ends.
    function start(method, args, label, after) {
        if (busy)
            return
        say("", false)
        job = { id: "", label: label, started: Date.now(), after: after }
        askLater(method, args, function(value) {
            job.id = value.id
            job.started = value.started || Date.now()
            clockTick = Date.now()
        }, function(message) {
            job = null
            say(message, true)
        })
    }

    function checkJob() {
        if (job === null || job.id === "" || job.asking)
            return
        var current = job
        current.asking = true
        askLater("job", [current.id], function(value) {
            current.asking = false
            if (job !== current || value.state === "running")
                return
            job = null
            if (value.state === "done") {
                say(current.label + " done.", false)
                if (current.after)
                    current.after()
                load()
            } else {
                say("It didn't go through: " + lastLogLine(value.log_tail), true)
            }
        }, function() { current.asking = false })
    }

    function post() {
        var text = composer.text.trim()
        var cid = cidField.text.trim()
        if (me === "" || (text === "" && cid === ""))
            return
        var reply = replyTo
        start("board_post", [me, text, JSON.stringify(cid === "" ? [] : [cid]), reply],
              reply !== "" ? "Your reply" : "Your post", function() {
            composer.text = ""
            cidField.text = ""
            replyTo = ""
        })
    }

    function like(target, undo) {
        start("board_like", [me, target, undo], undo ? "Unlike" : "Like")
    }

    function follow(name, undo) {
        askLater("board_follow", [name, undo], function(value) {
            follows = value || []
            if (view === "following")
                load()
        }, function(message) { say(message, true) })
    }

    function hide(target) {
        start("board_hide", [target], "Taking it down")
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.job !== null
        onTriggered: { root.clockTick = Date.now(); root.checkJob() }
    }
    Timer {
        interval: 60000
        repeat: true
        running: root.job === null && root.view !== "thread"
        onTriggered: { root.clockTick = Date.now(); root.load() }
    }
    Component.onCompleted: {
        loadMine()
        loadFollows()
        load()
    }

    TextEdit { id: clipboard; visible: false }

    // ── Layout ──────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Who you post as, and which feed.
        Rectangle {
            Layout.preferredWidth: 280
            Layout.fillHeight: true
            color: root.panel

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 18
                spacing: 14

                Text { text: "Board"; color: root.ink; font.family: root.sans; font.pixelSize: 28; font.weight: Font.DemiBold }

                Text { text: "POSTING AS"; color: root.faint; font.family: root.sans; font.pixelSize: 13; font.weight: Font.DemiBold; font.letterSpacing: 1 }
                Repeater {
                    model: root.mine
                    Tab {
                        required property var modelData
                        Layout.fillWidth: true
                        label: modelData.name
                        current: root.me === modelData.name
                        onPressed: root.me = modelData.name
                    }
                }
                Hint {
                    visible: root.mine.length === 0
                    font.pixelSize: 15
                    text: "No names to post as. Register one privately in the Names app first."
                }

                Rectangle { Layout.fillWidth: true; height: 1; color: root.line }

                Tab { Layout.fillWidth: true; label: "All"; current: root.view === "all"; onPressed: root.show("all") }
                Tab { Layout.fillWidth: true; label: "Following" + (root.follows.length > 0 ? "  " + root.follows.length : ""); current: root.view === "following"; onPressed: root.show("following") }

                Repeater {
                    model: root.follows
                    RowLayout {
                        required property string modelData
                        Layout.fillWidth: true
                        Layout.leftMargin: 12
                        Text { Layout.fillWidth: true; text: modelData; color: root.dim; font.family: root.sans; font.pixelSize: 15; elide: Text.ElideRight }
                        Link { text: "Unfollow"; color: root.faint; font.pixelSize: 13; onPressed: root.follow(modelData, true) }
                    }
                }

                Item { Layout.fillHeight: true }
                Hint {
                    font.pixelSize: 13
                    text: "Posts are public. Posting proves on this computer (about 4 min), so the chain sees your name, never your account."
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }

        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: page.implicitHeight + 72
            clip: true

            ColumnLayout {
                id: page
                x: 40
                y: 32
                width: Math.min(parent.width - 80, 720)
                spacing: 16

                // The composer.
                Card {
                    visible: root.mine.length > 0 && root.view !== "thread" || root.replyTo !== ""
                    Layout.fillWidth: true
                    RowLayout {
                        Layout.fillWidth: true
                        visible: root.replyTo !== ""
                        Text { Layout.fillWidth: true; text: "Replying to " + root.replyTo; color: root.brass; font.family: root.sans; font.pixelSize: 15 }
                        Link { text: "Cancel"; color: root.faint; font.pixelSize: 14; onPressed: root.replyTo = "" }
                    }
                    Area {
                        id: composer
                        Layout.fillWidth: true
                        implicitHeight: 96
                        code: false
                        hint: root.me !== "" ? "What's happening, " + root.me + "?" : "Pick a name to post as"
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Field {
                            id: cidField
                            Layout.fillWidth: true
                            size: 15
                            code: true
                            hint: "Attach a file: its Logos Storage CID (optional)"
                        }
                        Text {
                            text: composer.text.length + " / 1024"
                            color: composer.text.length > 1024 ? root.alarm : root.faint
                            font.family: root.sans
                            font.pixelSize: 13
                        }
                        Action {
                            label: root.replyTo !== "" ? "Reply" : "Post"
                            strong: true
                            enabledNow: !root.busy && root.me !== "" && composer.text.length <= 1024 && (composer.text.trim() !== "" || cidField.text.trim() !== "")
                            onPressed: root.post()
                        }
                    }
                }

                // The change being proved, and anything to report.
                Hint {
                    visible: root.job !== null
                    color: root.brass
                    font.pixelSize: 16
                    text: root.job === null ? "" : root.job.label + ": proving it on this computer, so the chain never learns your account. About 4 min, "
                        + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.job.started) / 60000)) + " so far."
                }
                Hint {
                    visible: root.job === null && root.notice !== ""
                    color: root.noticeIsError ? root.alarm : root.dim
                    text: root.notice
                }

                RowLayout {
                    Layout.fillWidth: true
                    Link {
                        visible: root.view === "thread"
                        text: "← Back"
                        color: root.faint
                        font.underline: false
                        onPressed: root.show("all")
                    }
                    Text {
                        Layout.fillWidth: true
                        text: root.view === "thread" ? "Thread" : root.view === "following" ? "Following" : "All posts"
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: 22
                        font.weight: Font.DemiBold
                    }
                    Link { text: root.loading ? "Loading…" : "Refresh"; color: root.faint; font.pixelSize: 15; onPressed: root.load() }
                }

                Hint {
                    visible: !root.loading && root.posts.length === 0
                    font.pixelSize: 17
                    text: root.loadError !== "" ? "Couldn't load the board: " + root.loadError
                        : root.view === "following" && root.follows.length === 0 ? "You don't follow anyone yet. Press Follow on a post."
                        : "No posts yet."
                }

                Repeater {
                    model: root.posts
                    Card {
                        id: card
                        required property var modelData
                        readonly property var who: root.people[modelData.name] || ({ display: "", color: "", pixels: null })
                        readonly property bool isMine: root.mine.some(function(entry) { return entry.name === card.modelData.name })
                        visible: !modelData.hidden
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 14
                            Avatar {
                                Layout.preferredWidth: 44
                                Layout.preferredHeight: 44
                                Layout.alignment: Qt.AlignTop
                                pixels: card.who.pixels
                                owner: card.modelData.name
                                letter: card.modelData.name.charAt(0)
                                ring: card.who.color !== "" ? card.who.color : root.brass
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Text {
                                        visible: card.who.display !== ""
                                        text: card.who.display
                                        textFormat: Text.PlainText
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: 17
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        text: card.modelData.name
                                        color: card.who.display !== "" ? root.faint : root.ink
                                        font.family: root.sans
                                        font.pixelSize: card.who.display !== "" ? 15 : 17
                                        font.weight: card.who.display !== "" ? Font.Normal : Font.DemiBold
                                    }
                                    Text { text: "· " + root.ago(card.modelData.time); color: root.faint; font.family: root.sans; font.pixelSize: 14 }
                                    Item { Layout.fillWidth: true }
                                    Link {
                                        visible: !card.isMine
                                        text: root.follows_(card.modelData.name) ? "Following" : "Follow"
                                        color: root.follows_(card.modelData.name) ? root.faint : root.brass
                                        font.pixelSize: 14
                                        onPressed: root.follow(card.modelData.name, root.follows_(card.modelData.name))
                                    }
                                }
                                Text {
                                    visible: card.modelData.parent !== null && card.modelData.parent !== undefined
                                    text: "replying to " + (card.modelData.parent ? root.nameOf(card.modelData.parent) : "")
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: 13
                                }
                                Text {
                                    Layout.fillWidth: true
                                    visible: card.modelData.text !== ""
                                    text: card.modelData.text
                                    textFormat: Text.PlainText
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: 17
                                    lineHeight: 1.2
                                    wrapMode: Text.Wrap
                                }
                                Flow {
                                    Layout.fillWidth: true
                                    visible: card.modelData.cids.length > 0
                                    spacing: 8
                                    Repeater {
                                        model: card.modelData.cids
                                        Chip {
                                            required property string modelData
                                            label: "File " + modelData.substr(0, 10) + "… · Copy CID"
                                            link: true
                                            onPressed: root.copy(modelData, "The CID")
                                        }
                                    }
                                }
                                RowLayout {
                                    spacing: 18
                                    Link {
                                        text: "Reply"
                                        color: root.faint
                                        font.pixelSize: 14
                                        visible: root.mine.length > 0
                                        onPressed: { root.replyTo = card.modelData.post; composer.forceActiveFocus() }
                                    }
                                    Link {
                                        text: "♥ " + Math.max(0, card.modelData.likes)
                                        color: root.faint
                                        font.pixelSize: 14
                                        font.underline: false
                                        onPressed: if (root.mine.length > 0) root.like(card.modelData.post, false)
                                    }
                                    Link {
                                        visible: root.view !== "thread"
                                        text: "Thread"
                                        color: root.faint
                                        font.pixelSize: 14
                                        onPressed: root.show("thread", card.modelData.parent ? card.modelData.parent : card.modelData.post)
                                    }
                                    Link {
                                        visible: card.isMine
                                        text: "Take down"
                                        color: root.faint
                                        font.pixelSize: 14
                                        onPressed: root.hide(card.modelData.post)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
