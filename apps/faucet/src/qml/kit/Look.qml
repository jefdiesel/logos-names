// The shared look: palette and type tokens, the same names in every app (Names Wallet is the
// reference). Every kit part holds one (`property Look look: Look {}`); an app may pass its own.
import QtQuick
import Logos.Theme

QtObject {
    readonly property color slate: Theme.palette.background
    readonly property color card: Theme.isDark ? Theme.palette.backgroundSecondary : Theme.palette.surfaceRaised
    readonly property color raised: Theme.isDark ? Theme.palette.surface : Theme.palette.backgroundButton
    readonly property color hover: Theme.palette.surfaceInteractiveHover
    readonly property color line: Theme.palette.border
    readonly property color ink: Theme.palette.text
    readonly property color dim: Theme.palette.textSecondary
    readonly property color faint: Theme.palette.textPlaceholder   // placeholders only
    // The accent: the primary button's fill, focus rings, a chosen row. Nothing else.
    readonly property color brass: Theme.isDark ? Theme.palette.warning : Theme.palette.primary
    readonly property color alarm: Theme.palette.error             // problem status lines only
    readonly property color good: Theme.palette.success            // done status lines only
    readonly property string sans: "Avenir Next"
    readonly property string mono: "Menlo"
    readonly property int pageWidth: 680       // a page column, at most
    readonly property int indexWidth: 300      // an index beside a page
    readonly property int edge: 56             // from the window's edge to everything
    function px(size) { return Math.round(size * (Theme.typography.scale || 1)) }
}
