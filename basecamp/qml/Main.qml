// Logos Names for Basecamp: find, register and manage names.
//
// A front end on the `names` module. The module reads the chain itself, so
// browsing needs no wallet. Registering a name, changing it, transferring it
// and deleting it are signed by the person's own `names` CLI wallet, which the
// module asks on this screen's behalf. Messaging goes through `chat_module`.
//
// A name held privately by the person's own `names` CLI (its wallet holds the
// key, and its changes are proven on this machine) is edited through the
// module's cli_* methods instead: each starts the CLI in the background and
// this screen polls the job until it ends.
//
// Controls are drawn here rather than taken from Qt Quick Controls so the app
// looks the same whatever style its host applies.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic

Item {
    id: root

    // ── State ───────────────────────────────────────────────────────────

    property var myNames: []           // [{name, display, owner, expires, never, lapsed}]: names the wallet holds; owner is the wallet account
    property var pendingNames: []      // [{name, display, owner, stage, ready_at, tx}]: registrations still finishing
    property real clockTick: Date.now()  // now, updated every few seconds for elapsed times
    property var cliPending: []        // [{name, display, state, registering, failed}]: the CLI's registrations still finishing, or stopped
    property var cliNames: []          // [{name, display, signer: "cli", state, lapsed}]: names the names CLI's wallet holds
    property string cliPath: ""        // the names CLI the module runs, and its wallet folder
    property string cliWallet: ""
    property bool revealing: false     // a pending registration is being asked about
    property int lockedUpTo: 0         // names this short are placed by the registry's admin (settings.locked_up_to), 0 unknown
    property var record: null          // the name on the plate
    property string forwardedFrom: ""  // the name asked for, when the plate shows where it forwards
    property var page: null
    property string editTab: "profile"   // the Edit screen's tab: profile, page or giveup
    property bool publicRegisterOpen: false
    property bool registering: false       // the "Register a name" screen is open
    // LEZ 0.4: a name registered from a public account publishes that account, so people can pay
    // it (the bridge page and `names pay` pay a name's published account). Ticked by default.
    property bool publishOnRegister: true
    property string publishAsk: ""         // the name whose "Let people pay it" is being confirmed
    // Get tokens: bridging from the person's Logos Blockchain node into LEZ (`cli_bridge`).
    property bool gettingTokens: false     // the "Get tokens" screen is open
    property string bridgeKey: ""          // the Logos Blockchain public key last bridged from, as the module keeps it
    property string bridgeNode: ""         // and the node, "" for Basecamp's
    property var bridgePlan: null          // the last checked plan: {total, notes, fee, to, lines}
    property string bridgePlanFor: ""      // what that plan was checked for: amount|key|node
    property bool bridgeBusy: false
    property bool bridgeConfirm: false     // "Bridge" was pressed and asks to confirm
    property string bridgeError: ""
    property string bridgeHash: ""         // the deposit last sent, "" if its hash wasn't readable
    property bool bridgeSent: false        // a deposit was sent from this screen
    property var bridgeSentLines: []       // what the CLI printed for it
    property string registerTaken: ""      // the name last checked there that someone holds
    property var cliBalance: undefined   // the names CLI's private account {account, balance}; undefined while asking, null if none // the public-account way to register is shown
    property bool entriesOpen: false     // the Entries part of a name's settings is shown
    property var pagePreview: null       // the page being written, as page_view would show it
    property string missing: ""        // a looked-up name that is not registered
    property var wallet: []            // the wallet's funded public accounts: [{account, balance, name}]
    property var privateWallet: []     // its funded private accounts: [{account, balance}]
    property int senderIndex: 0        // which of senders() pays
    property bool sending: false       // a payment is being prepared or sent
    property string walletProblem: ""  // why the wallet cannot be used, if it cannot
    property int fromIndex: 0
    property string tool: ""           // the open panel: "", "pay" or "settings"
    property string section: "wallet"  // wallet, messages, profile, search or settings
    // The board (Home): see board/ and the names module's board_* calls.
    property string boardView: "all"   // all, following or thread
    property string boardThreadOf: ""  // the post a thread view shows, <name>/<n>
    property var boardPosts: []
    property bool boardLoading: false
    property string boardError: ""
    property var boardFollows: []
    property var boardPeople: ({})     // name → {display, color, pixels}
    property string boardReplyTo: ""
    property var boardJob: null        // {id, label, started, after}
    property string savedMain: ""      // the main name the module keeps for this device
    property string anonName: ""       // this wallet's anonymous name, once made
    property bool anonOn: false        // the post box posts as the anon name
    property var boardLiked: []        // posts this device has liked, <name>/<n>
    property bool cliKnown: false      // the CLI's names have been read at least once
    property var boardSpelling: []     // [{word, suggestions}] in the post being written
    property string publicAsk: ""      // the name Settings is asking to confirm making public
    property bool registerPublic: false // a new registration then moves to its own public account
    // Messages: name-to-name DMs through Basecamp's chat module.
    property string chatState: ""       // the chat module's delivery state, "" until known
    property string chatError: ""
    property string chatMine: ""        // this installation's chat address
    property string mainChat: ""        // the chat address the main name publishes
    property var chatConvos: []         // [{convo_id, message_count, last_activity_ms, kind}]
    property var dmNames: ({})          // convo_id → name, as this device knows it
    property string chatOpenId: ""
    property var chatMessages: []       // [{from_self, content, timestamp_ms, sender}]
    property var chatNames: ({})        // chat address → name, as this device learned it
    property var chatMembers: []        // [{address, pending}] of the open group
    property bool chatNewGroup: false   // the New group form is open
    property bool boardAttach: false    // the post box shows its file field
    property bool registerHow: false    // the register card shows its three steps
    property var authorPosts: []        // the posts of the name on the plate, newest first
    property string authorOf: ""        // whose posts those are
    property bool authorLoading: false
    property int authorTotal: 0         // how many posts that name has made
    property bool anonPublic: false     // the anon name, once made, moves to its own public account
    property bool confirmingRelease: false
    property var pending: null         // a change that was sent and is not confirmed yet
    property string registry: ""
    property string sequencer: ""
    property string walletConfig: ""   // the wallet files the names module opens by itself
    property string walletStorage: ""
    property var deposit: null         // what the typed name would lock: {name, tier, deposit, mode}
    property var profileDraft: ({})    // the Profile fields as typed, by key
    property string notice: ""
    property bool noticeIsError: false
    property bool settingsOpen: false
    property bool configKnown: false   // the names module has answered once
    // The names module this app needs: the same release (metadata.json asks for it
    // at install, but an older module can still be the one Basecamp loaded).
    readonly property string neededModule: "0.17.0"
    // This app's own version, as in basecamp/metadata.json (a module test keeps them equal).
    readonly property string appVersion: "0.17.0"
    property string buildCommit: ""   // the module's build commit (get_config build_commit), "" for a dev build
    property var moduleFeatures: []    // get_config features: what this build of the names module can do
    property real machineMemory: 0     // get_config memory_bytes: this computer's memory (0: unknown)
    // One line before any proof on a computer with under 16 GB of memory (a proof takes about 9.5 GB).
    readonly property string proofWarning: machineMemory > 0 && machineMemory < 16 * 1024 * 1024 * 1024
        ? "This computer has " + Math.round(machineMemory / (1024 * 1024 * 1024)) + " GB of memory: a proof needs about 9.5 GB and 11 minutes or more, and it may be slow or fail here." : ""
    property string moduleOutdated: ""   // the loaded module's version when it is too old
    property string lastReply: ""
    property bool showLastReply: false // for finding out why something failed

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property bool busy: pending !== null
    readonly property string holder: record !== null ? holderOf(record.name) : ""
    // Held by the CLI's wallet and not by the open LEZ wallet: changed through the CLI.
    readonly property bool cliHeld: record !== null && holder === "" && cliHolds(record.name)
    readonly property bool mine: holder !== "" || cliHeld
    // "My names": the LEZ wallet's, then the CLI's, each with the signer that applies. The one
    // list of the names this wallet holds, wherever they are shown (Settings, the Names tab, the
    // "No name yet" card). A name the LEZ wallet holds is held by one of its public accounts.
    readonly property var allMine: {
        var all = myNames.map(function(entry) { return Object.assign({ signer: "wallet", "private": false }, entry) })
        for (var i = 0; i < cliNames.length; i++)
            if (!remembers(cliNames[i].name))
                all.push(cliNames[i])
        return all
    }
    readonly property string chatAddress: record !== null && record.texts && /^[0-9a-f]{64}$/.test(record.texts.chat || "") ? record.texts.chat : ""
    property bool pageSourceOpen: false  // the page's source is shown under it
    property bool pageAll: false         // a long page is shown in full, not folded
    property string pageLink: ""         // the link last pressed in the page, shown to copy or open
    // A page folds at about 30 lines; pageMetrics measures one.
    readonly property real pageFold: 30 * pageMetrics.height
    readonly property bool pageIsLong: pageText.implicitHeight > pageFold + pageMetrics.height * 3

    // The name's profile, as the holder set it. These entries are how the name
    // is shown, so they are drawn rather than listed. Each is checked against a
    // fixed form first: the holder writes them, and nothing else gets through.
    readonly property var presentationKeys: ["display", "bio", "avatar", "color", "theme", "font"]
    readonly property string profileDisplay: profileText("display")
    readonly property string profileBio: profileText("bio")
    // The avatar entry is a `data:` URL, which Basecamp's sandbox won't load,
    // so the module decodes it and the app paints the pixels: `{name, width,
    // height, pixels}` for the name on the plate, or null.
    property var avatarPixels: null
    readonly property var profileAvatar: record !== null && avatarPixels !== null && avatarPixels.name === record.name ? avatarPixels : null
    readonly property color accent: /^#[0-9a-fA-F]{6}$/.test(profileText("color")) ? readable(profileText("color")) : brass
    readonly property string headingFont: profileText("font") === "serif" ? "Georgia" : profileText("font") === "mono" ? mono : sans

    // ── Palette and type ────────────────────────────────────────────────

    // Text is white on near-black, and even the secondary grey stays above
    // 9:1 against the background, so nothing on screen reads as faint.
    // Colours come from the shared Logos theme, so Names follows the
    // person's choice of dark, light or high contrast.
    readonly property color slate: Theme.palette.background
    readonly property color panel: Theme.palette.backgroundSecondary
    // Boxes on the page: the panel colour at night, white paper by day.
    readonly property color card: Theme.isDark ? Theme.palette.backgroundSecondary : Theme.palette.surfaceRaised
    readonly property color raised: Theme.isDark ? Theme.palette.surface : Theme.palette.backgroundButton
    readonly property color hover: Theme.palette.surfaceInteractiveHover
    readonly property color line: Theme.palette.border
    readonly property color ink: Theme.palette.text
    readonly property color dim: Theme.palette.textSecondary
    readonly property color faint: Theme.palette.textPlaceholder   // placeholders only
    // The accent: Names' gold at night, the theme's one accent (Logos orange) by day.
    readonly property color brass: Theme.isDark ? Theme.palette.warning : Theme.palette.primary
    readonly property color alarm: Theme.palette.error
    readonly property color good: Theme.palette.success
    // The third tone: something of yours under way (a receive arriving from Ethereum). Not the
    // yellow of the primary buttons, not the grey of text, not green or blue: a calm violet.
    readonly property color underway: Theme.isDark ? "#B9A2EE" : "#6F4FB0"
    readonly property color underwayFill: Qt.rgba(underway.r, underway.g, underway.b, Theme.isDark ? 0.13 : 0.07)
    readonly property color underwayLine: Qt.rgba(underway.r, underway.g, underway.b, Theme.isDark ? 0.45 : 0.35)
    readonly property color noticeFill: raised   // a boxed note: neutral (colour belongs to status lines)
    readonly property color ownBubble: Qt.rgba(brass.r, brass.g, brass.b, 0.24)

    // ── One look for status, steps, headers and buttons (STYLE.md beside this file) ──
    // brass is the primary button's fill, focus and a chosen choice; nothing else. Status lines
    // carry the only other colours: waitTone (amber) for waiting, alarm (red) for a problem, good
    // for done, dim for working. The one exception: cautionTone on the consent box of a call the
    // wallet truly cannot decode (marked "!", not the clock).
    readonly property color waitTone: Theme.isDark ? "#F5A524" : "#A15C00"
    readonly property color cautionTone: waitTone
    function kindColor(kind) {
        return kind === "problem" ? alarm : kind === "waiting" ? waitTone : kind === "done" ? good : dim
    }
    // The text size of every status line, and the width of every page below the wallet.
    function statusPx() { return px(16) }
    readonly property int pageWidth: 680

    // Text sizes follow the theme's text size setting (1 where the host's
    // theme has none).
    function px(size) { return Math.round(size * (Theme.typography.scale || 1)) }
    readonly property string sans: "Avenir Next"
    readonly property string mono: "Menlo"

    // ── The Logos bridge ────────────────────────────────────────────────

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

    /// A reply as Advanced shows it, for finding out why something failed.
    /// A new wallet's reply holds its recovery phrase, so it is never kept.
    function shownReply(what, reply) {
        if (what === "names.cli_create_wallet" || what === "names.cli_restore_wallet")
            return what + " → (not shown: it may hold your recovery phrase)"
        return what + " → " + String(reply).substr(0, 400)
    }

    /// Calls a module and waits for its reply. For quick reads only: the
    /// screen stands still until the module answers.
    function call(module, method, args) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var reply = logos.callModule(module, method, args)
        lastReply = shownReply(module + "." + method, reply)
        return parsed(reply)
    }

    /// Asks the names module, returning its value or throwing its error.
    function ask(method, args) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var raw = logos.callModule("names", method, args)
        lastReply = shownReply("names." + method, raw)
        var answer = Logic.replyKind(raw)
        if (answer.kind === "missing")
            throw new Error(moduleMissing)
        if (answer.kind === "busy")
            throw new Error(answer.notTaken ? moduleBusy : moduleNoAnswer)
        if (answer.kind === "error")
            throw new Error(answer.error)
        return answer.value
    }

    /// The name `text` writes ("" when it isn't a name), from the module's `normalize`. Throws, as
    /// `ask` does, when the module is busy or missing: a busy answer is never taken as a name.
    function normalized(text) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var raw = logos.callModule("names", "normalize", [text])
        lastReply = shownReply("names.normalize", raw)
        var answer = Logic.normalizedReply(raw)
        if (answer.kind === "missing")
            throw new Error(moduleMissing)
        if (answer.kind !== "ok")
            throw new Error(answer.notTaken ? moduleBusy : moduleNoAnswer)
        return answer.value
    }

    // Two kinds of "not now", told apart (PROVIDER-SDK.md): busy is the module's own answer and
    // means the call was NOT taken, so pressing again is safe; no answer (a timeout) means it may
    // have been taken and may still be working, so look before doing it again. Polls ask again
    // after either (waitAgain).
    /// `done` for the answer to a read of the names network in use: dropped when the network
    /// changed since it was asked (S-A), so an account or balance read on one network is never
    /// shown, or copyable, under the other.
    function onNet(done) {
        var net = namesNet
        return function(value, reply) {
            if (Logic.sameNetAnswer(net, namesNet))
                done(value, reply)
        }
    }

    readonly property string moduleBusy: "The wallet is busy; still working."
    readonly property string moduleNoAnswer: "No answer from the wallet yet: it may still be working on it. Look before trying again."
    function waitAgain(message) {
        return message === moduleBusy || message === moduleNoAnswer
    }

    // The first half minute after Names opens, the names module may still be waiting for the
    // blockchain module: a READ that gets no answer then says "Starting…", plainly, not the red
    // no-answer line. A change never does (its no-answer means "look before trying again").
    // When the half minute is over, the reads that said so are asked again.
    readonly property string moduleStarting: "Starting… the wallet is still loading (up to half a minute after Basecamp opens)."
    property bool namesStarting: true
    Timer {
        interval: 30000
        running: root.bridgeReady && root.namesStarting
        onTriggered: {
            root.namesStarting = false
            if (root.notice === root.moduleStarting)
                root.notice = ""
            if (root.bridgeNetProblem === root.moduleStarting)
                root.loadBridgeNet()
            if (root.tokenError === root.moduleStarting)
                root.loadTokens()
        }
    }
    /// A read's failure, as shown: "Starting…" in the first half minute when it got no answer.
    function readFailed(message) {
        return namesStarting && waitAgain(message) ? moduleStarting : message
    }
    /// A read's failure, said: plainly while starting, else as an error.
    function sayRead(message) {
        var shown = readFailed(message)
        say(shown, shown !== moduleStarting)
    }
    // One-shot reads that met a busy module, asked again in 5 s (busy means the call wasn't taken:
    // the module was on another call that waits on the node or the Program Registry).
    property var busyAgain: ({})
    function againWhenFree(key, load) {
        var due = busyAgain
        due[key] = load
        busyAgain = due
        busyRetry.restart()
    }
    Timer {
        id: busyRetry
        interval: 5000
        onTriggered: {
            var due = root.busyAgain
            root.busyAgain = ({})
            for (var key in due)
                due[key]()
        }
    }
    readonly property string moduleMissing: "The names module is missing or out of date. In Basecamp, open Applications, click Names and press Install."
    // Asks still out, by number: one the host never answers is settled as busy after its
    // patience, so nothing that waits on it (the reveal, a refresh) stops for good.
    property var asksOut: ({})
    property int askCount: 0
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: {
            var now = Date.now()
            for (var id in root.asksOut)
                if (root.asksOut[id].deadline < now)
                    root.asksOut[id].expire()
        }
    }

    /// Asks the names module without holding the screen still, for anything
    /// that waits on the wallet. Exactly one of `done(value)` and
    /// `failed(message)` runs, later.
    function askLater(method, args, done, failed, patience) {
        var id = ++askCount
        var over = false
        function finish() {
            if (over)
                return false
            over = true
            delete asksOut[id]
            return true
        }
        asksOut[id] = { deadline: Date.now() + (patience || 30000) + 5000, expire: function() { if (finish()) failed(moduleNoAnswer) } }
        function settle(reply) {
            if (!finish())
                return
            lastReply = shownReply("names." + method, reply)
            // No reply, or one that isn't the module's, is a call that waited past its time: the
            // module answers one call at a time, so a slow one (a wallet catching up, a reveal)
            // holds the rest. Only the host saying the module isn't there asks for an install.
            var answer = Logic.replyKind(reply)
            if (answer.kind === "ok")
                done(answer.value, parsed(reply))
            else if (answer.kind === "error")
                failed(answer.error)
            else
                failed(answer.kind === "missing" ? moduleMissing : answer.notTaken ? moduleBusy : moduleNoAnswer)
        }
        if (bridgeReady && logos.callModuleAsync)
            logos.callModuleAsync("names", method, args, settle, patience || 30000)
        else
            Qt.callLater(function() {
                try {
                    settle(logos.callModule("names", method, args))
                } catch (error) {
                    if (finish())
                        failed(error.message)
                }
            })
    }

    /// A message as a sentence. One that opens with a name keeps the name's
    /// lower case.
    function sentence(message) {
        var text = String(message).trim()
        if (text === "")
            return ""
        var first = text.split(" ")[0]
        if (!/\.lez\b/i.test(first) && !isKnownName(first))
            text = text.charAt(0).toUpperCase() + text.substr(1)
        return /[.?!]$/.test(text) ? text : text + "."
    }

    /// Whether `word` is a name on screen or in My names, so a sentence
    /// that opens with it keeps its lower case.
    function isKnownName(word) {
        if ((record !== null && record.name === word) || missing === word || registerTaken === word)
            return true
        for (var i = 0; i < allMine.length; i++)
            if (allMine[i].name === word)
                return true
        for (var j = 0; j < cliPending.length; j++)
            if (cliPending[j].name === word)
                return true
        return false
    }

    /// Says one line in the page's status line. `how`: true for a failure (shown as a problem, or
    /// as a wait when its words are one: busy, no answer, starting), false for a plain result
    /// (done), or a kind said outright ("working", "waiting"). noticeIsError keeps meaning "it
    /// failed" (an app's request is answered with it), whatever colour the line has.
    function say(message, how) {
        notice = sentence(message)
        noticeIsError = how === true
        noticeKind = Logic.noticeKind(notice, how)
    }
    property string noticeKind: ""

    // ── Reading ─────────────────────────────────────────────────────────

    /// Reads the module's settings. At start-up the module may not be running
    /// yet, so this asks again until it answers; only then can "no registry"
    /// mean anything.
    /// Whether version text `have` ("0.11.2") is at least `need`.
    function versionAtLeast(have, need) {
        if (have === "")
            return false
        var a = have.split("."), b = need.split(".")
        for (var i = 0; i < 3; i++) {
            var x = parseInt(a[i] || "0", 10), y = parseInt(b[i] || "0", 10)
            if (isNaN(x))
                return false
            if (x !== y)
                return x > y
        }
        return true
    }

    function loadConfig() {
        function take(config) {
            if (!config || typeof config !== "object" || !("sequencer" in config))
                return false
            // It answered: whatever it says, the module is reachable.
            configAskSince = 0
            if (notice === moduleUnreachable)
                notice = ""
            var loaded = config.module_version ? String(config.module_version) : ""
            buildCommit = config.build_commit && config.build_commit !== "dev" ? String(config.build_commit) : ""
            moduleFeatures = Array.isArray(config.features) ? config.features : []
            readBridgeNote()
            readWhatsNew()
            machineMemory = Number(config.memory_bytes) || 0
            // Every 0.17.0 says 0.17.0: the reveal driver needs a build that has `reveal_name`.
            if (versionAtLeast(loaded, neededModule) && !(Array.isArray(config.features) && config.features.indexOf("reveal_name") >= 0))
                loaded = loaded + " (an earlier build)"
            if (!versionAtLeast(loaded, neededModule) || loaded.indexOf("earlier build") >= 0) {
                moduleOutdated = loaded === "" ? "older than 0.11" : loaded
                say("The names module is out of date (" + moduleOutdated + "; this app needs " + neededModule + " or newer). In Basecamp, open Applications, click Names and press Install.", true)
                return true
            }
            moduleOutdated = ""
            namesNet = config.names_network && config.names_network.id === "lez-0.4" ? "0.4" : "0.3"
            registry = config.registry ? config.registry : ""
            sequencer = config.sequencer ? config.sequencer : ""
            zoneId = config.zone && config.zone.id ? config.zone.id : "home"
            zoneName = config.zone && config.zone.name ? config.zone.name : ""
            zoneHome = !config.zone || config.zone.home !== false
            loadZones()
            walletConfig = config.wallet_config ? config.wallet_config : ""
            walletStorage = config.wallet_storage ? config.wallet_storage : ""
            cliPath = config.cli ? config.cli : ""
            bridgePrograms = Array.isArray(config.bridge_programs) ? config.bridge_programs : []
            addBundledBridge()
            cliWallet = config.cli_wallet ? config.cli_wallet : ""
            configKnown = true
            settingsOpen = registry === ""
            savedMain = config.main_name ? config.main_name : ""
            tokenProgram = config.token_program ? config.token_program : ""
            bridgeKey = config.bridge_key ? config.bridge_key : ""
            bridgeNode = config.bridge_node && config.bridge_node !== defaultBridgeNode ? config.bridge_node : ""
            loadWallet(false)
            loadCliMine()
            loadCliBalance()
            loadCliStatus()
            loadClaims()
            // The Wallet opens first: its balances, every account, tokens and node.
            loadAccounts()
            loadTokens()
            loadWatched()
            loadChain()
            loadRunningJobs()
            loadAppGrants()
            loadInstalled(checkBundle)
            loadBackupStatus()
            return true
        }
        if (bridgeReady && logos.callModuleAsync) {
            if (configAskSince === 0)
                configAskSince = Date.now()
            logos.callModuleAsync("names", "get_config", [], function(reply) {
                if (!take(parsed(reply)))
                    configRetry.start()
            }, 20000)
            return
        }
        try {
            if (!take(call("names", "get_config", [])))
                configRetry.start()
        } catch (error) {
            configRetry.start()
        }
    }

    Timer {
        id: configRetry
        interval: 2000
        onTriggered: root.loadConfig()
    }

    // The names module never answering at all (Basecamp hasn't connected this window to it; seen
    // after an update on 15 Nov): after 20 s, one plain line instead of an empty wallet, and
    // asked again every 10 s until it answers.
    readonly property string moduleUnreachable: "Names can't reach its module. Settings → Module Inspector → Names → Unload All, then close the Names tab and open Names again. If it still can't, restart Basecamp."
    property double configAskSince: 0
    Timer {
        interval: 10000
        repeat: true
        running: root.bridgeReady && root.configAskSince > 0
        onTriggered: {
            if (Date.now() - root.configAskSince < 20000)
                return
            root.say(root.moduleUnreachable, true)
            root.loadConfig()
        }
    }

    /// Saves the registry. Sequencers are set per zone (Settings, Zones).
    function saveConfig(newRegistry) {
        try {
            if (newRegistry !== registry && !call("names", "set_registry", [newRegistry]))
                say("That is not a program ID.", true)
        } catch (error) {
            say(error.message, true)
        }
        loadConfig()
    }

    // ── Zones (LP-0022) ─────────────────────────────────────────────────
    // A zone is one LEZ chain, known by its channel, with its own sequencer and its
    // own wallet. Names are kept on the first, Home; one zone is in use at a time.

    property var zones: []                // zone_list: Home first
    property string zoneId: "home"        // the zone in use
    property string zoneName: ""          // what the person calls it
    property bool zoneHome: true
    property int zoneMax: 16
    property bool zoneBusy: false         // a zone change is with the module
    property string zoneNote: ""          // what the last change said
    property bool zoneNoteBad: false
    property string zoneOpen: ""          // the zone whose row is open for changes
    property bool zoneRemoving: false     // "Remove" was pressed on the open row and asks to confirm
    property bool zoneAdding: false
    property var zoneOtherChain: null     // Home's new sequencer serves another chain: {url, channel}
    readonly property bool zonesMany: zones.length > 1
    readonly property string homeName: zones.length > 0 ? zones[0].name : "Home"

    function takeZones(view) {
        if (!view || !Array.isArray(view.zones))
            return
        zones = view.zones
        zoneMax = view.max || 16
        var used = zones.filter(function(zone) { return zone.current })[0]
        if (used) {
            zoneId = used.id
            zoneName = used.name
            zoneHome = !!used.home
        }
    }

    function loadZones() {
        askLater("zone_list", [], takeZones, function(message) {
            console.log("names_ui: zone_list:", message)
            if (waitAgain(message))
                againWhenFree("zones", loadZones)
        }, 20000)
    }

    /// One change to the zones: the list it answers with is shown, then `done(value)`.
    function zoneAsk(method, args, done) {
        if (zoneBusy)
            return
        zoneBusy = true
        zoneNote = ""
        zoneNoteBad = false
        askLater(method, args, function(value) {
            zoneBusy = false
            takeZones(value)
            done(value)
        }, function(message) {
            zoneBusy = false
            zoneNote = sentence(message)
            zoneNoteBad = true
        }, 40000)
    }

    function zoneCalled(id) {
        var found = zones.filter(function(zone) { return zone.id === id })[0]
        return found ? found.name : "the zone"
    }

    function zoneAdd(name, url) {
        zoneAsk("zone_add", [name.trim(), url.trim()], function(value) {
            zoneAdding = false
            zoneOtherChain = null
            zoneNote = "Added " + zoneCalled(value.added) + ". It has a wallet folder of its own: use it to make its wallet."
        })
    }

    /// Uses zone `id`: every balance, account and send is that zone's from here on.
    function zoneUse(id) {
        zoneAsk("zone_use", [id], function() {
            zoneOpen = ""
            zoneNote = "Using " + zoneName + "."
            // Nothing read on the zone left is shown as this one's.
            cliBalance = undefined
            cliAccounts = null
            cliStatus = null
            tokenHoldings = null
            walletAccount = null
            if (walletView === "account")
                walletView = "home"
            loadConfig()
            loadAppRequests()
        })
    }

    function zoneRemove(id) {
        zoneAsk("zone_remove", [id], function(value) {
            zoneOpen = ""
            zoneRemoving = false
            var gone = value.removed || {}
            zoneNote = "Removed " + (gone.name || "the zone") + ". Its wallet folder was kept" + (gone.wallet_dir ? " (" + gone.wallet_dir + ")" : "") + ": adding the zone again finds it."
            loadActivity()
        })
    }

    function zoneRename(id, name) {
        zoneAsk("zone_rename", [id, name.trim()], function() {
            zoneNote = "Renamed."
            if (!zoneHome || id === "home")
                loadConfig()
        })
    }

    /// Points zone `id` at another sequencer of the same chain. Home may become another
    /// chain, only once the person has said so (`otherChainOk`).
    function zoneSequencer(id, url, otherChainOk) {
        url = url.trim()
        zoneAsk("zone_sequencer", [id, url, !!otherChainOk], function(value) {
            if (value.changed === false && value.other_chain) {
                zoneOtherChain = { url: url, channel: value.channel || "" }
                return
            }
            zoneOtherChain = null
            zoneNote = "Sequencer changed. The wallet, its accounts and its history are as they were."
            loadConfig()
        })
    }

    /// The zone a request or plan is for, as a sheet shows it.
    function zoneFact(what) {
        if (!what || what.zone === undefined || what.zone === null)
            return ""
        var name = what.zone_name ? what.zone_name : "A zone removed since"
        var channel = what.zone_channel ? what.zone_channel : what.zone !== "home" ? what.zone : ""
        return name + (channel ? " · channel " + Logic.keyShown(channel) : "")
    }

    /// Reads the deposit a name would lock. Until it is known, registering waits.
    function loadDeposit(name) {
        deposit = null
        askLater("deposit", [name], function(value) {
            if (missing === name)
                deposit = value
        }, function(message) {
            console.log("names_ui: deposit:", message)
            if (waitAgain(message) && missing === name)
                againWhenFree("deposit", function() { if (missing === name) loadDeposit(name) })
        })
    }

    /// A time in milliseconds as a date.
    function dated(milliseconds) {
        return Qt.formatDate(new Date(milliseconds), "d MMMM yyyy")
    }

    /// Looks at the wallet again.
    function connectWallet() {
        loadWallet(true)
    }

    /// Names the wallet the names module should open by itself.
    function saveWalletFiles(configPath, storagePath) {
        try {
            if (!call("names", "set_wallet_files", [configPath, storagePath])) {
                say("One of those files does not exist.", true)
                return
            }
        } catch (error) {
            say(error.message, true)
            return
        }
        loadConfig()
        loadWallet(true)
    }

    /// Names the CLI the module runs for privately held names, and its wallet
    /// folder. Empty fields go back to the defaults.
    function saveCliSettings(path, folder) {
        try {
            if (path !== cliPath && !call("names", "set_cli", [path])) {
                say("That is not the full path of a program.", true)
                return
            }
            if (folder !== cliWallet && !call("names", "set_cli_wallet", [folder])) {
                say("That is not the full path of a folder.", true)
                return
            }
        } catch (error) {
            say(error.message, true)
            return
        }
        loadConfig()
        say("Using the names CLI at " + (path !== "" ? path : "its default place") + ".", false)
    }

    /// Reads the names the names CLI's wallet holds. They need no LEZ wallet.
    function loadCliMine() {
        if (registry === "") {
            cliNames = []
            cliPending = []
            return
        }
        askLater("cli_mine", [], onNet(function(value, reply) {
            value = value || []
            // The last list kept while reading it again fails says so (cli-mine-stale-list-unmarked-during-outage).
            namesNote = Logic.readNote(reply)
            cliNames = value.filter(function(entry) { return !entry.registering && !entry.failed })
            cliPending = value.filter(function(entry) { return entry.registering || entry.failed })
            cliKnown = true
            // Each name's profile: its picture, and the npk that says which account it pays into.
            cliNames.forEach(function(entry) { boardLookUp(entry.name) })
            if (savedMain === "" && mainName !== "")
                setMain(mainName)
        }), function(message) {
            console.log("names_ui: CLI names:", message)
            cliRetry.start()
        })
        askLater("anon_name", [], function(value) { anonName = value || "" }, function() {})
        askLater("board_likes", [], function(value) { boardLiked = value || [] }, function() {})
    }

    /// Asks how much the names CLI's private account holds, for registering.
    function loadCliBalance() {
        if (cliPath === "")
            return
        var shown = cliBalance
        cliBalance = undefined
        askLater("cli_balance", [], onNet(function(value, reply) {
            cliBalance = value
            // Balances the sequencer didn't give show as unknown, and this says which.
            balanceNote = Logic.readNote(reply)
            loadActivity()
        }), function(message) {
            console.log("names_ui: CLI balance:", message)
            // Busy keeps the balance shown (the 15 s poll asks again); anything else is said.
            if (waitAgain(message)) {
                cliBalance = shown === undefined ? null : shown
                return
            }
            cliBalance = null
            balanceNote = sentence(message)
        }, 120000)
    }

    property string balanceNote: ""   // what the last balance read said about itself: partial, or why it failed

    // ── Two networks: LEZ 0.3 (public Logos testnet) and LEZ 0.4 (bridge test network) ──

    /// The names network the module is on: "0.3" (LEZ 0.3, the public Logos testnet, the
    /// default) or "0.4" (LEZ 0.4, the bridge test network). Names are looked up, registered
    /// and paid there, with that network's own CLI and wallet (set_names_network).
    property string namesNet: "0.3"
    /// Which network the Wallet shows: always the names network, so a balance, a name and a
    /// payment on one screen are on the same chain. One switch for both.
    readonly property string walletNet: namesNet
    property bool netSwitching: false
    property string netProblem: ""
    /// The public accounts LEZ 0.3's wallet listed before a switch to LEZ 0.4, so the LEZ 0.4
    /// balances still show what they hold there (LEZ 0.4's own wallet may have none yet).
    property var accounts03: []
    property var bridgeNet: null          // bridge_net_balances, or null until it answers
    property string bridgeNetProblem: ""  // why the module couldn't be asked
    readonly property var bridgeNetAccounts: Logic.mergedAccounts(Logic.publicAccounts(cliBalance, cliAccounts), namesNet === "0.4" ? accounts03 : [])
    // A newly read account list is asked about at once (the module answers from memory).
    onBridgeNetAccountsChanged: if (walletNet === "0.4") loadBridgeNet()
    readonly property var bridgeNetStatusNow: Logic.bridgeNetStatus(bridgeNet, clockShown)
    // On LEZ 0.4 the rows know which accounts are the 0.4 wallet's own: a LEZ 0.3 one is view only.
    readonly property var bridgeNetBlocks: Logic.bridgeNetRows(bridgeNet ? bridgeNet.read : null, namesNet === "0.4" ? Logic.publicAccounts(cliBalance, cliAccounts) : null)
    property bool bridgeNetShowEmpty: false   // the wallet's accounts with nothing there, shown one by one
    readonly property var bridgeNetView: Logic.bridgeNetShown(bridgeNetBlocks, bridgeNetShowEmpty, receiveKeepAccounts)

    /// A time of day for a reading taken at `seconds`.
    function clockShown(seconds) {
        return Qt.formatTime(new Date(Number(seconds) * 1000), "hh:mm")
    }

    /// Asks the module what the public accounts hold on the bridge test network. It answers
    /// at once with its last read and reads again itself, at most every 30 s.
    function loadBridgeNet() {
        askLater("bridge_net_balances", [JSON.stringify(bridgeNetAccounts)], onNet(function(value) {
            bridgeNet = value
            bridgeNetProblem = ""
            if (value && value.updating)
                bridgeNetSoon.restart()
        }), function(message) {
            bridgeNetProblem = sentence(readFailed(message))
        })
    }

    property string bridgeNetWatchProblem: ""

    // ── Claim a bridge deposit: names_ui calls the lez_bridge module itself (rfp-022
    //    modules/NAMES-EMBED.md, Part A). The module proposes to the names module as the app
    //    "lez_bridge", and this app's own sheets answer it in place. The claim file's text is only
    //    ever in memory: never logged, never written. ──
    property string claimPaste: ""
    property var claimCheck: null          // relayed_claim_check's value
    property bool claimChecking: false
    property bool claimAsking: false       // relayed_claim on its way
    property var claimNeeds: null          // relayed_claim's {needs, request, message}: a sheet first
    property string claimNeedsState: ""
    property var claimUncertain: null      // relayed_claim's `uncertain`: a proposal got no answer
    property var depositClaimJob: null            // relayed_claim_status's value
    property string claimError: ""
    property bool claimAgain: true         // pressing Claim again is safe after claimError
    property bool claimMissing: false      // no LEZ Bridge package here
    property string claimNote: ""
    property var claimDeployment: null     // the bridge module's config().deployment
    property bool claimPolling: false      // one call at a time: the module answers one at a time
    property bool claimNeedsPolling: false
    property bool claimCheckWith: true     // the installed lez_bridge has relayed_claim_check_with (probed)
    // The claim as steps: the first-time questions expected (from the wallet's grants) and those
    // answered; the file checked, kept in memory only, to check again without picking it again;
    // an automatic check again when the refusal gives a time; Details open or not.
    property var claimExtras: []
    property var claimExtrasDone: []
    property string claimNeedsAfter: "claim"  // what an approved sheet leads to: the claim, or the check again
    property string claimSource: ""
    property double claimRetryAt: 0
    property double claimClock: Date.now()
    property bool claimDetails: false
    readonly property string claimRetryLeft: Logic.durationWords(Math.max(1, Math.round((claimRetryAt - claimClock) / 1000)))
    readonly property string claimSummaryNow: Logic.claimSummary(claimCheck, claimBridgeNow.label)
    // A claim line's kind: busy and no answer are waits (Logic.noticeKind), the rest problems.
    readonly property string claimErrorKind: Logic.noticeKind(claimError, true)
    // A refusal or failure's kind and its ONE next action (Logic.claimNext).
    readonly property var claimNextNow: Logic.claimNext(claimStageNow === "failed" && depositClaimJob !== null && depositClaimJob.error ? depositClaimJob.error.code
        : claimCheck !== null && claimCheck.error ? claimCheck.error.code : "", claimStageNow === "failed", claimSource !== "" && claimSource.indexOf("deposit:") !== 0)
    function claimNextAction(action) {
        if (action === "back")
            leaveClaim()
        else if (action === "another") {
            leaveClaim(); openClaim()
        } else if (action === "retry" && claimSource === "") {
            leaveClaim(); openClaim()
        } else
            checkAgain()
    }
    readonly property string claimRefusalLine: claimCheck !== null && claimCheck.ok === false && claimCheck.error ? Logic.claimErrorLine(claimCheck.error.code, claimCheck.error) : ""
    Timer {
        interval: 1000
        repeat: true
        // Only while the claim screen is open and in front; coming back, it checks when due.
        running: root.claimRetryAt > 0 && root.section === "wallet" && root.walletView === "claim" && root.receiveFrontmost
        onTriggered: {
            root.claimClock = Date.now()
            if (root.claimClock >= root.claimRetryAt) {
                root.claimRetryAt = 0
                root.checkAgain()
            }
        }
    }
    /// The same claim file checked again (it is still in memory): nothing to pick again.
    function checkAgain() {
        claimRetryAt = 0
        if (claimSource.indexOf("deposit:") === 0)
            checkDeposit(claimSource.substr(8))
        else if (claimSource !== "")
            checkClaimFile(claimSource, false)
    }
    property var bridgePrograms: []        // get_config bridge_programs: the bridges this wallet reads
    // The LEZ Bridge package, installed from here: what Basecamp has installed (package_manager,
    // read only; null until read for sure), whether the bridge module answered, whether the copy
    // shipped inside Names checked out ("ok" | "bad" | "" unchecked), and the install on its way.
    property var installedList: null
    property var claimBridgeAnswers: null
    property string bridgeBundle: ""
    property var bridgeInstall: null       // {phase: "asking"|"installing"|"slow"|"starting"|"failed"|"updated", line}
    readonly property string bridgeSetupNow: Logic.bridgeSetup({ list: installedList, answers: claimBridgeAnswers, checkWith: claimCheckWith, features: bridgeConfig !== null ? bridgeFeatures : null })
    readonly property var bridgeLabels: [["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "bridge v2"], ["4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr", "bridge v3"], [Logic.BRIDGE_V4_PROGRAM, "bridge v4"]]
    readonly property string claimAccount: claimCheck !== null && claimCheck.ok && claimCheck.deposit ? String(claimCheck.deposit.account || "") : ""
    readonly property string claimAccountProblemNow: cliAccounts === null ? "" : Logic.claimAccountProblem(claimAccount, cliAccounts)
    readonly property var claimBridgeNow: Logic.claimBridge(claimCheck !== null && claimCheck.call ? claimCheck.call.program : "", bridgePrograms, bridgeLabels)
    readonly property string claimStageNow: Logic.claimStage({ missing: claimMissing, looking: claimBridgeAnswers === null, job: depositClaimJob, uncertain: claimUncertain, needs: claimNeeds,
        asking: claimAsking, checking: claimChecking, check: claimCheck, accountProblem: claimAccountProblemNow, bridgeProblem: claimBridgeNow.problem })

    /// Calls the lez_bridge module. A read the module host wasn't ready for, or that met it busy
    /// with an earlier call, is sent again after 2, 4 and 6 s; nothing that proposes is
    /// (`retry` false). `failed(error, kind)`.
    function bridgeCall(method, args, done, failed, timeout, retry, attempt) {
        if (!bridgeReady || !logos.callModuleAsync) {
            if (failed) failed("This screen needs to run inside Basecamp.", "missing")
            return
        }
        var tries = attempt || 0
        logos.callModuleAsync("lez_bridge", method, args, function(reply) {
            var r = Logic.bridgeReply(reply)
            if (r.ok) {
                if (done) done(r.value)
                return
            }
            if (retry !== false && (r.kind === "starting" || r.kind === "busy") && tries < 3) {
                againAfter(2000 * (tries + 1), function() { bridgeCall(method, args, done, failed, timeout, retry, tries + 1) })
                return
            }
            if (failed) failed(r.error, r.kind)
        }, timeout || 120000)
    }
    /// A saved list of bridges without v4 (the bundled bridge's default since 0343cb0) gets it
    /// added, so its claims aren't refused and its tokens show; once per window, and never more
    /// than the module allows (4).
    property bool bridgeAddTried: false
    function addBundledBridge() {
        var list = Logic.bridgesToAdd(bridgePrograms)
        if (list === null || bridgeAddTried || !bridgeReady)
            return
        bridgeAddTried = true
        askLater("set_bridge_programs", [JSON.stringify(list)], function(value) {
            if (value === true || value === "true") {
                bridgePrograms = list
                console.log("names_ui: the wallet now reads the bridge", Logic.BRIDGE_V4_PROGRAM)
            } else
                console.log("names_ui: set_bridge_programs refused", JSON.stringify(value))
        }, function(message) { console.log("names_ui: set_bridge_programs:", message) }, 15000)
    }
    function againAfter(ms, fn) {
        var t = Qt.createQmlObject('import QtQuick; Timer { repeat: false }', root, "claimLater")
        t.interval = ms
        t.triggered.connect(function() { t.destroy(); fn() })
        t.start()
    }


    // ── Receive from Ethereum (rfp-022 modules/NAMES-RECEIVE.md): the bridge module makes the
    //    deposit's secret from this wallet's own values and gives back a code for the deposit page
    //    (no claim file); the card then follows each receive and offers its claim. The secret never
    //    reaches this app. ──
    property var receiveRegistry: null
    property var bridgeConfig: null        // the LEZ Bridge's config(): its features and notices (null until read)
    property var bridgeAnswers: null       // the LEZ Bridge module answered its last config call (null: not asked)
    readonly property var bridgeFeatures: bridgeConfig !== null && Array.isArray(bridgeConfig.features) ? bridgeConfig.features : []
    readonly property bool bridgeReceives: Logic.bridgeCan(bridgeFeatures, "receive") && Logic.bridgeCan(bridgeFeatures, "public_receive")
    property string bridgeUpdateThen: ""   // where the update goes on to: "receive", "claim" or ""
    // Config calls to an installed bridge that timed out in a row: two is "stuck" (a deadlocked
    // module answers nobody until it is unloaded).
    property int bridgeFails: 0
    property bool bridgeStale: false       // its last reply was to an old token: a reopen reconnects
    property bool bridgeNotLoaded: false   // its last config call found the module not loaded
    // "LEZ Bridge updated." once, on the screen the person lands on after a setup step finished
    // (bridgeDoneView), then gone at the next screen.
    property string bridgeDone: ""
    property string bridgeDoneView: ""
    // A setup step that waits on Basecamp or a reopen ("install" | "update" | ""): kept in the names
    // module (app_note) across the reopen, with where the person was going.
    property string bridgePending: ""
    property bool bridgeNoteRead: false
    // The one bridge setup state the claim screen, Receive and the card all show (Logic.bridgeSetupState).
    readonly property var bridgeSetup: Logic.bridgeSetupState({ list: installedList, bundle: bridgeBundle, answers: bridgeAnswers,
        stuck: bridgeFails >= 2, stale: bridgeStale, notLoaded: bridgeNotLoaded, features: bridgeFeatures, config: bridgeConfig, install: bridgeInstall,
        done: bridgeDone !== "" && walletView === bridgeDoneView ? bridgeDone : "" })
    onWalletViewChanged: {
        if (bridgeDone !== "" && walletView !== bridgeDoneView)
            bridgeDone = ""
        // A bridge panel or screen came into view: one check now (not again within 10 s), and
        // what is due by itself recomputed for this view.
        if (receiveWatching) {
            bridgeCheckOnView(true)
            receiveScheduleNext()
            sbScheduleNext()
        }
    }
    // The bridge needs a step first (Update, Unload, a reopen…): Receive's and Claim's own steps
    // wait under it, so nobody runs into a failure the setup line already explains.
    // An update (or its Unload step) for a bridge that answers and receives holds nothing up:
    // Receive and Claim go on with the running one; the update is offered beside them.
    readonly property bool bridgeSetupOptional: (bridgeSetup.state === "update" || bridgeSetup.state === "unload") && bridgeAnswers === true && bridgeReceives
    readonly property bool bridgeSetupClear: bridgeSetup.state === "" || bridgeSetup.state === "done" || bridgeSetupOptional
    // The setup's button is yellow only when nothing works without it (the bridge missing, stuck,
    // or unable to receive); an optional update is a plain button, everywhere.
    readonly property bool cardSetupBlocks: bridgeSetup.button !== "" && bridgeSetup.action !== "settings" && !bridgeSetupOptional
    function bridgeAnswered(value) {
        bridgeConfig = value
        bridgeAnswers = true
        bridgeFails = 0
        bridgeStale = false
        bridgeNotLoaded = false
    }
    /// A config call that failed: `kind` from Logic.bridgeReply ("busy" is Basecamp's timeout).
    function bridgeSilent(kind) {
        bridgeAnswers = false
        bridgeStale = kind === "stale"
        bridgeNotLoaded = kind === "missing" || kind === "starting"
        if (kind === "busy")
            bridgeFails = bridgeFails + 1
    }
    property bool bridgeRestartAsking: false
    readonly property var receiveTokenList: Logic.receiveTokens(receiveRegistry)
    property string receiveToken: "ETH"
    property int receiveDenom: 1
    property string receiveAccount: ""
    property bool receivePublicOk: false
    property bool receiveAsking: false
    property var receiveNeeds: null
    property string receiveNeedsState: ""
    property bool receiveNeedsPolling: false
    property var receiveExtras: []
    property var receiveResult: null
    property string receiveError: ""
    property bool receiveMissing: false
    property bool receiveDetails: false
    property var receives: []              // this wallet's receives: [{id, symbol, amount, account}]
    property var receiveStatuses: ({})     // receive_status(id) by id
    property bool receivesLoaded: false
    readonly property bool receiveHasStatus: bridgeConfig === null || (Logic.bridgeCan(bridgeFeatures, "receive_status") && Logic.bridgeCan(bridgeFeatures, "receives"))
    property bool receivePolling: false
    readonly property var receiveAccountList: Logic.publicAccounts(cliBalance, cliAccounts)
    // The account chosen, else the first public account (the one the card shows first).
    readonly property string receiveAccountNow: receiveAccountList.indexOf(receiveAccount) >= 0 ? receiveAccount : receiveAccountList.length > 0 ? receiveAccountList[0] : ""
    // The receive the Receive screen follows ("" while choosing a new one): the code just made,
    // one opened from the card, or the one on its way when the screen opens.
    property string receiveFocus: ""
    property var receiveCodes: ({})        // codes made in this window, by id (with made_at), to show again
    property var receiveCodesAgain: ({})   // codes of earlier sessions, from receive_request(id), by id
    property var receiveCodeRefusals: ({}) // receive_request(id) refused: id → {line, detail}
    // Why a new code can't be made with the running bridge yet ("" when it can).
    readonly property string receiveHoldNow: Logic.receiveHoldLine(bridgeConfig, bridgePrograms)
    property var receiveCodeExtras: ({})   // the first-time steps each of those asked, so its numbers stay
    property var receiveClaimedAt: ({})    // receives claimed while Names was open: id → ms
    property bool receiveShowAll: false    // the card's older unused codes, shown one by one
    // One check of every receive a minute, one call at a time (the bridge's notes), plus one when
    // the card or Receive opens, when Names comes back to the front, and on "Check now".
    property double receiveCheckedAt: 0
    property double receiveNextAt: 0       // the next check by itself (Logic.bridgeNextCheck), 0: none
    property bool receiveAskNow: false      // a check asked for (Check now, or the screen came into view)
    property bool receiveCheckFailed: false
    property double receiveNow: Date.now()
    // A config/registry call of Receive's own is under way: no check meanwhile (one call at a time).
    property bool receiveHold: false
    readonly property var receiveFocusRow: receiveFocus === "" ? null : (receives.filter(function(x) { return x.id === receiveFocus })[0] || null)
    readonly property var receiveFocusProgress: Logic.receiveProgress(receiveFocus === "" ? null : receiveStatuses[receiveFocus])
    readonly property string receiveCheckLine: Logic.receiveCheckLine(receivePolling, receiveCheckedAt, receiveNextAt, receiveNow, receiveCheckFailed)
    // The card's account blocks keep the accounts a receive is arriving in, or just arrived in.
    readonly property var receiveKeepAccounts: Logic.receiveRowsShown(receives, receiveStatuses, receiveClaimedAt, receiveNow, true).rows
        .filter(function(x) { return x.progress.stage !== "waiting" && x.progress.stage !== "checking" && x.progress.stage !== "lost" }).map(function(x) { return x.account })
    readonly property var receiveStepsNow: Logic.receiveSteps({
        stage: receiveFocus !== "" ? receiveFocusProgress.stage : receiveResult !== null ? "code" : receiveNeeds !== null ? "needs" : "choose",
        needs: receiveNeeds !== null ? receiveNeeds.needs : "", extras: receiveExtras,
        claimedBlock: receiveFocus !== "" && receiveStatuses[receiveFocus] ? receiveStatuses[receiveFocus].claimed_block : null })

    /// Opens Receive from Ethereum. `focusId`: a receive to show (a card row); without it, the one
    /// on its way if there is one (Logic.receiveInProgress), else the choice of a new code.
    function openEthReceive(focusId) {
        walletView = "ethreceive"
        loadInstalled(checkBundle)
        receiveError = ""
        receiveNeeds = null
        receiveDetails = false
        receivePublicOk = false
        receiveMissing = false
        receiveFocus = typeof focusId === "string" ? focusId : Logic.receiveInProgress(receives, receiveStatuses, receiveCodes)
        receiveResult = receiveFocus !== "" && receiveCodes[receiveFocus] ? receiveCodes[receiveFocus]
            : receiveFocus !== "" && receiveCodesAgain[receiveFocus] ? receiveCodesAgain[receiveFocus] : null
        receiveExtras = receiveFocus !== "" && receiveCodeExtras[receiveFocus] ? receiveCodeExtras[receiveFocus] : []
        if (receiveAccount === "" || receiveAccountList.indexOf(receiveAccount) < 0)
            receiveAccount = receiveAccountList.length > 0 ? receiveAccountList[0] : ""
        // Is the bridge module here (config answers at once), then its denominations (the
        // registry reads the chains; without it, the bridge test network's known list), then one
        // check of the receive shown. One call at a time.
        receiveHold = true
        bridgeCall("config", [], function(value) {
            bridgeAnswered(value)
            if (receiveFocus === "" && bridgeReceives)
                loadReceiveRegistry()
            else if (receiveFocus !== "" && receiveResult === null && Logic.bridgeCan(bridgeFeatures, "receive_request") && !receiveCodeRefusals[receiveFocus])
                loadReceiveCode(receiveFocus)
            else {
                receiveHold = false
                receiveCheckSoon()
            }
        }, function(message, kind) {
            receiveHold = false
            bridgeSilent(kind)
            if (kind === "missing" || kind === "starting" || kind === "busy" || kind === "stale")
                receiveMissing = true
        }, 10000, false)
    }
    /// The code of a receive made in an earlier session (`receive_request`: read-only, no wallet
    /// or chain call). Refused for a code an older bridge made: the screen says so, never a link.
    function loadReceiveCode(id) {
        receiveHold = true
        bridgeCall("receive_request", [id], function(value) {
            receiveHold = false
            if (value && typeof value.request === "string" && value.request.indexOf("logosdep1") === 0) {
                var again = receiveCodesAgain
                again[id] = value
                receiveCodesAgain = again
                receiveCodesAgainChanged()
                if (receiveFocus === id && receiveResult === null)
                    receiveResult = value
                console.log("names_ui: receive: code shown again for", id)
            }
            receiveCheckSoon()
        }, function(message, kind) {
            receiveHold = false
            if (kind !== "busy" && kind !== "missing" && kind !== "starting" && kind !== "stale") {
                var all = receiveCodeRefusals
                all[id] = Logic.receiveCodeRefused(message)
                receiveCodeRefusals = all
                receiveCodeRefusalsChanged()
            }
            console.log("names_ui: receive_request", id + ":", kind, message)
            receiveCheckSoon()
        }, 30000)
    }
    /// The denominations: read once per screen (it reads both chains), never polled.
    function loadReceiveRegistry() {
        receiveHold = true
        bridgeCall("registry", [], function(value) { receiveRegistry = value; receiveHold = false; receiveCheckSoon() }, function() { receiveHold = false; receiveCheckSoon() }, 60000, false)
    }
    /// "Make another code": the choice of amount and account, whatever is on its way.
    function startNewReceive() {
        receiveFocus = ""
        receiveResult = null
        receiveExtras = []
        receiveError = ""
        receiveNeeds = null
        receivePublicOk = false
        receiveDetails = false
        if (receiveRegistry === null && bridgeReceives && !receivePolling)
            loadReceiveRegistry()
    }
    /// One check at the next tick (a panel or screen came into view), unless one finished in the
    /// last 10 s.
    function receiveCheckSoon() {
        if (Date.now() - receiveCheckedAt > checkOnViewMs)
            receiveAskNow = true
    }
    // A panel coming into view again checks only if the last check is older than this (Check now
    // always checks): switching between the browser and Basecamp doesn't cost a round each time.
    readonly property int checkOnViewMs: 60000
    // The bridge still starting or busy when a list is first read: asked again a few times, 15 s
    // apart, so a panel and its Check now appear without the person leaving and coming back.
    property int bridgeListRetries: 0
    function bridgeListRetry(kind) {
        if ((kind === "starting" || kind === "busy" || kind === "missing") && bridgeListRetries < 4) {
            bridgeListRetries++
            return Date.now() + 15000
        }
        return 0
    }
    /// "Check now": one call for the receive on the Receive screen, a round on the card; asked
    /// at once, or as soon as the call under way has answered.
    function receiveCheckNow() {
        receiveAskNow = true
        if (!receivePolling && !receiveHold)
            pollReceives(walletView === "ethreceive" ? receiveFocus : "")
    }
    /// The next receive check by itself: only a deposit becoming final, on the Receive screen, in
    /// front (Logic.bridgeNextCheck); otherwise none: Check now, and once on view.
    function receiveScheduleNext() {
        var onScreen = walletView === "ethreceive"
        var items = onScreen && receiveFocus !== "" ? [{ mode: Logic.receiveAutoMode(receiveStatuses[receiveFocus]), focused: true }] : []
        receiveNextAt = Logic.bridgeNextCheck({ now: Date.now(), visible: receiveWatching, frontmost: receiveFrontmost, screen: onScreen ? "receive" : "card", items: items })
    }

    /// The deposit code for the chosen amount and account. A public account only after the person
    /// has read the "not private" line (the module refuses it without "true").
    function makeReceive(switched) {
        if (receiveAsking || receiveAccountNow === "" || !receivePublicOk || receiveHoldNow !== "")
            return
        receiveAccount = receiveAccountNow
        receiveAsking = true
        receiveError = ""
        // New codes are made on the bundled bridge's default (v4): a claim file for another vault
        // may have switched the bridge, so it is switched back first (once).
        var dep = bridgeConfig && bridgeConfig.deployment ? String(bridgeConfig.deployment.name || "") : ""
        if (dep !== "" && dep !== Logic.RECEIVE_DEPLOYMENT) {
            if (switched === true) {
                receiveAsking = false
                receiveError = "The LEZ Bridge stays on " + dep + ", not the new vault. Nothing was made."
                return
            }
            bridgeCall("use_deployment", [Logic.RECEIVE_DEPLOYMENT], function(config) {
                receiveAsking = false
                var next = Object.assign({}, bridgeConfig)
                next.deployment = config && config.deployment ? config.deployment : { name: Logic.RECEIVE_DEPLOYMENT }
                bridgeConfig = next
                console.log("names_ui: receive: the bridge switched from", dep, "to", next.deployment.name)
                makeReceive(true)
            }, function(message, kind) {
                receiveAsking = false
                receiveError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Try again." : message)
                console.log("names_ui: receive: use_deployment:", kind, message)
            }, 30000, false)
            return
        }
        bridgeCall("make_public_deposit_request", [receiveToken, String(receiveDenom), receiveAccount, "true"], function(value) {
            receiveAsking = false
            var r = Logic.receiveReply(value)
            if (r.kind === "needs" && Logic.needsWithoutRequest(value) !== "") {
                receiveError = sentence(Logic.needsWithoutRequest(value))
                console.log("names_ui: receive:", receiveError)
                return
            }
            if (r.kind === "needs") {
                var key = Logic.needsStep(value.needs)
                if (receiveExtras.indexOf(key) < 0)
                    receiveExtras = receiveExtras.concat([key])
                receiveNeeds = value
                receiveNeedsState = value.request && value.request.state ? value.request.state : "pending"
                return
            }
            if (r.kind === "old") {
                receiveError = "This LEZ Bridge can't make deposit codes yet. Update it."
                return
            }
            receiveResult = value
            if (value.deposit && value.deposit.id) {
                var id = String(value.deposit.id)
                if (!receives.some(function(x) { return x.id === id }))
                    receives = [{ id: id, symbol: String(value.symbol || ""), amount: String(value.denom_text || ""), account: receiveAccount }].concat(receives)
                // Followed from here on: kept for this window, so the screen can show it again.
                var codes = receiveCodes
                codes[id] = Object.assign({ made_at: Date.now() }, value)
                receiveCodes = codes
                receiveCodesChanged()
                var extras = receiveCodeExtras
                extras[id] = receiveExtras.slice()
                receiveCodeExtras = extras
                receiveCodeExtrasChanged()
                // Nothing can be on Ethereum yet: checked when the person comes back from the page.
                if (!receiveStatuses[id]) {
                    var all = receiveStatuses
                    all[id] = { stage: "requested" }
                    receiveStatuses = all
                    receiveStatusesChanged()
                }
                receiveFocus = id
                // Not checked yet (not an earlier check of another code); none by itself.
                receiveCheckedAt = 0
                receiveCheckFailed = false
                receiveNextAt = 0
                console.log("names_ui: receive: code made for", id)
            }
        }, function(message, kind) {
            receiveAsking = false
            if (kind === "missing")
                receiveMissing = true
            else
                receiveError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Try again." : message)
            console.log("names_ui: receive:", kind, message)
        }, 120000, false)
    }
    function pollReceiveNeeds() {
        var id = receiveNeeds && receiveNeeds.request ? String(receiveNeeds.request.request || "") : ""
        if (id === "" || receiveNeedsPolling)
            return
        receiveNeedsPolling = true
        bridgeCall("wallet_request", [id], function(value) {
            receiveNeedsPolling = false
            receiveNeedsState = value && value.state ? value.state : ""
            if (receiveNeedsState === "approved" || receiveNeedsState === "done") {
                receiveNeeds = null
                makeReceive()
            } else if (receiveNeedsState === "rejected" || receiveNeedsState === "expired" || receiveNeedsState === "cancelled") {
                receiveNeeds = null
                receiveError = Logic.approvalEndedLine(receiveNeedsState)
            }
        }, function() { receiveNeedsPolling = false }, 120000)
    }
    Timer {
        interval: 2500
        repeat: true
        running: root.receiveNeeds !== null && (root.receiveNeedsState === "" || root.receiveNeedsState === "pending")
        onTriggered: root.pollReceiveNeeds()
    }

    /// The card's receives: the bridge's own list once, then receive_status of each, one call at
    /// a time, every 60 s while the LEZ 0.4 wallet shows (it never asks Names anything). `onlyId`:
    /// just that receive (the Receive screen's); else every open one, the screen's first.
    function pollReceives(onlyId) {
        if (receivePolling || receiveHold || !bridgeReady || receiveAsking || receiveNeeds !== null || sbPolling || sbAsking || sbNeeds !== null)
            return
        // A round before the list is read (the first, or a retry of a refused first read) checks
        // everything, as if asked.
        var asked = receiveAskNow || !receivesLoaded || bridgeConfig === null
        receiveAskNow = false
        if (bridgeConfig === null) {
            receivePolling = true
            bridgeCall("config", [], function(value) {
                receivePolling = false
                bridgeAnswered(value)
                // The same round goes on: what was asked stays asked.
                receiveAskNow = asked
                if (receiveHasStatus)
                    pollReceives(onlyId)
            }, function(message, kind) { receivePolling = false; bridgeSilent(kind); receiveCheckFailed = true; receiveNextAt = bridgeListRetry(kind) }, 10000, false)
            return
        }
        if (!receiveHasStatus) {
            receiveNextAt = 0
            return
        }
        if (!receivesLoaded) {
            // The list (no chain read): what receive_status last saw for each, newest first.
            receivePolling = true
            bridgeCall("receives", [], function(value) {
                receivePolling = false
                receivesLoaded = true
                bridgeListRetries = 0
                var found = Logic.receivesFrom(value)
                receives = found.concat(receives.filter(function(x) { return !found.some(function(f) { return f.id === x.id }) }))
                var all = receiveStatuses
                found.forEach(function(x) { if (!all[x.id] && x.stage !== "unknown") all[x.id] = { stage: x.stage } })
                receiveStatuses = all
                receiveStatusesChanged()
                receiveAskNow = asked
                pollReceives(onlyId)
            }, function(message, kind) {
                receivePolling = false
                receiveCheckFailed = true
                receiveNextAt = bridgeListRetry(kind)
                console.log("names_ui: receives:", kind, message)
            }, 30000, false)
            return
        }
        var open = receives.filter(function(x) { var st = receiveStatuses[x.id]; return (!st || st.stage !== "claimed") && (!onlyId || x.id === onlyId) })
        // Due by itself (not asked): only what checks by itself.
        if (!asked)
            open = open.filter(function(x) { return Logic.receiveAutoMode(receiveStatuses[x.id]) !== "none" })
        if (receiveFocus !== "")
            open = open.filter(function(x) { return x.id === receiveFocus }).concat(open.filter(function(x) { return x.id !== receiveFocus }))
        if (open.length === 0) {
            receiveCheckedAt = Date.now()
            receiveNextAt = 0
            return
        }
        receivePolling = true
        var failedAny = false
        var next = function(i) {
            if (i >= open.length) {
                receivePolling = false
                receiveCheckedAt = Date.now()
                receiveCheckFailed = failedAny
                receiveScheduleNext()
                return
            }
            var id = open[i].id
            bridgeCall("receive_status", [id], function(value) {
                var all = receiveStatuses
                var was = all[id] ? all[id].stage : ""
                all[id] = value
                receiveStatuses = all
                receiveStatusesChanged()
                if (value && value.stage !== was)
                    console.log("names_ui: receive", id + ":", value.stage)
                // Claimed (from here or elsewhere): the row says so for a while, and the account's
                // balance is read again so it shows.
                if (value && value.stage === "claimed") {
                    var at = receiveClaimedAt
                    if (!at[id]) {
                        at[id] = Date.now()
                        receiveClaimedAt = at
                        receiveClaimedAtChanged()
                    }
                    loadBridgeNet()
                }
                next(i + 1)
            }, function(message, kind) {
                // The endpoint's "too many requests" after the bridge's own retries: a wait.
                if (!Logic.rateLimited(message))
                    failedAny = true
                console.log("names_ui: receive_status", id + ":", kind, message)
                next(i + 1)
            }, 60000, false)
        }
        next(0)
    }
    // When the bridge things are checked (the owner, 15 Nov: "put a check now button so if they
    // ignore it, we ignore it"): when the person asks (Check now); once when the card, Receive or
    // Bridge to Ethereum comes into view or Basecamp comes back to the front; and by itself only as
    // Logic.bridgeNextCheck says (a job in progress every 5 s while shown; on its own open screen,
    // in front, a deposit becoming final every 2.5 min, a releasable time once when it passes).
    // Nothing while the person is elsewhere.
    readonly property bool receiveWatching: bridgeReady && namesNet === "0.4" && section === "wallet"
        && (walletView === "home" || walletView === "ethreceive" || walletView === "sendback")
    /// One check because a bridge panel or screen came into view: always when the person opened
    /// it (`opened`), else (Basecamp back in front) only if the last one is a minute old.
    function bridgeCheckOnView(opened) {
        if (opened || Date.now() - receiveCheckedAt > checkOnViewMs)
            receiveAskNow = true
        if (opened || Date.now() - sbCheckedAt > checkOnViewMs)
            sbAskNow = true
    }
    onReceiveWatchingChanged: if (receiveWatching) bridgeCheckOnView(true); else { receiveNextAt = 0; sbNextAt = 0 }
    readonly property bool receiveFrontmost: Qt.application.state === Qt.ApplicationActive
    onReceiveFrontmostChanged: {
        if (!receiveWatching)
            return
        if (receiveFrontmost)
            bridgeCheckOnView(false)
        else {
            // Away: only a job in progress goes on.
            receiveAskNow = false
            sbAskNow = false
            receiveScheduleNext()
            sbScheduleNext()
        }
    }
    // The clock: a tick a second while something is due or shown in front (it moves the "Checked
    // N s ago" line); a check only when asked or due.
    Timer {
        interval: 1000
        repeat: true
        running: root.receiveWatching && (root.receiveFrontmost || root.receiveNextAt > 0 || root.sbNextAt > 0 || root.receiveAskNow || root.sbAskNow)
        onTriggered: {
            root.receiveNow = Date.now()
            var receiveDue = root.receiveAskNow || (root.receiveNextAt > 0 && root.receiveNow >= root.receiveNextAt)
            var sbDue = root.sbAskNow || (root.sbNextAt > 0 && root.receiveNow >= root.sbNextAt)
            if (receiveDue && root.walletView !== "sendback")
                root.pollReceives(root.walletView === "ethreceive" ? root.receiveFocus : "")
            else if (sbDue && root.walletView !== "ethreceive")
                root.pollSendBacks()
        }
    }

    /// A receive claimed here: claimed in `block` (or unknown), shown so for a while.
    function receiveClaimed(id, block) {
        if (id === "")
            return
        var all = receiveStatuses
        all[id] = Object.assign({}, all[id] || {}, { stage: "claimed", claimed_block: block || (all[id] ? all[id].claimed_block : null) })
        receiveStatuses = all
        receiveStatusesChanged()
        var at = receiveClaimedAt
        at[id] = Date.now()
        receiveClaimedAt = at
        receiveClaimedAtChanged()
        console.log("names_ui: receive", id + ": claimed", block ? "in block " + block : "")
    }

    // ── Send back to Ethereum (rfp-022 modules/NAMES-SEND-BACK.md): wrapped tokens in a public
    // account on bridge v4 burned (the wallet signs, the bridge's relay pays), then released on
    // Ethereum to the address the person connects on the bridge page. The bridge module keeps the
    // send-back; this screen asks it and says what it answers. One bridge call at a time. ──
    property string sbAccount: ""          // the public account the tokens leave
    property var sbAsset: null             // its card asset row (bridgeNetRows)
    property int sbDenom: -1               // the denomination chosen
    property var sbLink: null              // send_back_link()
    property bool sbLinkCopied: false
    property string sbCodeInput: ""
    property var sbCheck: null             // send_back_check(code), with `code`
    property string sbCheckError: ""
    property bool sbAsking: false
    property var sbNeeds: null             // the bridge's first-time question (connect, programs, account, keys)
    property string sbNeedsState: ""
    property bool sbNeedsPolling: false
    property string sbNeedsThen: ""        // what to ask again once it is answered: "plan" | "burn"
    property var sbExtras: []
    property string sbError: ""
    property string sbFocus: ""            // the send-back on the screen
    property var sbStatuses: ({})          // id → send_back_status
    property var sendBacks: []             // Logic.sendBacksFrom rows
    property bool sendBacksLoaded: false
    property bool sbPolling: false
    property double sbNextAt: 0            // the next check by itself (Logic.bridgeNextCheck), 0: none
    property bool sbAskNow: false
    property double sbCheckedAt: 0
    property bool sbCheckFailed: false
    property var sbReceivedAt: ({})        // id → when it was seen released, for the card
    property var sbWitness: null           // send_back_witness(): today's proof command
    property string sbSealInput: ""
    property var sbRelease: null           // send_back_release_code()
    property bool sbDetails: false
    property bool sbNamesOld: false        // send_back_burn said this Names Wallet can't sign a burn
    property bool sbCommandCopied: false
    property bool sbProverTried: false     // set_release_prover tried once in this window
    readonly property bool sbCan: Logic.bridgeCan(bridgeFeatures, "send_back")
    readonly property var sbAmounts: Logic.sendBackAmounts(receiveRegistry, sbAsset)
    readonly property var sbSources: Logic.sendBackSources(bridgeNetBlocks)
    /// Picks the account and token Send back starts from (a source row), or none.
    function sbPick(source) {
        sbAccount = source ? source.account : ""
        sbAsset = source ? source.asset : null
        sbDenom = -1
        if (sbAmounts.length === 1)
            sbDenom = sbAmounts[0].index
    }
    readonly property var sbFocusRow: sbFocus !== "" ? (sendBacks.filter(function(x) { return x.id === sbFocus })[0] || null) : null
    readonly property var sbFocusStatus: sbFocus !== "" ? (sbStatuses[sbFocus] || (sbFocusRow !== null ? { stage: sbFocusRow.stage } : null)) : null
    readonly property var sbProgressNow: Logic.sendBackProgress(sbFocusStatus, receiveNow / 1000)
    readonly property var sbStepsNow: Logic.sendBackSteps({
        stage: sbFocus !== "" ? (sbProgressNow.stage === "checking" ? "planned" : sbProgressNow.stage) : sbNeeds !== null ? "needs" : sbDenom >= 0 ? "address" : "choose",
        needs: sbNeeds !== null ? sbNeeds.needs : "", extras: sbExtras })
    readonly property var sbRowsNow: Logic.sendBackRowsShown(sendBacks, sbStatuses, sbReceivedAt, receiveNow)
    // "next check in N min" only for a gentle check of the send-back on screen (never a known time).
    readonly property double sbNextShown: walletView === "sendback" && sbFocusStatus !== null && Logic.sendBackAutoMode(sbFocusStatus).mode === "gentle" ? sbNextAt : 0

    /// Opens Send back to Ethereum for `asset` (a card asset row) of `account`, or the send-back
    /// `focusId` (a card row).
    function openSendBack(account, asset, focusId) {
        walletView = "sendback"
        sbError = ""
        sbNeeds = null
        sbCheck = null
        sbCheckError = ""
        sbCodeInput = ""
        sbWitness = null
        sbRelease = null
        sbSealInput = ""
        sbDetails = false
        sbNamesOld = false
        sbExtras = []
        sbAccount = account || ""
        sbAsset = asset || null
        sbDenom = -1
        sbFocus = typeof focusId === "string" ? focusId : ""
        if (sbFocus !== "" && sbFocusRow !== null)
            sbAccount = sbFocusRow.account
        // Opened from the card's button: one account to send from is chosen for the person.
        if (sbFocus === "" && sbAsset === null && sbSources.length === 1)
            sbPick(sbSources[0])
        loadInstalled(checkBundle)
        receiveHold = true
        bridgeCall("config", [], function(value) {
            bridgeAnswered(value)
            bridgeCall("send_back_link", [], function(link) {
                sbLink = link
                receiveHold = false
                if (sbFocus === "" && receiveRegistry === null)
                    loadReceiveRegistry()
                else
                    sbAskNow = true
                if (sbFocus === "" && sbAsset !== null && sbAmounts.length === 1 && sbDenom < 0)
                    sbDenom = sbAmounts[0].index
            }, function(message, kind) {
                receiveHold = false
                sbError = kind === "missing" ? "" : sentence(message)
                console.log("names_ui: send back: send_back_link:", kind, message)
            }, 30000, false)
        }, function(message, kind) {
            receiveHold = false
            bridgeSilent(kind)
            if (kind === "missing" || kind === "starting" || kind === "busy" || kind === "stale")
                receiveMissing = true
        }, 10000, false)
    }
    function sbCopyLink() {
        if (sbLink === null || !sbLink.link)
            return
        copy(String(sbLink.link), "The link")
        sbLinkCopied = true
        console.log("names_ui: send back: the withdraw page link copied")
        againAfter(2500, function() { sbLinkCopied = false })
    }
    /// The pasted withdraw-to code: what it says (address and four words), read by the bridge.
    function sbCheckCode() {
        var code = Logic.sendBackCode(sbCodeInput)
        sbCheck = null
        sbCheckError = ""
        if (code === "") {
            sbCheckError = "Paste the code the bridge page shows: it starts logoswd1."
            return
        }
        sbAsking = true
        bridgeCall("send_back_check", [code], function(value) {
            sbAsking = false
            if (value && value.ok === true)
                sbCheck = Object.assign({ code: code }, value)
            else {
                var e = value && value.error ? value.error : {}
                sbCheckError = e.message ? sentence(String(e.message)) : Logic.sendBackCodeTitle(e.code) + "."
            }
        }, function(message, kind) {
            sbAsking = false
            sbCheckError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Try again." : message)
        }, 60000, false)
    }
    /// A first-time question from the bridge (connect, programs, account, keys): drawn in the page
    /// (appInline), and asked again once answered.
    function sbNeedsReply(value, then) {
        if (Logic.needsWithoutRequest(value) !== "") {
            sbError = sentence(Logic.needsWithoutRequest(value))
            return
        }
        var key = Logic.needsStep(value.needs)
        if (sbExtras.indexOf(key) < 0)
            sbExtras = sbExtras.concat([key])
        sbNeeds = value
        sbNeedsThen = then
        sbNeedsState = value.request && value.request.state ? value.request.state : "pending"
    }
    /// "Send back": the exit is made (the wallet's keys), then the burn is proposed to this wallet;
    /// its approval shows here.
    function sbPlan() {
        if (sbAsking || sbCheck === null || sbDenom < 0 || sbAsset === null)
            return
        sbAsking = true
        sbError = ""
        bridgeCall("send_back_plan", [Logic.sendBackSymbol(sbAsset.symbol), String(sbDenom), sbAccount, sbCheck.code], function(value) {
            sbAsking = false
            if (value && value.needs) {
                sbNeedsReply(value, "plan")
                return
            }
            if (value && value.ok === false) {
                var e = value.error || {}
                sbError = e.message ? sentence(String(e.message)) : Logic.sendBackCodeTitle(e.code) + "."
                return
            }
            if (!value || !value.id) {
                sbError = "The LEZ Bridge gave no answer to bridge it. Nothing was done."
                return
            }
            var id = String(value.id)
            var row = { id: id, amount: String(value.amount_text || ""), account: String(value.account || sbAccount), recipient: String(value.recipient || ""), words: String(value.words_text || ""), stage: String(value.stage || "planned") }
            sendBacks = [row].concat(sendBacks.filter(function(x) { return x.id !== id }))
            var all = sbStatuses
            all[id] = value
            sbStatuses = all
            sbStatusesChanged()
            sbFocus = id
            console.log("names_ui: send back: planned", id)
            sbBurn()
        }, function(message, kind) {
            sbAsking = false
            sbError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Try again." : message)
            console.log("names_ui: send back: send_back_plan:", kind, message)
        }, 180000, false)
    }
    function sbBurn() {
        if (sbAsking || sbFocus === "")
            return
        sbAsking = true
        sbError = ""
        var id = sbFocus
        bridgeCall("send_back_burn", [id, "{}"], function(value) {
            sbAsking = false
            if (value && value.needs === "names_update") {
                sbNamesOld = true
                sbError = Logic.sendBackBurnHeld(value)
                console.log("names_ui: send back: names_update:", value.detail || "")
                return
            }
            if (value && value.needs) {
                sbNeedsReply(value, "burn")
                return
            }
            var all = sbStatuses
            all[id] = value
            sbStatuses = all
            sbStatusesChanged()
            sbScheduleNext()
            console.log("names_ui: send back:", id, value && value.stage)
        }, function(message, kind) {
            sbAsking = false
            sbError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Try again." : message)
            console.log("names_ui: send back: send_back_burn:", kind, message)
        }, 240000, false)
    }
    function pollSbNeeds() {
        var id = sbNeeds && sbNeeds.request ? String(sbNeeds.request.request || "") : ""
        if (id === "" || sbNeedsPolling)
            return
        sbNeedsPolling = true
        bridgeCall("wallet_request", [id], function(value) {
            sbNeedsPolling = false
            sbNeedsState = value && value.state ? value.state : ""
            if (sbNeedsState === "approved" || sbNeedsState === "done") {
                sbNeeds = null
                if (sbNeedsThen === "plan")
                    sbPlan()
                else
                    sbBurn()
            } else if (sbNeedsState === "rejected" || sbNeedsState === "expired" || sbNeedsState === "cancelled") {
                sbNeeds = null
                sbError = Logic.approvalEndedLine(sbNeedsState)
            }
        }, function() { sbNeedsPolling = false }, 120000)
    }
    Timer {
        interval: 2500
        repeat: true
        running: root.sbNeeds !== null && (root.sbNeedsState === "" || root.sbNeedsState === "pending")
        onTriggered: root.pollSbNeeds()
    }
    /// "Check now" on a send-back: asked at once, or as soon as the call under way has answered.
    function sbCheckNow() {
        sbAskNow = true
        if (!sbPolling && !sbAsking)
            pollSendBacks()
    }
    /// The next send-back check by itself (Logic.sendBackAutoMode, bridgeNextCheck): a job in
    /// progress every 5 s while shown; on its own screen in front, a busy endpoint gently and a
    /// releasable time once; otherwise none.
    function sbScheduleNext() {
        var onScreen = walletView === "sendback"
        var rows = onScreen ? sendBacks.filter(function(x) { return x.id === sbFocus }) : sendBacks
        var items = rows.map(function(x) {
            var m = Logic.sendBackAutoMode(sbStatuses[x.id] || { stage: x.stage })
            return { mode: m.mode, at: m.at, focused: onScreen && x.id === sbFocus }
        })
        sbNextAt = Logic.bridgeNextCheck({ now: Date.now(), visible: receiveWatching, frontmost: receiveFrontmost, screen: onScreen ? "sendback" : "card", items: items })
    }
    /// The send-backs: the bridge's list once, then send_back_status of each one under way, one
    /// call at a time, never beside a receive check; when asked or due (sbScheduleNext).
    function pollSendBacks() {
        if (sbPolling || receivePolling || receiveHold || !bridgeReady || receiveAsking || sbAsking || receiveNeeds !== null || sbNeeds !== null)
            return
        var asked = sbAskNow || !sendBacksLoaded
        sbAskNow = false
        if (!sbCan) {
            sbNextAt = 0
            return
        }
        if (!sendBacksLoaded) {
            sbPolling = true
            bridgeCall("send_backs", [], function(value) {
                sbPolling = false
                sendBacksLoaded = true
                var found = Logic.sendBacksFrom(value)
                sendBacks = found.concat(sendBacks.filter(function(x) { return !found.some(function(f) { return f.id === x.id }) }))
                sbAskNow = true
            }, function(message, kind) {
                sbPolling = false
                sbCheckFailed = !Logic.rateLimited(message)
                sbNextAt = bridgeListRetry(kind)
                console.log("names_ui: send_backs:", kind, message)
            }, 30000, false)
            return
        }
        var onScreen = walletView === "sendback" ? sbFocus : ""
        var open = sendBacks.filter(function(x) {
            var st = sbStatuses[x.id] ? sbStatuses[x.id].stage : x.stage
            return st !== "released" && (st !== "planned" || x.id === onScreen) && (onScreen === "" || x.id === onScreen)
                && (asked || Logic.sendBackAutoMode(sbStatuses[x.id] || { stage: x.stage }).mode !== "none")
        })
        if (open.length === 0) {
            sbCheckedAt = Date.now()
            sbNextAt = 0
            return
        }
        sbPolling = true
        var failedAny = false
        var next = function(i) {
            if (i >= open.length) {
                sbPolling = false
                sbCheckedAt = Date.now()
                sbCheckFailed = failedAny
                sbScheduleNext()
                return
            }
            var id = open[i].id
            bridgeCall("send_back_status", [id], function(value) {
                var all = sbStatuses
                var was = all[id] ? all[id].stage : open[i].stage
                all[id] = value
                sbStatuses = all
                sbStatusesChanged()
                if (value && value.stage !== was) {
                    console.log("names_ui: send back", id + ":", value.stage)
                    // The burn landed: the account's balance is read again.
                    if (value.stage === "burned")
                        loadBridgeNet()
                }
                if (value && value.stage === "released" && !sbReceivedAt[id]) {
                    var at = sbReceivedAt
                    at[id] = Date.now()
                    sbReceivedAt = at
                    sbReceivedAtChanged()
                }
                next(i + 1)
            }, function(message, kind) {
                console.log("names_ui: send_back_status", id + ":", kind, message)
                if (!Logic.rateLimited(message))
                    failedAny = true
                next(i + 1)
            }, 90000, false)
        }
        next(0)
    }
    /// "Make the release proof": the bridge's prover on this computer; without one set up (today),
    /// the proof inputs and the command that makes it.
    function sbProve() {
        if (sbAsking || sbFocus === "")
            return
        sbAsking = true
        sbError = ""
        var id = sbFocus
        bridgeCall("send_back_prove", [id], function(value) {
            // No prover set yet: the installed one, once (the bridge refuses a path with no program).
            if (value && value.needs === "prover" && !sbProverTried) {
                sbProverTried = true
                bridgeCall("set_release_prover", [JSON.stringify([Logic.RELEASE_PROVER])], function() {
                    console.log("names_ui: send back: release prover set to", Logic.RELEASE_PROVER)
                    sbAsking = false
                    sbProve()
                }, function(message, kind) {
                    console.log("names_ui: send back: set_release_prover:", kind, message)
                    sbAsking = false
                    sbProve()
                }, 30000, false)
                return
            }
            if (value && value.needs === "prover") {
                bridgeCall("send_back_witness", [id], function(w) {
                    sbAsking = false
                    sbWitness = w
                }, function(message, kind) {
                    sbAsking = false
                    sbError = sentence(message)
                    console.log("names_ui: send back: send_back_witness:", kind, message)
                }, 120000, false)
                return
            }
            sbAsking = false
            if (value && value.needs) {
                sbNeedsReply(value, "burn")
                return
            }
            var all = sbStatuses
            all[id] = Object.assign({}, all[id] || {}, { stage: value && value.stage ? value.stage : "proving" })
            sbStatuses = all
            sbStatusesChanged()
            sbScheduleNext()
        }, function(message, kind) {
            sbAsking = false
            sbError = sentence(message)
            console.log("names_ui: send back: send_back_prove:", kind, message)
        }, 120000, false)
    }
    /// The seal the bridge team sends (today's stand-in for the wrap service), checked by the bridge
    /// against Sepolia's verifier before it is kept.
    function sbSeal() {
        var seal = sbSealInput.replace(/\s+/g, "")
        if (sbAsking || sbFocus === "" || seal === "")
            return
        sbAsking = true
        sbError = ""
        var id = sbFocus
        bridgeCall("send_back_seal", [id, seal], function(value) {
            sbAsking = false
            sbSealInput = ""
            var all = sbStatuses
            all[id] = value
            sbStatuses = all
            sbStatusesChanged()
        }, function(message, kind) {
            sbAsking = false
            sbError = sentence(message)
            console.log("names_ui: send back: send_back_seal:", kind, message)
        }, 120000, false)
    }
    /// "Copy the link" once releasable: the bridge page's link with the release code in it.
    function sbCopyRelease() {
        if (sbAsking || sbFocus === "")
            return
        if (sbRelease !== null && sbRelease.id === sbFocus && sbRelease.release_link) {
            copy(String(sbRelease.release_link), "The link")
            sbLinkCopied = true
            againAfter(2500, function() { sbLinkCopied = false })
            return
        }
        sbAsking = true
        var id = sbFocus
        bridgeCall("send_back_release_code", [id], function(value) {
            sbAsking = false
            sbRelease = Object.assign({ id: id }, value)
            sbCopyRelease()
        }, function(message, kind) {
            sbAsking = false
            sbError = sentence(message)
            console.log("names_ui: send back: send_back_release_code:", kind, message)
        }, 60000, false)
    }

    /// Claims a receive that arrived: the claim screen, with the deposit instead of a file.
    function claimReceive(id) {
        leaveClaim()
        walletView = "claim"
        claimSource = "deposit:" + id
        claimBridgeAnswers = true
        checkDeposit(id)
    }

    /// Opens the claim screen: is the LEZ Bridge package here, and is a claim already running?
    function openClaim() {
        walletView = "claim"
        claimError = ""
        claimNote = ""
        claimMissing = false
        claimBridgeAnswers = null
        // What an earlier install said is over; one still on its way keeps its line.
        if (bridgeInstall !== null && (bridgeInstall.phase === "failed" || bridgeInstall.phase === "updated"))
            bridgeInstall = null
        loadInstalled(function() {
            // Not installed: said at once, not after the module call gives up.
            if (Logic.bridgeSetup({ list: installedList }) === "absent")
                claimMissing = true
            checkBundle()
        })
        bridgeCall("config", [], function(value) {
            claimBridgeAnswers = true
            bridgeAnswered(value)
            claimDeployment = value && value.deployment ? value.deployment : null
            bridgeCall("relayed_claims", [], function(value) {
                var job = Logic.runningClaimJob(Array.isArray(value) ? value : value && value.jobs ? value.jobs : [])
                if (job !== null && depositClaimJob === null)
                    depositClaimJob = job
                // Does this build have relayed_claim_check_with? An empty file asks nothing of anyone
                // (its answer is "bad_file"); an older build hasn't the method at all.
                bridgeCall("relayed_claim_check_with", ["", JSON.stringify({ wallet_hint: false })], function() { claimCheckWith = true },
                    function(message, kind) { if (kind !== "busy" && kind !== "starting") claimCheckWith = false }, 15000, false)
            })
        }, function(message, kind) {
            // Not installed, not started, or never answering: Install / Start, or one plain line.
            claimBridgeAnswers = false
            bridgeSilent(kind)
            claimMissing = kind === "missing" || kind === "starting" || kind === "busy" || kind === "stale"
            if (!claimMissing)
                claimError = sentence(message)
            console.log("names_ui: claim:", kind, message)
        }, 10000, false)
    }

    /// What Basecamp has installed, from its package manager (read only). `done` runs after.
    function loadInstalled(done) {
        if (!(bridgeReady && logos.callModuleAsync)) {
            if (done) done()
            return
        }
        logos.callModuleAsync("package_manager", "getInstalledPackages", [], function(reply) {
            installedList = Logic.installedPackages(reply)
            if (installedList === null)
                console.log("names_ui: the installed packages couldn't be read:", String(reply).slice(0, 200))
            if (done) done()
        }, 15000)
    }

    /// Checks the LEZ Bridge files shipped inside Names against the hashes built into this app,
    /// before anything offers to install them (the names module reads the files).
    property int bundleTries: 0          // checks of the shipped bridge the names module didn't answer
    property bool bundleChecking: false
    function checkBundle() {
        if (bridgeBundle !== "" || installedList === null || bundleChecking)
            return
        if (moduleFeatures.indexOf("file_sha256") < 0) {
            bridgeBundle = "bad"
            return
        }
        // A check the module didn't answer (busy reading the wallet as Names opens) is no verdict:
        // the bundle stays unchecked and is asked again, up to 6 times, 10 s apart.
        var left = Logic.BUNDLED_BRIDGE.length, good = true, unanswered = false
        bundleChecking = true
        Logic.BUNDLED_BRIDGE.forEach(function(b) {
            var path = Logic.bundledPath(installedList, b.file)
            var settle = function(ok) {
                good = good && ok
                if (--left > 0)
                    return
                bundleChecking = false
                if (unanswered && bundleTries < 6) {
                    bundleTries = bundleTries + 1
                    againAfter(10000, checkBundle)
                    return
                }
                bridgeBundle = good && !unanswered ? "ok" : "bad"
            }
            if (path === "")
                return settle(false)
            askLater("file_sha256", [path], function(sha) { settle(sha === b.sha256) }, function(message) {
                console.log("names_ui: the shipped", b.file, "can't be checked:", message)
                if (message === moduleNoAnswer || message === moduleBusy)
                    unanswered = true
                settle(false)
            }, 30000)
        })
    }

    /// Installs the LEZ Bridge shipped inside Names: Basecamp's own install question first (it
    /// names Names as the app asking, and says the package isn't signed); only on its OK, the
    /// module, then the app; then starts it. `update`: over an older build of the same version,
    /// which "only if newer" would skip.
    function updateBridge(then) {
        bridgeUpdateThen = then || ""
        installBridge(true)
    }
    function installBridgeFor(then) {
        bridgeUpdateThen = then || ""
        installBridge(false)
    }
    function installBridge(update) {
        if (bridgeInstall !== null && bridgeInstall.phase !== "failed" && bridgeInstall.phase !== "updated")
            return
        // The files are the shipped ones already: nothing to install again, only the restart.
        if (update && installedList !== null && Logic.installedEntry(installedList, "lez_bridge") !== null && !Logic.bridgeDiffers(installedList)) {
            bridgeInstall = { phase: "unload" }
            bridgePending = "update"
            noteBridge("update:" + bridgeUpdateThen)
            return
        }
        if (bridgeBundle !== "ok" || installedList === null || !logos.request)
            return
        bridgeInstall = { phase: "asking", update: !!update, line: "Basecamp asks you to confirm the install…" }
        logos.request("basecamp.packages.confirm_install", { name: "lez_bridge", version: "0.1.0", repositoryUrl: "" }, function(res) {
            var answer = Logic.intentAnswer(res)
            if (answer === "cancelled") {
                bridgeInstall = null
                return
            }
            if (answer !== "ok") {
                console.log("names_ui: confirm_install:", JSON.stringify(res))
                var file = Logic.bundledPath(installedList, Logic.BUNDLED_BRIDGE[0].file)
                bridgeInstall = { phase: "failed", line: (answer === "not_declared"
                    ? "Basecamp hasn't learnt yet that Names may ask this (it does at its next start), so nothing was installed."
                    : "Basecamp didn't ask to confirm, so nothing was installed.")
                    + " Or install it yourself: Package Manager (the cube icon) → Install Local Package, the file " + file + "." }
                return
            }
            installBridgePart(0, !!update)
        })
    }
    function installBridgePart(i, update) {
        var b = Logic.BUNDLED_BRIDGE[i]
        var path = Logic.bundledPath(installedList, b.file)
        bridgeInstall = { phase: "installing", update: update, line: "Installing the " + b.label + "…" }
        logos.callModuleAsync("package_manager", "installPlugin", [path, !update, "file://" + path], function(reply) {
            var r = Logic.installReply(reply)
            if (r.timeout) {
                // Not a failure: Basecamp may still be installing. Look at what is installed.
                bridgeInstall = { phase: "slow", update: update, line: "Still installing the " + b.label + "…" }
                againAfter(10000, function() { afterBridgeInstall(update, 0) })
                return
            }
            if (!r.ok) {
                bridgeInstall = { phase: "failed", line: "The " + b.label + " didn't install: " + sentence(r.error || "Basecamp didn't say why") }
                return
            }
            if (i + 1 < Logic.BUNDLED_BRIDGE.length)
                installBridgePart(i + 1, update)
            else
                afterBridgeInstall(update, 0)
        }, 300000)
    }
    /// After the install: both packages listed (Basecamp rescans by itself), then the module starts.
    function afterBridgeInstall(update, tries) {
        loadInstalled(function() {
            var both = installedList !== null && Logic.BUNDLED_BRIDGE.every(function(b) { return Logic.installedEntry(installedList, b.name) !== null })
            var fresh = both && !Logic.bridgeDiffers(installedList)
            if (!(update ? fresh : both)) {
                if (tries < 30) {
                    againAfter(10000, function() { afterBridgeInstall(update, tries + 1) })
                    return
                }
                bridgeInstall = { phase: "failed", line: "The install didn't finish. Look in Package Manager (the cube icon), then come back here." }
                return
            }
            // The running LEZ Bridge keeps its old code until Basecamp unloads it (Module Inspector);
            // a new one starts when Names opens (it is Names' optional dependency).
            bridgeInstall = update ? { phase: "unload" } : { phase: "reopen" }
            bridgePending = update ? "update" : "install"
            noteBridge(bridgePending + ":" + bridgeUpdateThen)
        })
    }

    /// While a setup step waits on Basecamp (reopen, unload, a stuck bridge): the bridge is asked
    /// every 3 s, one call at a time; once the new one answers and can receive, the step is over and
    /// the person goes on to where they started.
    function pollBridgeSetup() {
        if (bridgeRestartAsking)
            return
        bridgeRestartAsking = true
        bridgeCall("config", [], function(value) {
            bridgeRestartAsking = false
            bridgeAnswered(value)
            if (!Logic.bridgeBehind(bridgeConfig) && (bridgeInstall !== null || bridgePending !== ""))
                bridgeSetupDone()
        }, function(message, kind) {
            bridgeRestartAsking = false
            bridgeSilent(kind)
        }, 5000, false)
    }
    /// A setup step is over (the new bridge answers and can receive): "LEZ Bridge updated." once,
    /// on the screen the person started from.
    function bridgeSetupDone() {
        var kind = bridgePending !== "" ? bridgePending : bridgeInstall !== null && bridgeInstall.update ? "update" : "install"
        var then = bridgeUpdateThen
        bridgeUpdateThen = ""
        bridgePending = ""
        bridgeInstall = null
        noteBridge("")
        loadInstalled(function() {})
        if (then === "receive")
            openEthReceive()
        else if (then === "sendback")
            openSendBack(sbAccount, sbAsset, sbFocus === "" ? undefined : sbFocus)
        else if (then === "claim")
            openClaim()
        bridgeDoneView = walletView
        bridgeDone = kind === "update" ? "LEZ Bridge updated." : "LEZ Bridge installed."
    }
    /// Keeps the step under way in the names module (`app_note`), so a reopen of Names knows it:
    /// "update:receive", "install:claim"… ("" forgets it).
    function noteBridge(value) {
        if (moduleFeatures.indexOf("app_note") >= 0)
            askLater("app_note", ["bridge_setup", value], function() {}, function() {}, 10000)
    }
    /// "What's new" at the top of Settings: shown until "Got it", which the names module keeps in
    /// memory (`app_note`), so it shows again after the next update (the module starts afresh) or
    /// Basecamp start. `whatsNewSeen`: "?" until read, else the build it was seen for ("" none).
    property string whatsNewSeen: "?"
    readonly property bool whatsNewShown: Logic.WHATS_NEW.length > 0 && (whatsNewSeen === "" || (whatsNewSeen !== "?" && whatsNewSeen !== buildCommit))
    property bool whatsNewRead: false
    function readWhatsNew() {
        if (whatsNewRead)
            return
        whatsNewRead = true
        if (moduleFeatures.indexOf("app_note") < 0) {
            whatsNewSeen = ""
            return
        }
        // app_note answers the old value and forgets it when given "": read, then keep it again.
        askLater("app_note", ["whats_new", ""], function(value) {
            whatsNewSeen = String(value || "")
            if (whatsNewSeen !== "")
                askLater("app_note", ["whats_new", whatsNewSeen], function() {}, function() {}, 10000)
        }, function() { whatsNewSeen = "" }, 10000)
    }
    function whatsNewDone() {
        whatsNewSeen = buildCommit !== "" ? buildCommit : "dev"
        if (moduleFeatures.indexOf("app_note") >= 0)
            askLater("app_note", ["whats_new", whatsNewSeen], function() {}, function() {}, 10000)
    }

    /// When Names opens: a setup step noted before the reopen is finished here once the new bridge
    /// answers (its line shows once); otherwise it stays noted for the next look.
    function readBridgeNote() {
        if (bridgeNoteRead || moduleFeatures.indexOf("app_note") < 0)
            return
        bridgeNoteRead = true
        askLater("app_note", ["bridge_setup", ""], function(value) {
            var note = String(value || "")
            if (note === "")
                return
            var parts = note.split(":")
            bridgePending = parts[0] === "update" ? "update" : "install"
            bridgeUpdateThen = parts[1] || ""
            noteBridge(note)
            loadInstalled(checkBundle)
            bridgeCall("config", [], function(value) {
                bridgeAnswered(value)
                if (!Logic.bridgeBehind(bridgeConfig))
                    bridgeSetupDone()
            }, function(message, kind) { bridgeSilent(kind) }, 10000)
        }, function() {}, 10000)
    }
    Timer {
        interval: 3000
        repeat: true
        // Only while that setup step is on screen and Basecamp is in front.
        running: (root.bridgeSetup.state === "reopen" || root.bridgeSetup.state === "unload" || root.bridgeSetup.state === "stuck") && root.bridgeSetupOnView
        onTriggered: root.pollBridgeSetup()
    }
    // The setup box is shown on Claim, Receive, Bridge to Ethereum and the LEZ 0.4 card.
    readonly property bool bridgeSetupOnView: section === "wallet" && receiveFrontmost
        && (walletView === "home" ? walletNet === "0.4" : ["claim", "ethreceive", "sendback"].indexOf(walletView) >= 0)
    function bridgeSetupAction(action, then) {
        if (action === "install")
            installBridgeFor(then)
        else if (action === "update")
            updateBridge(then)
        else if (action === "settings" && logos.request)
            logos.request("basecamp.settings.open", {}, function() {})
    }

    /// Back to the Wallet. A file checked and not claimed is forgotten by the module (its key leaves
    /// its memory); a running claim goes on and is shown again when the screen opens.
    function leaveClaim() {
        if (claimCheck !== null && claimCheck.id && depositClaimJob === null)
            bridgeCall("relayed_claim_forget", [claimCheck.id], null, null, 30000, false)
        claimCheck = null
        claimNeeds = null
        claimUncertain = null
        claimError = ""
        claimPaste = ""
        claimSource = ""
        claimRetryAt = 0
        claimDetails = false
        claimExtras = []
        claimExtrasDone = []
        if (depositClaimJob !== null && depositClaimJob.state !== "running")
            depositClaimJob = null
        walletView = "home"
    }

    function pickClaimFile() {
        claimNote = ""
        try {
            var d = Qt.createQmlObject('import QtQuick.Dialogs; FileDialog { title: "Choose your claim file"; nameFilters: ["Claim files (*.json)", "All files (*)"] }', root, "claimFilePicker")
            d.accepted.connect(function() { var path = String(d.selectedFile); d.destroy(); checkClaimFile(path) })
            d.rejected.connect(function() { d.destroy() })
            d.open()
        } catch (error) {
            claimNote = "This Basecamp has no file picker here: paste the claim file's text, or its full path, below."
        }
    }

    /// A check's answer (a file's or a received deposit's): shown, its first-time questions
    /// foreseen, checked again by itself when not ready with a time, and a claim already started
    /// for it shown rather than started twice.
    function takeClaimCheck(value) {
        claimChecking = false
        claimCheck = value
        if (value && value.ok) {
            var extras = Logic.claimExtras(appGrants, value.deposit ? value.deposit.account : "")
            claimExtras = claimSource.indexOf("deposit:") === 0 ? claimExtras.concat(extras.filter(function(k) { return claimExtras.indexOf(k) < 0 })) : extras
            claimExtrasDone = claimSource.indexOf("deposit:") === 0 ? claimExtrasDone : []
        }
        if (value && !value.ok && value.error) {
            var wait = Logic.claimRetryAfter(value.error.code, value.error)
            if (wait > 0)
                claimRetryAt = Date.now() + wait * 1000
        }
        if (value && value.ok)
            bridgeCall("relayed_claims", [], function(list) {
                var job = Logic.claimJobFor(Array.isArray(list) ? list : list && list.jobs ? list.jobs : [], value)
                if (job !== null)
                    depositClaimJob = job
            })
    }

    /// A received deposit's six checks, with its key from this wallet's values (no file): the
    /// same answer as a file's check. Its first `needs: "secret"` is a sheet first, then this again.
    function checkDeposit(id) {
        claimError = ""
        claimRetryAt = 0
        claimCheck = null
        depositClaimJob = null
        claimNeeds = null
        claimUncertain = null
        claimChecking = true
        bridgeCall("relayed_claim_check_deposit", [id, JSON.stringify({ wallet_hint: false })], function(value) {
            if (Logic.needsWithoutRequest(value) !== "") {
                claimChecking = false
                claimError = sentence(Logic.needsWithoutRequest(value))
                console.log("names_ui: claim:", claimError)
                return
            }
            if (value && value.needs) {
                claimChecking = false
                var key = Logic.needsStep(value.needs)
                if (claimExtras.indexOf(key) < 0)
                    claimExtras = claimExtras.concat([key])
                claimNeedsAfter = "check"
                claimNeeds = value
                claimNeedsState = value.request && value.request.state ? value.request.state : "pending"
                return
            }
            takeClaimCheck(value)
        }, function(message, kind) {
            claimChecking = false
            if (kind === "missing")
                claimError = "This LEZ Bridge can't claim a received deposit yet. Update it."
            else
                claimError = sentence(kind === "busy" ? "The LEZ Bridge is still answering an earlier call. Check again." : message)
            console.log("names_ui: claim:", kind, message)
        }, 90000, false)
    }

    /// The six checks (nothing is proposed, signed or sent). A pasted file leaves the field at once.
    function checkClaimFile(source, switched) {
        var given = Logic.claimInput(source)
        if (given.kind === "empty")
            return
        if (given.problem !== "") {
            claimNote = given.problem
            return
        }
        claimPaste = ""
        claimNote = ""
        claimError = ""
        claimSource = String(source)
        claimRetryAt = 0
        claimCheck = null
        depositClaimJob = null
        claimNeeds = null
        claimUncertain = null
        claimChecking = true
        // The six checks without asking the wallet anything (relayed_claim_check_with, newer
        // lez_bridge packages); an older package has only relayed_claim_check, which also asks
        // the wallet and may take up to about a minute while it is busy.
        var withOptions = claimCheckWith
        var checked = function(value) {
            // A file for the other built-in bridge: the module is switched to it and checks again.
            var tried = Array.isArray(switched) ? switched : []
            var other = claimDeployment ? Logic.otherDeployment(claimDeployment.name, tried) : ""
            if (value && !value.ok && value.error && value.error.code === "other_vault" && other !== "" && tried.length < 2) {
                var was = claimDeployment.name
                bridgeCall("use_deployment", [other], function(config) {
                    claimDeployment = config && config.deployment ? config.deployment : { name: other }
                    checkClaimFile(source, tried.concat([was]))
                }, function(message) {
                    claimChecking = false
                    claimCheck = value
                })
                return
            }
            takeClaimCheck(value)
        }
        var refused = function(message, kind) {
            if (withOptions && kind !== "busy") {
                // relayed_claim_check_with failed as a call (an older package hasn't it, whatever
                // words the host uses for that): the older call, from now on. A check's own
                // refusal is an answer, not a failure, so it never comes here.
                claimCheckWith = false
                withOptions = false
                bridgeCall("relayed_claim_check", [source], checked, refused, 90000)
                return
            }
            claimChecking = false
            if (kind === "missing")
                claimMissing = true
            else
                claimError = sentence(message)
            console.log("names_ui: claim:", kind, message)
        }
        if (withOptions)
            bridgeCall("relayed_claim_check_with", [source, JSON.stringify({ wallet_hint: false })], checked, refused, 90000)
        else
            bridgeCall("relayed_claim_check", [source], checked, refused, 90000)
    }

    /// Claim: the module runs its gates and proposes. A wallet sheet it needs first comes up here
    /// in place; once answered, this asks again. Never sent again by itself (it proposes).
    function startClaim(afterUncertain) {
        if (claimCheck === null || !claimCheck.ok || claimAsking)
            return
        claimAsking = true
        claimError = ""
        claimUncertain = null
        bridgeCall("relayed_claim", [claimCheck.id, afterUncertain ? JSON.stringify({ after_uncertain: true }) : "{}"], function(value) {
            claimAsking = false
            if (value && value.needs === "uncertain") {
                claimUncertain = value
                return
            }
            if (Logic.needsWithoutRequest(value) !== "") {
                claimError = sentence(Logic.needsWithoutRequest(value))
                console.log("names_ui: claim:", claimError)
                claimAgain = true
                return
            }
            if (value && value.needs) {
                claimNeedsAfter = "claim"
                if (claimExtras.indexOf(Logic.needsStep(value.needs)) < 0)
                    claimExtras = claimExtras.concat([Logic.needsStep(value.needs)])
                claimNeeds = value
                claimNeedsState = value.request && value.request.state ? value.request.state : "pending"
                return
            }
            depositClaimJob = value
        }, function(message, kind) {
            claimAsking = false
            var failure = Logic.claimFailure(kind === "busy" ? "still answering an earlier call" : message)
            claimError = failure.line
            console.log("names_ui: claim:", claimError)
            claimAgain = failure.again
        }, 240000, false)
    }

    function pollClaimNeeds() {
        var id = claimNeeds && claimNeeds.request ? String(claimNeeds.request.request || "") : ""
        if (id === "" || claimNeedsPolling)
            return
        claimNeedsPolling = true
        bridgeCall("wallet_request", [id], function(value) {
            claimNeedsPolling = false
            claimNeedsState = value && value.state ? value.state : ""
            if (claimNeedsState === "approved" || claimNeedsState === "done") {
                claimExtrasDone = claimExtrasDone.concat([Logic.needsStep(claimNeeds.needs)])
                claimNeeds = null
                var after = claimNeedsAfter
                claimNeedsAfter = "claim"
                if (after === "check")
                    checkAgain()
                else
                    startClaim(false)
            } else if (claimNeedsState === "rejected" || claimNeedsState === "expired" || claimNeedsState === "cancelled") {
                claimNeeds = null
                claimError = Logic.approvalEndedLine(claimNeedsState)
                claimAgain = true
            }
        }, function() { claimNeedsPolling = false }, 120000)
    }
    Timer {
        interval: 2500
        repeat: true
        running: root.claimNeeds !== null && (root.claimNeedsState === "" || root.claimNeedsState === "pending")
        onTriggered: root.pollClaimNeeds()
    }

    /// The claim's progress: the module moves the job only while it is asked, one call at a time.
    function pollClaimJob() {
        if (depositClaimJob === null || claimPolling)
            return
        claimPolling = true
        bridgeCall("relayed_claim_status", [depositClaimJob.id], function(value) {
            claimPolling = false
            depositClaimJob = value
            if (value && value.state === "done") {
                loadBridgeNet()
                // A receive claimed from the card: its row reads "Claimed in block N" for a while.
                if (claimSource.indexOf("deposit:") === 0)
                    receiveClaimed(claimSource.substr(8), value.block)
            }
        }, function() { claimPolling = false }, 150000)
    }
    Timer {
        interval: 2500
        repeat: true
        running: root.depositClaimJob !== null && root.depositClaimJob.state === "running"
        onTriggered: root.pollClaimJob()
    }
    function cancelClaim() {
        if (depositClaimJob === null)
            return
        bridgeCall("relayed_claim_cancel", [depositClaimJob.id], function(value) { depositClaimJob = value }, function(message) { claimError = sentence(message); console.log("names_ui: claim:", claimError) }, 120000, false)
    }

    /// Watches (or stops watching) a public account on the bridge test network.
    function bridgeNetWatch(account, on) {
        var problem = on ? Logic.publicIdProblem(account) : ""
        if (problem !== "") {
            bridgeNetWatchProblem = problem
            return
        }
        askLater("bridge_net_watch", [String(account).trim(), on], function() {
            bridgeNetWatchProblem = ""
            if (on)
                bridgeNetWatchField.text = ""
            loadBridgeNet()
        }, function(message) {
            bridgeNetWatchProblem = sentence(message)
        })
    }

    /// Switches the names network, and with it the Wallet, to `net` ("0.3" or "0.4"). The
    /// module keeps each network's settings apart; everything shown is read again.
    function setWalletNet(net) {
        if (net === namesNet || netSwitching)
            return
        if (namesNet === "0.3")
            accounts03 = Logic.publicAccounts(cliBalance, cliAccounts)
        netSwitching = true
        netProblem = ""
        askLater("set_names_network", ["lez-" + net], function(value) {
            netSwitching = false
            walletRowOpen = ""
            record = null
            missing = ""
            bridgeNet = null
            // Nothing read on the network left is shown, or copyable, as this one's (S-A).
            cliBalance = undefined
            cliAccounts = null
            cliStatus = null
            cliNames = []
            cliPending = []
            cliKnown = false
            // The registrations under way are the network's own: the reveal driver never sees the
            // other network's (MF-2), and the module's reveal_name can't start one anyway.
            myNames = []
            pendingNames = []
            revealing = false
            tokenHoldings = null
            walletAccount = null
            if (walletView === "account")
                walletView = "home"
            namesNet = net
            loadConfig()
            if (net === "0.4")
                loadBridgeNet()
        }, function(message) {
            netSwitching = false
            netProblem = sentence(message)
        })
    }

    // Read when the bridge test network's balances come into view (and after the person's own
    // actions, and on Refresh): no timer ("if they ignore it, we ignore it").
    readonly property bool bridgeNetOnView: walletNet === "0.4" && section === "wallet" && walletView === "home" && Qt.application.state === Qt.ApplicationActive
    onBridgeNetOnViewChanged: if (bridgeNetOnView) loadBridgeNet()
    // A read under way: ask again shortly for its result (the module answers from memory).
    Timer { id: bridgeNetSoon; interval: 3000; onTriggered: root.loadBridgeNet() }
    property string namesNote: ""     // the same for the names list: kept from before, and why
    property string accountsNote: ""  // the same for the accounts list

    // ── Accounts and fees ───────────────────────────────────────────────

    property var cliAccounts: null        // cli_accounts, sorted for Settings; null until read
    property string accountsError: ""
    /// The names program is older than this app (the October testnet build): it can't list or make accounts.
    readonly property bool cliOld: cliOutdated || /older than this app/.test(accountsError)
    property bool accountsBusy: false     // a change to the accounts is on its way
    property string accountRenaming: ""   // the account whose nickname is being edited
    property string accountNewKind: ""    // "public" or "private" while one is being made
    property var feeQuote: null           // cli_fee, or null
    property double feeQuoteAt: 0

    /// Reads every account of the names CLI's wallet.
    function loadAccounts() {
        if (cliPath === "")
            return
        askLater("cli_accounts", [], onNet(function(value, reply) {
            cliAccounts = Logic.accountsShown(value)
            accountsNote = Logic.readNote(reply)
            accountsError = ""
        }), function(message) {
            console.log("names_ui: accounts:", message)
            // Busy keeps the accounts shown and asks again; anything else is said.
            if (waitAgain(message)) {
                againWhenFree("accounts", loadAccounts)
                return
            }
            accountsError = sentence(message)
        }, 180000)
    }

    /// Changes the accounts through `method`, then reads them again.
    function accountChange(method, args, done) {
        if (accountsBusy)
            return
        accountsBusy = true
        accountsError = ""
        askLater(method, args, function(value) {
            accountsBusy = false
            if (done)
                done(value)
            loadAccounts()
            loadCliBalance()
        }, function(message) {
            accountsBusy = false
            accountsError = sentence(message)
        }, 60000)
    }

    function useAccount(account) {
        accountChange("cli_account_use", [account], function(value) {
            say(Logic.accountTitle(value) + " " + Logic.keyShown(String(value.account || "").replace(/^(Public|Private)\//, "")) + " now pays and registers.", false)
        })
    }

    function nameAccount(account, label) {
        accountChange("cli_account_label", [account, label], function() { accountRenaming = "" })
    }

    function newAccount(kind, label) {
        accountChange("cli_account_new", [kind, label], function(value) {
            accountNewKind = ""
            say(value.label ? "Made " + value.label + "." : "Made a new " + kind + " account.", false)
        })
    }

    /// Reads what the chain's fees are now, at most once a minute.
    function loadFee() {
        if (cliPath === "" || Date.now() - feeQuoteAt < 60000)
            return
        feeQuoteAt = Date.now()
        askLater("cli_fee", [], function(value) {
            feeQuote = value
        }, function(message) {
            feeQuoteAt = 0
            console.log("names_ui: fee:", message)
        }, 30000)
    }

    // ── The Logos Blockchain: Basecamp's own node (chain_* in the module) ──

    property var chainStatus: null       // chain_status, or null
    property var chainAccounts: null     // chain_accounts: [{key, balance}]; null until read
    // The keys the node uses now: an account made since it started can't send, and what
    // reaches it before the node uses it isn't seen, so it isn't offered to pay or receive.
    readonly property var chainUsable: Logic.chainUsable(chainAccounts)
    property var chainStake: null        // chain_stake, or null
    property string chainError: ""       // why the node couldn't be read
    property bool chainReading: false
    property string chainFrom: ""        // the key the Send form pays from
    property var chainPlan: null         // the checked send: {from, to, name, forwarded_from, amount, balance}
    property string chainPlanFor: ""     // what that plan was checked for: from|to|amount
    property bool chainBusy: false       // a check, send or claim is on its way
    property string chainProblem: ""     // why the last check, send or claim failed
    property var chainSent: null         // the last send: {hash, amount, to, name, state, fee}
    property double chainSentAt: 0
    property string chainClaimed: ""     // the last claim's transaction, "" if none
    property var chainClaimPlan: null    // chain_claim(false): what one claim costs (fee_about, warning)
    property string chainAccountNote: "" // what the last New account did, or why it couldn't
    property bool chainAccountNoteIsError: false

    /// Makes a new account on the node (chain_account_new), labelled `label`, and reads the keys again.
    function chainAccountNew(label) {
        if (chainBusy)
            return
        chainBusy = true
        chainAccountNote = ""
        askLater("chain_account_new", [String(label || "").trim()], function(value) {
            chainBusy = false
            chainAccountNoteIsError = false
            chainAccountNote = "Made " + (value.label ? value.label + " (" + Logic.keyShown(value.key) + ")" : "account " + Logic.keyShown(value.key)) + "."
                + (value.note ? " " + value.note : "")
            // Read the keys again now, even while a fuller read (loadChain) is on its way.
            askLater("chain_accounts", [], function(keys) { chainAccounts = keys }, function(message) { console.log("names_ui: accounts:", message) }, 30000)
        }, function(message) {
            chainBusy = false
            chainAccountNoteIsError = true
            chainAccountNote = sentence(message)
        }, 30000)
    }

    /// Opens Blockchain and reads the node.
    function openChain() {
        chainProblem = ""
        openWallet("stake")
    }

    // A chain read the busy module didn't get to in time is asked again, not shown as a stopped node.
    Timer {
        id: chainRetry
        interval: 5000
        onTriggered: root.loadChain()
    }

    /// Reads the node's state, its keys and its stake.
    function loadChain() {
        if (chainReading)
            return
        chainReading = true
        askLater("chain_status", [], function(value) {
            chainStatus = value
            chainError = ""
            askLater("chain_accounts", [], function(keys) {
                chainAccounts = keys
                var usable = Logic.chainUsable(keys)
                if (chainFrom === "" && usable.length > 0)
                    chainFrom = usable[0].key
                askLater("chain_stake", [], function(stake) {
                    chainStake = stake
                    chainReading = false
                    // What one claim would cost, asked of the node before Claim is pressed.
                    if (stake && stake.vouchers > 0)
                        askLater("chain_claim", [false], function(plan) { chainClaimPlan = plan }, function(message) {
                            // Busy keeps the plan shown and reads the node again.
                            if (waitAgain(message))
                                chainRetry.start()
                            else
                                chainClaimPlan = null
                        }, 30000)
                }, function(message) {
                    chainReading = false
                    console.log("names_ui: stake:", message)
                    if (waitAgain(message))
                        chainRetry.start()
                }, 30000)
            }, function(message) {
                chainReading = false
                if (waitAgain(message)) {
                    chainRetry.start()
                    return
                }
                chainError = sentence(message)
            }, 60000)
        }, function(message) {
            chainReading = false
            if (waitAgain(message)) {
                chainRetry.start()
                return
            }
            chainStatus = null
            chainError = sentence(message)
        }, 20000)
    }

    function chainAsked() {
        return [chainFrom, sendToResolved(), Logic.amountDigits(sendAmount.text, Logic.CHAIN_AMOUNT_MAX)]
    }

    /// Checks the send and shows its plan; nothing is sent.
    function chainCheck() {
        var asked = chainAsked()
        var problem = asked[0] === "" ? "Choose a key to send from." : Logic.chainToProblem(asked[1])
        if (problem === "")
            problem = Logic.amountProblem(sendAmount.text, Logic.CHAIN_AMOUNT_MAX)
        if (problem !== "") {
            chainProblem = problem
            return
        }
        chainBusy = true
        chainProblem = ""
        chainPlan = null
        askLater("chain_send", [asked[0], asked[1], asked[2], false], function(plan) {
            chainBusy = false
            chainPlan = plan
            chainPlanFor = asked.join("|")
        }, function(message) {
            chainBusy = false
            chainProblem = sentence(message)
        }, 60000)
    }

    /// Sends what the plan shows. The node proves it first, which can take a minute.
    function chainSend() {
        var asked = chainAsked()
        if (chainPlan === null || chainPlanFor !== asked.join("|")) {
            chainCheck()
            return
        }
        chainBusy = true
        chainProblem = ""
        askLater("chain_send", [asked[0], asked[1], asked[2], true], function(sent) {
            chainBusy = false
            chainPlan = null
            sendAmount.text = ""
            chainSent = { hash: sent.hash, amount: sent.amount, to: sent.to, name: sent.name, state: sent.hash ? "pending" : "unknown", fee: sent.fee }
            chainSentAt = Date.now()
            loadChain()
        }, function(message) {
            chainBusy = false
            chainProblem = sentence(message)
        }, 240000)
    }

    /// Asks whether the last send is in a block yet.
    function chainPoll() {
        if (chainSent === null || chainSent.hash === null || chainSent.state !== "pending")
            return
        askLater("chain_tx", [chainSent.hash], function(tx) {
            if (chainSent === null || chainSent.hash !== tx.hash)
                return
            var updated = Object.assign({}, chainSent)
            updated.state = tx.state
            if (tx.fee)
                updated.fee = tx.fee
            chainSent = updated
            if (tx.state === "in_block")
                loadChain()
        }, function(message) {
            console.log("names_ui: chain tx:", message)
        }, 20000)
    }

    /// Claims one leader reward: the node's claim cashes one voucher and pays its own fee.
    function chainClaim() {
        chainBusy = true
        chainProblem = ""
        askLater("chain_claim", [true], function(claimed) {
            chainBusy = false
            chainClaimed = claimed.hash ? claimed.hash : ""
            say("Claimed " + Logic.grouped(claimed.claimed) + " (one block's reward)" + (claimed.fee ? " for a fee of " + Logic.grouped(claimed.fee) : "")
                + "; it lands with the next block." + (claimed.warning ? " " + claimed.warning : ""), claimed.warning ? true : false)
            loadChain()
        }, function(message) {
            chainBusy = false
            chainProblem = sentence(message)
        }, 240000)
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.section === "wallet" && root.chainSent !== null && root.chainSent.state === "pending" && Date.now() - root.chainSentAt < 600000
        onTriggered: root.chainPoll()
    }

    // ── Wallet: every balance, with send, receive, staking and the bridge ──

    property string walletView: "home"   // home, send, receive, stake, add or bridge
    property string sendKind: "lez"      // what Send sends: lez, token or chain
    property string sendDefinition: ""   // the token Send sends, when sendKind is token
    property string sendTitle: "LEZ"
    property bool sendFromPrivate: true  // LEZ and tokens: from the private account, else the public one
    property bool sendReviewing: false   // the summary is up, waiting for Confirm
    property string receiveKind: "lez"   // what Receive shows: lez, token or chain
    property var watchedTokens: []       // token IDs added by address (watched_tokens)
    property var watchedNames: ({})      // their names, as cli_token_info read them
    property string addTokenProblem: ""
    property bool addTokenBusy: false
    readonly property color orange: Theme.palette.primary
    property bool walletHidden: false    // amounts shown as dots (the eye)
    property var walletOpen: ({})        // chain group id -> false when closed
    property string walletRowOpen: ""    // the address row whose actions are open
    property bool walletShowEmpty: false // empty accounts that don't pay are listed too

    /// The rows a group shows: an empty account that isn't a default (a program's
    /// helper account, an unused one) only once asked for.
    function groupRows(group) {
        // An account a name is paid into stays in view even while it holds nothing.
        return walletShowEmpty ? group.rows : group.rows.filter(function(row) { return row.balance !== "0" || row.canSend || row.kind !== "lez" || (row.name !== undefined && row.name !== "") })
    }

    readonly property var walletGroups: Logic.walletGroups(cliBalance, cliAccounts, tokenProgram !== "" ? tokenHoldings : null, watchedTokens, watchedNames,
                                                           chainAccounts, chainAccounts === null && chainError !== "", chainStake, namesByNpk,
                                                           mainName, mainName).concat(Logic.bridgedGroups(bridgedHoldings))

    /// The wallet's names by the npk their profiles publish: {npk: [name]}, from the lookups.
    readonly property var namesByNpk: {
        var map = {}
        cliNames.forEach(function(entry) {
            var person = boardPeople[entry.name]
            if (person && person.npk) {
                map[person.npk] = (map[person.npk] || []).concat([entry.name])
            }
        })
        return map
    }

    /// An amount as the Wallet shows it: grouped, a dash while unknown, dots when hidden.
    // ── History (activity in the module) ───────────────────────────────
    property var activityRows: []         // newest first, as `activity` gives it
    property string activityFilter: ""    // Activity: "", "logos", "lez" or "bridge"
    readonly property var activityShown: activityRows.filter(function(e) {
        return activityFilter === "" || (activityFilter === "bridge" ? (e.kind === "bridge" || e.kind === "split") : e.chain === activityFilter)
    })

    property var walletAccount: null      // the account whose page is open: a Wallet row

    /// Opens an account's own page: its balance, what it can do, its history.
    function openAccount(row) {
        walletAccount = row
        walletView = "account"
        loadActivity()
    }

    /// The account a Wallet row stands for, as the history names it: `Public/…`,
    /// `Private/…`, or a node key.
    function rowAccount(row) {
        if (!row)
            return ""
        if (row.kind === "chain")
            return row.key
        if (row.account)
            return row.account
        if (row.kind === "lez")
            return row.fromPrivate ? (cliBalance ? "Private/" + cliBalance.account : "") : (cliPublic ? "Public/" + cliPublic.account : "")
        return ""
    }

    /// Whether Send works for `row`, and if not why: {enabled, reason, switchFirst}.
    function sendState(row) {
        if (!row)
            return { enabled: false, reason: "", switchFirst: false }
        if (row.later)   // a Logos Blockchain account the node takes at its next start: the note says why
            return { enabled: false, reason: "", switchFirst: false }
        if (row.balance === null || row.balance === undefined)
            return { enabled: false, reason: "Its balance isn't known yet.", switchFirst: false }
        if (/^0*$/.test(String(row.balance)))
            return { enabled: false, reason: "Nothing to send from it.", switchFirst: false }
        if (row.canSend || row.kind !== "lez")
            return { enabled: true, reason: "", switchFirst: false }
        if (cliOutdated)
            return { enabled: false, reason: "Your names program sends only from your main accounts. The LEZ 0.4 update lifts this.", switchFirst: false }
        return { enabled: true, reason: "", switchFirst: true }
    }

    /// Send from `row`: from another LEZ account, it becomes the paying one first.
    function sendFrom(row) {
        var state = sendState(row)
        if (!state.enabled)
            return
        if (!state.switchFirst) {
            openSend(row, "")
            return
        }
        accountChange("cli_account_use", [row.account], function() {
            var same = Object.assign({}, row)
            same.canSend = true
            openSend(same, "")
        })
    }

    function loadActivity() {
        askLater("activity", [""], function(value) {
            activityRows = Array.isArray(value) ? value : []
        }, function(message) {
            console.log("names_ui: activity:", message)
        }, 30000)
    }

    /// Whether something in the history is still on its way (worth reading again).
    readonly property bool activityMoving: activityRows.some(function(e) { return e.state === "sending" || (e.kind === "bridge" && e.state === "in_block") })

    Timer {
        interval: 30000
        repeat: true
        running: root.section === "wallet" && root.activityMoving
        onTriggered: { root.loadActivity(); root.loadCliBalance() }
    }

    /// A row's subtitle with only its kind marked: "Private" in brass, the rest dim.
    function subShown(sub) {
        var text = String(sub || "").replace(/&/g, "&amp;").replace(/</g, "&lt;")
        return /^Private/.test(text) ? "<font color=\"" + brass + "\">Private</font>" + text.substr(7) : text
    }

    function amountShown(value) {
        if (walletHidden)
            return "••••••"
        return value === null || value === undefined ? "—" : grouped(value)
    }

    function walletAnyOpen() {
        return walletGroups.some(function(group) { return walletOpen[group.id] !== false })
    }

    function walletSetAll(open) {
        var next = {}
        walletGroups.forEach(function(group) { next[group.id] = open })
        walletOpen = next
        walletRowOpen = ""
    }

    function walletToggle(id) {
        var next = Object.assign({}, walletOpen)
        next[id] = walletOpen[id] === false
        walletOpen = next
    }

    /// Every address that can send, for Send's From choices.
    readonly property var walletRows: walletGroups.reduce(function(rows, group) { return rows.concat(group.rows.filter(function(row) { return row.canSend })) }, [])

    /// Opens the Wallet at `view` (home by default) and reads every balance.
    function openWallet(view) {
        go("wallet")
        walletView = view || "home"
        sendReviewing = false
        walletRefresh()
    }

    function walletRefresh() {
        loadActivity()
        loadCliBalance()
        loadAccounts()
        cliNames.forEach(function(entry) { boardLookUp(entry.name) })
        loadTokens()
        loadWatched()
        loadChain()
        loadFee()
        if (walletNet === "0.4")
            loadBridgeNet()
    }

    /// The Send screen for `asset` (a walletRows row), to `to` if given.
    function openSend(asset, to) {
        sendKind = asset.kind
        sendDefinition = asset.definition || ""
        sendTitle = asset.title
        sendFromPrivate = asset.kind === "chain" ? false : asset.fromPrivate
        if (asset.kind === "chain" && asset.key)
            chainFrom = asset.key
        sendTitle = asset.kind === "chain" ? "Logos Blockchain" : asset.kind === "lez" ? "LEZ" : asset.title
        sendReviewing = false
        chainPlan = null
        chainProblem = ""
        tokenProblem = ""
        go("wallet")
        walletView = "send"
        sendTo.text = to || ""
        sendAmount.text = ""
        if (asset.kind !== "chain")
            loadFee()
        if (to)
            sendAmount.forceActiveFocus()
        else
            sendTo.forceActiveFocus()
    }

    /// Send's first LEZ row: for a name's Pay button.
    function payTo(name) {
        var rows = walletRows.filter(function(row) { return row.id === "lez-private" })
        openSend(rows.length > 0 ? rows[0] : { kind: "lez", title: "LEZ", definition: "", fromPrivate: true }, name)
    }

    function openReceive(asset) {
        receiveKind = asset.kind
        go("wallet")
        walletView = "receive"
    }

    property bool sendFromOpen: false      // Send's From list is open

    function sendRowIsCurrent(row) {
        return row.kind === sendKind && row.definition === sendDefinition && (row.kind === "chain" ? row.key === chainFrom : row.fromPrivate === sendFromPrivate)
    }
    function sendFromRow() {
        var found = walletRows.filter(function(row) { return sendRowIsCurrent(row) })
        return found.length > 0 ? found[0] : null
    }
    function sendRowTitle(row) {
        return row.kind === "chain" ? "Logos Blockchain" : row.kind === "lez" ? "LEZ · " + row.title : row.title
    }

    /// What Send's To means: a bare LEZ account ID, as copied from a wallet, is that public
    /// account (`Public/` added); a name can't have capitals, so it is never one.
    function sendToResolved() {
        var to = sendTo.text.trim()
        if (sendKind !== "chain" && /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to) && /[A-Z]/.test(to))
            return "Public/" + to
        return to
    }

    /// Whether a LEZ or token send goes to a name (so privately, from the private account).
    function sendToName() {
        var to = sendToResolved()
        return to !== "" && !payIsAccount(to)
    }

    /// Why Send can't go as it stands, or "" (also "" while a field is empty).
    function sendProblem() {
        var to = sendToResolved()
        var amount = sendAmount.text.trim()
        if (to === "" || amount === "")
            return ""
        if (sendKind === "chain") {
            var bad = Logic.chainToProblem(to)
            return bad !== "" ? bad : Logic.amountProblem(amount, Logic.CHAIN_AMOUNT_MAX)
        }
        if (/^Private\//.test(to))
            return "A private account can't be paid by its address: nobody but its holder knows its keys. Pay its name, or ask for a public account."
        if (sendKind === "token")
            return tokenSendProblem()
        var bad = (payIsAccount(to) ? payAccountProblem(to) : "") || Logic.amountProblem(amount)
        return bad !== "" ? bad : sendBalanceProblem(Logic.amountDigits(amount))
    }

    /// Why the account Send pays from can't pay `amount` (digits) and its fee, or "" when it
    /// can or its balance isn't known yet (send-failure-text-raw-and-no-balance-check).
    function sendBalanceProblem(digits) {
        var from = payFromPublic ? cliPublic : cliBalance
        if (!/^[0-9]+$/.test(digits) || !from || from.balance === undefined || from.balance === null)
            return ""
        var which = payFromPublic ? "public" : "private"
        if (!Logic.atLeast(String(from.balance), digits))
            return "Your " + which + " account holds " + grouped(from.balance) + ", less than " + grouped(digits) + "."
        if (Logic.atLeast(digits, String(from.balance)))
            return "Your " + which + " account holds exactly " + grouped(digits) + ", and the fee comes on top: send a little less."
        return ""
    }

    /// The Send amount as it will be sent ("1000"), or "" while it isn't an amount.
    function sendDigits() {
        return Logic.amountDigits(sendAmount.text, sendKind === "chain" ? Logic.CHAIN_AMOUNT_MAX : Logic.AMOUNT_MAX)
    }

    /// Shows the summary to confirm; a chain send is checked by the node first.
    function sendReview() {
        if (sendProblem() !== "" || sendToResolved() === "" || sendAmount.text.trim() === "")
            return
        if (sendKind === "chain") {
            chainCheck()
            sendReviewing = true
            return
        }
        sendReviewing = true
    }

    /// Sends what the summary shows.
    function sendConfirm() {
        sendReviewing = false
        if (sendKind === "chain") {
            chainSend()
            return
        }
        if (sendKind === "token") {
            tokenFromPrivate = sendFromPrivate || sendToName()
            tokenSend()
            return
        }
        // On LEZ 0.4 a name is paid from the public account: paying privately isn't offered there.
        payFromPublic = (!sendFromPrivate && !sendToName()) || (namesNet === "0.4" && sendToName())
        if (namesNet === "0.4" && !sendToName()) {
            say("Sending on LEZ 0.4 isn't in the wallet yet: only paying a name is. Switch to LEZ 0.3 to send.", true)
            return
        }
        payName()
    }

    /// What the chain will see, in one line.
    function sendSeen() {
        if (sendKind === "chain")
            return "The Logos Blockchain is public: anyone can see this transfer, and the node's key that sent it."
        if (sendToName())
            return "Private: it goes from your private account to the name's private keys. The chain doesn't show who paid whom, or how much."
        return sendFromPrivate ? "Paying an account is public: everyone sees which account got how much. From your private account, it doesn't show who paid."
            : "Public: everyone can see that you paid, who got it, and how much."
    }

    /// How long it takes, in one line.
    function sendTakes() {
        if (sendKind === "chain")
            return "Your node proves it, up to a minute, then it's in the next blocks."
        var proves = sendToName() || sendFromPrivate
        if (!proves)
            return "One block, about 30 seconds."
        return sendKind === "token" ? "Proved on this computer first: 7-12 minutes." : "Proved on this computer first: usually 5–12 minutes."
    }

    /// The fee, in one line ("" while unknown).
    function sendFee() {
        if (sendKind === "chain")
            return Logic.chainFeeLine(chainPlan)
        var estimate = Logic.feeEstimate(feeQuote, sendFromPrivate || sendToName())
        return estimate === "" ? "" : estimate
    }

    /// What is being sent right now, in one line, or "".
    function sendingLine() {
        if (boardJob !== null && /^cli_pay/.test(boardJob.kind))
            return boardJob.kind === "cli_pay_account" && payingPublicly
                ? boardJob.label + ": sending it publicly, one block (about 30 seconds)."
                : boardJob.label + ": proving it on this computer. Usually 5–12 min, " + Math.max(0, Math.floor((Math.max(clockTick, Date.now()) - boardJob.started) / 60000)) + " so far."
        if (tokenJob !== null)
            return tokenJobLine(tokenJob)
        if (chainBusy)
            return "Your node is working on it…"
        if (chainSent !== null && chainSent.state === "pending")
            return "Sent " + grouped(chainSent.amount) + " on the Logos Blockchain; waiting for a block…"
        return ""
    }

    /// The tokens added by address, and their names.
    function loadWatched() {
        if (tokenProgram === "")
            return
        askLater("watched_tokens", [], function(list) {
            watchedTokens = Array.isArray(list) ? list.map(function(entry) { return entry.definition }) : []
            var names = {}
            ;(list || []).forEach(function(entry) { if (entry.name) names[entry.definition] = entry.name })
            watchedNames = names
        }, function(message) {
            console.log("names_ui: watched tokens:", message)
            if (waitAgain(message))
                againWhenFree("watched", loadWatched)
        })
    }

    /// Adds the token `definition` by address; `program` first when none is set.
    function addToken(definition, program) {
        addTokenProblem = ""
        if (program !== "" && program !== tokenProgram)
            saveTokenProgram(program)
        if (tokenProgram === "") {
            addTokenProblem = "Enter the token program's ID too: no token program runs on the public testnet yet."
            return
        }
        if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(definition)) {
            addTokenProblem = "A token's address is its ID on LEZ: 32 to 44 letters and digits."
            return
        }
        addTokenBusy = true
        askLater("watch_token", [definition, true], function(list) {
            addTokenBusy = false
            addTokenField.text = ""
            loadWatched()
            loadTokens()
            var added = (list || []).filter(function(entry) { return entry.definition === definition })
            say("Added " + (added.length > 0 && added[0].name ? Logic.tokenShown(added[0].name) : "the token") + " to your wallet.", false)
            walletView = "home"
        }, function(message) {
            addTokenBusy = false
            addTokenProblem = sentence(message)
        }, 60000)
    }

    function removeToken(definition) {
        askLater("watch_token", [definition, false], function() { loadWatched() }, function(message) { say(sentence(message), true) })
    }

    /// Puts the node's key on your name as its `blockchain` entry, so it can be paid by name.
    function keyOnName(key) {
        if (mainName === "")
            return
        var name = mainName
        change("set_text", [name, "blockchain", key], name, function() {
            return ask("resolve", [name]).texts["blockchain"] === key
        }, "Saved: " + name + " can now be paid on the Logos Blockchain.")
    }

    // ── Get tokens ──────────────────────────────────────────────────────

    readonly property string defaultBridgeNode: "http://127.0.0.1:8080"
    /// The CLI's default public account, which bridging credits: {account, balance}, or null.
    readonly property var cliPublic: cliBalance !== undefined && cliBalance !== null && cliBalance["public"] ? cliBalance["public"] : null

    /// Opens the "Get tokens" screen.
    function openGetTokens(fromKey) {
        go("wallet")
        walletView = "bridge"
        loadChain()
        loadAccounts()
        bridgePlan = null
        bridgePlanFor = ""
        bridgeConfirm = false
        bridgeError = ""
        bridgeSplitHash = ""
        bridgeSplitTries = 0
        bridgeSent = false
        bridgeHash = ""
        // The key it was opened from, else the one used last, else the node's first.
        bridgeFromKey = fromKey ? fromKey : bridgeKey !== "" ? bridgeKey : chainUsable !== null && chainUsable.length > 0 ? chainUsable[0].key : ""
        bridgeFromLez = ""
        bridgeTo = ""
        bridgeToPrivate = false
        bridgeFromOpen = false
        bridgeToOpen = false
        bridgeOtherNode = bridgeNode !== ""
        bridgeNodeField.text = bridgeNode
        loadCliBalance()
    }

    /// The Bridge screen opened from a LEZ account: LEZ to the Logos Blockchain.
    function openBridgeFromLez(account) {
        openGetTokens("")
        bridgeFromLez = account
    }

    /// The LEZ account the Bridge was opened from, in words, and what it holds.
    function bridgeLezEntry() {
        var found = (cliAccounts || []).filter(function(entry) { return entry.account === bridgeFromLez })
        return found.length > 0 ? found[0] : null
    }
    function bridgeLezTitle() {
        var entry = bridgeLezEntry()
        return entry !== null ? Logic.accountTitle(entry) : Logic.keyShown(bridgeFromLez.replace(/^(Public|Private)\//, ""))
    }
    function bridgeLezAmount() {
        var entry = bridgeLezEntry()
        return entry !== null && entry.balance !== null ? amountShown(entry.balance) : ""
    }

    /// Whether the amount typed is below what a move to private costs (LEZ 0.4).
    function bridgeAmountShort() {
        var asked = bridgeAsked()[0]
        return !cliOutdated && /^[0-9]+$/.test(asked) && !Logic.atLeast(asked, shieldMinimum)
    }

    /// What the Move to private card moves: what was typed, else what was bridged.
    function privateMoveAmount() {
        return privateMoveField.text.trim() !== "" ? Logic.amountDigits(privateMoveField.text) : bridgeAsked()[0]
    }

    property bool bridgeReview: false    // Review was pressed: confirm at once if the plan is ready
    property bool bridgeFromOpen: false  // the From list is open
    property bool bridgeToOpen: false    // the To list is open

    /// What the picked node key holds, in words.
    function bridgeFromAmount() {
        var found = (chainAccounts || []).filter(function(entry) { return entry.key === bridgeFrom })
        return found.length > 0 ? amountShown(found[0].balance) : ""
    }

    /// The picked LEZ public account: its title and what it holds.
    function bridgeToEntry() {
        var rows = bridgeTargets
        var found = rows.filter(function(row) { return row.account === bridgeTo })
        return found.length > 0 ? found[0] : rows.length > 0 ? rows[0] : null
    }
    function bridgeToTitle() {
        var entry = bridgeToEntry()
        return entry !== null ? entry.title : "Public"
    }
    function bridgeToAmount() {
        var entry = bridgeToEntry()
        return entry !== null ? amountShown(entry.balance) : ""
    }

    /// Makes a new public LEZ account and picks it as where the bridge lands.
    function bridgeToNewAccount() {
        accountChange("cli_account_new", ["public", "Bridged"], function(value) {
            if (value && value.account)
                bridgeTo = value.account
            bridgeToPrivate = false
            say("Made a new public account for the bridge.", false)
        })
    }

    /// What the fields ask for, so a plan is only sent for what it was checked for.
    property string bridgeFromLez: ""    // a LEZ account picked as the source: withdrawals, not yet possible
    property bool bridgeToPrivate: false // land in the default public account, then move it to private
    property string bridgeFromKey: ""    // the node key the bridge sends from, picked
    property string bridgeTo: ""         // the LEZ public account it credits, picked ("" = the default)
    property bool bridgeOtherNode: false // the node address field is open

    /// The LEZ public accounts the bridge can credit: [{account, title, balance}].
    readonly property var bridgeTargets: {
        var rows = (cliAccounts || []).filter(function(entry) { return entry.kind === "public" && (entry.balance !== "0" || entry["default"] || entry.account === bridgeTo) })
        if (rows.length === 0 && cliPublic !== null)
            rows = [{ account: "Public/" + cliPublic.account, label: null, balance: cliPublic.balance, "default": true }]
        rows = rows.slice().sort(function(a, b) { return (b["default"] ? 1 : 0) - (a["default"] ? 1 : 0) })
        return rows.map(function(entry) { return { account: entry.account, title: entry["default"] ? "Public account (your main one)" : "Public account · " + Logic.keyShown(entry.account.substr(7)), balance: entry.balance } })
    }

    /// The node key the bridge sends from: the one picked, if the node holds it, else its first.
    readonly property string bridgeFrom: {
        var keys = (chainUsable || []).map(function(entry) { return entry.key })
        return keys.indexOf(bridgeFromKey) >= 0 ? bridgeFromKey : keys.length > 0 ? keys[0] : ""
    }

    function bridgeAsked() {
        return [Logic.amountDigits(bridgeAmountField.text, Logic.CHAIN_AMOUNT_MAX), bridgeFrom, bridgeNodeField.text.trim(), bridgeToPrivate ? "" : bridgeTo]
    }

    /// Checks the fields before the module sees them; "" if they look right.
    function bridgeInputProblem(asked) {
        if (asked[0] === "")
            return Logic.amountProblem(bridgeAmountField.text, Logic.CHAIN_AMOUNT_MAX)
        if (bridgeSplitHash !== "")
            return "Wait for the note to be made first."
        if (!/^[0-9a-f]{64}$/.test(asked[1]))
            return "Choose the node key to send from. If none is listed, start your node in Basecamp's Blockchain app."
        if (asked[2] !== "" && !/^https?:\/\/(127\.0\.0\.1|localhost):[0-9]{1,5}\/?$/.test(asked[2]))
            return "The node is an address on this computer, such as " + defaultBridgeNode + "."
        return ""
    }

    /// Runs `names bridge`: the plan, or with `send` the deposit itself.
    function bridgeRun(send) {
        var asked = bridgeAsked()
        var problem = bridgeInputProblem(asked)
        bridgeError = problem
        if (!send || problem !== "")
            bridgeConfirm = false
        if (problem !== "")
            return
        if (send && bridgePlanFor !== asked.join("|")) {
            bridgeConfirm = false
            return
        }
        bridgeBusy = true
        if (!send) {
            bridgePlan = null
            bridgeHash = ""
            bridgeSent = false
        }
        // Through Basecamp's own node, one note of exactly the amount (chain_bridge).
        askLater("chain_bridge", [asked[1], asked[3], asked[0], send], function(value) {
            bridgeBusy = false
            bridgeConfirm = false
            bridgeKey = asked[1]
            bridgeNode = asked[2] === defaultBridgeNode ? "" : asked[2].replace(/\/$/, "")
            if (send) {
                bridgePlan = null
                bridgePlanFor = ""
                bridgeHash = value && value.hash ? value.hash : ""
                bridgeSent = true
                bridgeSentLines = value && value.lines ? value.lines : []
                bridgeAmountField.text = ""
            } else {
                bridgePlan = value
                bridgePlanFor = asked.join("|")
                // Review: a plan that can go straight away opens its confirmation.
                if (bridgeReview && bridgePlanReady())
                    bridgeConfirm = true
                bridgeReview = false
            }
        }, function(message) {
            bridgeBusy = false
            bridgeConfirm = false
            bridgeError = sentence(message)
        }, 150000)
    }

    property bool networkConfirm: false   // Advanced: switching registry or sequencer awaits a yes
    property string bridgeSplitHash: ""  // the split under way: a send from the key to itself
    property int bridgeSplitTries: 0      // checks of it so far; it stops after about 4 minutes

    /// Whether the checked plan can be sent: an exact note, a channel the node knows, a fee.
    function bridgePlanReady() {
        return bridgePlan !== null && !bridgePlan.needs_split && bridgePlan.channel_known !== false && bridgePlan.fee_short !== true
    }

    /// Splits the amount off into a note of its own, then checks the bridge again once it lands.
    function bridgeSplit() {
        var asked = bridgeAsked()
        bridgeError = ""
        bridgeBusy = true
        askLater("chain_split", [asked[1], asked[0], true], function(value) {
            bridgeBusy = false
            bridgeSplitTries = 0
            bridgeSplitHash = value && value.hash ? value.hash : ""
            if (bridgeSplitHash === "")
                bridgeRun(false)
        }, function(message) {
            bridgeBusy = false
            bridgeError = sentence(message)
        }, 240000)
    }

    Timer {
        interval: 4000
        repeat: true
        running: root.bridgeSplitHash !== ""
        onTriggered: {
            root.bridgeSplitTries += 1
            if (root.bridgeSplitTries > 60) {
                root.bridgeSplitHash = ""
                root.bridgeError = "The note hasn't shown up in your node's wallet yet. Press Check in a few minutes: nothing is lost."
                return
            }
            root.askLater("chain_tx", [root.bridgeSplitHash], function(tx) {
                if (tx.state === "in_block") {
                    root.bridgeSplitHash = ""
                    root.bridgeRun(false)
                }
            }, function(message) { console.log("names_ui: split:", message) }, 20000)
        }
    }

    /// The checked plan in plain words.
    function bridgePlanText() {
        if (bridgePlan === null)
            return ""
        var asked = bridgeAsked()[0]
        if (bridgePlan.channel_known === false)
            return "Your node doesn't know LEZ's deposit channel (" + Logic.keyShown(bridgePlan.channel || "") + "). It may still be syncing, or your sequencer is on another network. Nothing can be bridged until it does."
        if (bridgePlan.needs_split)
            return "No note on that key holds exactly " + grouped(asked) + ", and a bridge deposits whole notes. First split "
                + grouped(asked) + " off into a note of its own: a send from the key to itself (" + (/^\d+$/.test(String(bridgePlan.split_fee_about)) ? "a fee of about " + grouped(bridgePlan.split_fee_about) + ", your node's figure" : "a fee of about 2,000") + ", in a block within a minute). "
                + "The rest comes back as change. The notes it spends leave the stake, and the new ones join it within two epochs."
        if (bridgePlan.fee_short === true)
            return "That key holds a note of exactly " + grouped(asked) + ", but the deposit fee (at most " + grouped(bridgePlan.fee || "100000")
                + ") comes from its other notes, which hold " + grouped(bridgePlan.others || "0") + ". Mine or receive a little more first."
        return "Ready: exactly " + grouped(asked) + " from one note of that key, into " + (bridgeTo !== "" ? Logic.keyShown(bridgeTo.substr(7)) : "your LEZ public account")
            + ". Network fee " + (/^\d+$/.test(String(bridgePlan.fee_about)) ? "about " + grouped(bridgePlan.fee_about) + " (your node's figure; at most " + grouped(bridgePlan.fee || "100000") + ")" : "at most " + grouped(bridgePlan.fee || "100000")) + ", paid from the key's other notes. Nothing has been sent yet."
    }

    /// Whether the plan deposits more than twice what was asked (whole notes): worth a warning.
    function bridgeOvershoots() {
        if (bridgePlan === null || !bridgePlan.total)
            return false
        var asked = bridgeAsked()[0]
        return /^[0-9]+$/.test(asked) && Logic.atLeast(bridgePlan.total, Logic.addDigits(asked, asked)) && bridgePlan.total !== Logic.addDigits(asked, asked)
    }

    // ── Getting started: install, wallet, move to private ───────────────

    property var cliStatus: null        // cli_status: {cli, prover, installing, wallet_dir, wallet, public, private, busy}; null until known
    // The LEZ 0.4 tool can't try a public call here first (the shipped one, no r0vm): each
    // public change says once, where it is confirmed, what a refused one still costs.
    readonly property var toolNow: namesNet === "0.4" && cliStatus !== null && cliStatus.tool ? cliStatus.tool : null
    property bool toolAutoTried: false   // the shipped LEZ 0.4 tool was offered to the module this session
    property var installJob: null       // the install under way: {id, started, line}
    property string installProblem: ""
    property string installWarning: ""
    property bool walletBusy: false     // "Create my wallet" is waiting on the module
    property string walletError: ""
    // A new wallet's recovery phrase, shown once until "I've written it down".
    // Never logged, saved or put on the clipboard.
    property string recoveryPhrase: ""
    property var shieldJob: null        // the move to private under way: {id, started}
    property string shieldProblem: ""
    property bool shieldMoved: false
    /// What a name's deposit takes.
    readonly property string nameDeposit: "1000000"
    /// The least worth moving to private: on LEZ 0.4 each private transaction reserves its
    /// fee in the proof, about 5,100,000, and registering takes two, plus the deposit. An
    /// older names program (LEZ 0.3, no private fees) moves its own default, 5,000,000.
    readonly property string shieldMinimum: cliOutdated ? "5000000" : "12000000"
    /// What a new wallet needs from the Names faucet: enough to register one name privately.
    readonly property string faucetAmount: shieldMinimum
    /// Everything the public account holds moves to private: names and payments use it there.
    readonly property string shieldAmount: cliPublic !== null && Logic.atLeast(cliPublic.balance, shieldMinimum) ? String(cliPublic.balance) : shieldMinimum
    // Free tokens from the Names faucet (`cli_faucet`), for a new wallet's public account.
    property bool faucetBusy: false
    property string faucetError: ""
    property bool faucetSent: false      // the faucet sent them; waiting for them to arrive
    property double faucetAt: 0
    /// The names CLI's wallet exists and has its accounts.
    readonly property bool hasCliWallet: cliStatus !== null && !!cliStatus.wallet && !!cliStatus["public"]
    /// Your public account's ID, as far as it is known.
    readonly property string myPublic: cliPublic !== null ? cliPublic.account : hasCliWallet ? cliStatus["public"] : ""
    readonly property bool canShield: cliPublic !== null && Logic.atLeast(cliPublic.balance, shieldMinimum)
    /// A wallet from before LEZ 0.4, which no longer opens.
    readonly property bool oldWallet: cliStatus !== null && !!cliStatus.old_wallet
    property bool restoreOpen: false    // Welcome: "Restore from my recovery phrase" was chosen

    // ── Settings → Backup: one encrypted file of the wallet folders (names module backup_*).
    //    The password is read from its field at the moment of the call and the field is cleared;
    //    it is never kept in this app's state, a log or the last reply. ──
    property var backupStatus: null
    property string backupMode: ""          // "" | "make" | "restore"
    property string backupWhich: "both"     // "this" | "both"
    property string backupWhere: ""         // a chosen file path; "" = the default folder
    property bool backupBusy: false
    property string backupError: ""
    property var backupMade: null           // backup_make's value
    property string restoreFile: ""
    property var restoreInside: null        // backup_open's value
    property var restoreResult: null
    property bool backupDetails: false
    function backupWhen(seconds) {
        return Qt.formatDateTime(new Date(Number(seconds) * 1000), "d MMM, hh:mm")
    }
    function loadBackupStatus() {
        askLater("backup_status", [], function(value) { backupStatus = value }, function(message) {
            console.log("names_ui: backup_status:", message)
        })
    }
    /// The typed passwords go whenever the Backup screen opens again or Settings is left.
    function clearBackupPasswords() {
        try { backupPw1.text = ""; backupPw2.text = ""; restorePw.text = "" } catch (e) {}
    }
    function openBackup(mode) {
        go("settings")
        clearBackupPasswords()
        backupMode = mode
        backupError = ""
        backupMade = null
        restoreInside = null
        restoreResult = null
        backupDetails = false
        loadBackupStatus()
    }
    function chooseBackupWhere() {
        try {
            var d = Qt.createQmlObject('import QtQuick.Dialogs; FileDialog { title: "Save the backup as"; fileMode: FileDialog.SaveFile; defaultSuffix: "lnsbackup"; nameFilters: ["Names Wallet backups (*.lnsbackup)"] }', root, "backupSave")
            d.accepted.connect(function() { backupWhere = Logic.dialogPath(d.selectedFile); d.destroy() })
            d.rejected.connect(function() { d.destroy() })
            d.open()
        } catch (error) {
            backupWhere = ""
            backupError = "This Basecamp has no save dialog here: the backup goes to the folder shown."
        }
    }
    function makeBackup(password) {
        var nets = Logic.backupNetworks(backupWhich, namesNet === "0.4" ? "lez-0.4" : "lez-0.3", backupStatus)
        if (backupBusy || nets === "")
            return
        backupBusy = true
        backupError = ""
        backupMade = null
        askLater("backup_make", [nets, password, backupWhere], function(value) {
            backupBusy = false
            backupMade = value
            loadBackupStatus()
        }, function(message) {
            backupBusy = false
            backupError = sentence(message)
        }, 120000)
    }
    function chooseRestoreFile() {
        try {
            var d = Qt.createQmlObject('import QtQuick.Dialogs; FileDialog { title: "Choose the backup file"; nameFilters: ["Names Wallet backups (*.lnsbackup)", "All files (*)"] }', root, "backupOpen")
            d.accepted.connect(function() { restoreFile = Logic.dialogPath(d.selectedFile); restoreInside = null; d.destroy() })
            d.rejected.connect(function() { d.destroy() })
            d.open()
        } catch (error) {
            backupError = "This Basecamp has no file dialog here: paste the backup file's full path."
        }
    }
    function openRestoreFile(password) {
        if (backupBusy || restoreFile === "")
            return
        backupBusy = true
        backupError = ""
        restoreInside = null
        askLater("backup_open", [restoreFile, password], function(value) {
            backupBusy = false
            restoreInside = value
        }, function(message) {
            backupBusy = false
            backupError = sentence(message)
        }, 60000)
    }
    function restoreFromBackup(password, network) {
        if (backupBusy)
            return
        backupBusy = true
        backupError = ""
        askLater("backup_restore", [restoreFile, password, network], function(value) {
            backupBusy = false
            restoreResult = value
            restoreInside = null
            loadBackupStatus()
            loadConfig()
        }, function(message) {
            backupBusy = false
            backupError = sentence(message)
        }, 60000)
    }
    /// A names program from before LEZ 0.4, which can't sign for this app.
    readonly property bool cliOutdated: cliStatus !== null && !!cliStatus.outdated
    property bool restoreBusy: false
    property string restoreError: ""
    property string restoreDone: ""
    property bool claimBusy: false
    property var claimJob: null         // the claim under way: {id, name}
    property string claimLine: ""
    /// When names from the earlier chain stop being claimable, in chain time, or null.
    property var claimsUntil: null
    readonly property bool hasPrivateFunds: cliBalance !== undefined && cliBalance !== null && cliBalance.balance !== null && cliBalance.balance !== undefined && !/^0*$/.test(String(cliBalance.balance))

    /// Reads where the CLI stands (nothing is run), and picks up an install
    /// or a move to private that was under way.
    // While Welcome is up, what it shows is read again every 15 s, so a passing busy or failed
    // read (or a change made elsewhere) never leaves it showing the wrong step.
    Timer {
        interval: 15000
        repeat: true
        running: root.bridgeReady && root.configKnown && root.needsWelcome
        onTriggered: { root.loadCliStatus(); root.loadCliMine() }
    }

    function loadCliStatus() {
        // The network it was asked on: an answer that comes back after a switch is dropped, and
        // nothing read on one network (its public account) is kept under the other (S-A).
        var net = namesNet
        askLater("cli_status", [], function(value) {
            if (net !== namesNet)
                return
            var before = cliStatus
            cliStatus = Logic.mergeCliStatus(cliStatus, Logic.onNet(value, net))
            if (before && before.wallet && !(cliStatus && cliStatus.wallet))
                console.log("names_ui: the module says there is no wallet now:", JSON.stringify(value))
            if (value && value.installing && installJob === null)
                installJob = { id: value.installing.id, started: value.installing.started || Date.now(), line: installLine(value.installing.log_tail) }
            // LEZ 0.4 with no tool at all: the one shipped inside Names goes in by itself.
            if (net === "0.4" && value && !value.cli && cliPath === "" && !toolAutoTried && installJob === null) {
                toolAutoTried = true
                if (installedList === null)
                    loadInstalled(function() { installBundledTool(true) })
                else
                    installBundledTool(true)
            }
            if (value && value.busy && value.busy.kind === "shield" && shieldJob === null)
                shieldJob = { id: value.busy.id, started: value.busy.started || Date.now() }
        }, function(message) {
            console.log("names_ui: CLI status:", message)
        })
    }

    /// LEZ 0.4: the tool shipped inside Names, put in place by the module (checksum, then its
    /// version). `auto`: on opening, with no tool set (nothing said when it can't).
    function installBundledTool(auto) {
        var path = Logic.bundledPath(installedList, Logic.BUNDLED_TOOL.file)
        if (installJob !== null || path === "" || moduleFeatures.indexOf("bundled_tool") < 0) {
            if (!auto)
                installProblem = "This Names package has no LEZ 0.4 names tool inside. Install the Names package again."
            return
        }
        installProblem = ""
        installJob = { id: "", started: Date.now(), line: "Installing the LEZ 0.4 names tool…" }
        askLater("cli_install_bundled", [path, Logic.BUNDLED_TOOL.sha256], function(value) {
            installJob = null
            loadConfig()
            loadCliStatus()
        }, function(message) {
            installJob = null
            // Opening Names with no tool: a module that didn't answer is asked again next time.
            if (auto && (message === moduleNoAnswer || message === moduleBusy))
                toolAutoTried = false
            else
                installProblem = sentence(message)
        }, 60000)
    }

    /// Installs the names program and its prover into the module's folder (on LEZ 0.4, the
    /// tool shipped inside Names; the full download only in a build that allows it).
    function installCli() {
        if (namesNet === "0.4" && !Logic.FULL_TOOL_DOWNLOAD) {
            installBundledTool(false)
            return
        }
        installFullTool()
    }
    function installFullTool() {
        if (installJob !== null)
            return
        installProblem = ""
        installWarning = ""
        installJob = { id: "", started: Date.now(), line: "Starting…" }
        askLater("cli_install", [], function(value) {
            if (value.installed) {
                installJob = null
                installWarning = value.prover || namesNet === "0.4" ? "" : "The prover isn't installed, so private changes can't be made yet."
                loadConfig()
                return
            }
            installJob = { id: value.job.id, started: value.job.started || Date.now(), line: "Starting…" }
        }, function(message) {
            installJob = null
            installProblem = sentence(message)
        }, 60000)
    }

    /// What an install is doing, from the last line of its log.
    function installLine(log) {
        var last = lastLogLine(log)
        if (/^Downloading names/.test(last))
            return "Downloading names…"
        if (/^Downloading RISC Zero/.test(last))
            return "Downloading the prover, about 70 MB…"
        if (/^Installed .*(names|man pages)/.test(last))
            return "Checking the download…"
        return "Installing…"
    }

    function checkInstall() {
        var job = installJob
        if (job === null || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (installJob !== job)
                return
            if (value.state === "running") {
                installJob = { id: job.id, started: job.started, line: installLine(value.log_tail) }
                return
            }
            installJob = null
            if (value.state === "done") {
                installWarning = value.warning ? value.warning : ""
                loadConfig()
            } else {
                installProblem = Logic.jobProblem(value, "The install stopped")
            }
        }, function() { job.asking = false })
    }

    /// Makes the wallet and its two accounts. A new wallet's recovery phrase
    /// comes back once, to show.
    function createWallet() {
        if (walletBusy)
            return
        walletBusy = true
        walletError = ""
        askLater("cli_create_wallet", [], function(value) {
            walletBusy = false
            if (value && value.phrase)
                recoveryPhrase = value.phrase
            if (value && value.problem)
                walletError = sentence(value.problem)
            loadCliStatus()
            loadCliBalance()
            loadCliMine()
        }, function(message) {
            walletBusy = false
            walletError = sentence(message)
        }, 200000)
    }

    /// Brings back a wallet from before LEZ 0.4 from its recovery phrase. Tokens stay
    /// behind with the old chain; the accounts come back, so names can be claimed.
    function restoreWallet(phrase) {
        if (restoreBusy)
            return
        restoreBusy = true
        restoreError = ""
        askLater("cli_restore_wallet", [phrase], function(value) {
            restoreBusy = false
            restoreOpen = false
            restoreDone = oldWallet ? "Your wallet is back. The old files were kept beside it." : "Your wallet is restored: the same public accounts as the wallet this phrase made."
            loadCliStatus()
            loadCliBalance()
            loadCliMine()
        }, function(message) {
            restoreBusy = false
            restoreError = sentence(message)
        }, 200000)
    }

    /// Learns whether names from the earlier chain can still be claimed.
    function loadClaims() {
        askLater("settings", [], function(value) {
            claimsUntil = value && value.claims_until !== undefined ? value.claims_until : null
            lockedUpTo = value && value.locked_up_to ? Number(value.locked_up_to) : 0
        }, function(message) {
            console.log("names_ui: settings:", message)
            if (waitAgain(message))
                againWhenFree("settings", loadClaims)
        })
    }

    /// Takes back a name held on the earlier chain, as a job of the CLI.
    function claimName(name) {
        if (claimBusy)
            return
        claimBusy = true
        claimLine = "Taking back " + name + "."
        askLater("cli_claim", [name, ""], function(value) {
            claimJob = { id: value.id, name: name }
        }, function(message) {
            claimBusy = false
            claimLine = sentence(message)
        }, 60000)
    }

    function checkClaim() {
        var job = claimJob
        if (job === null)
            return
        askLater("job", [job.id], function(value) {
            if (claimJob !== job || value.state === "running")
                return
            claimJob = null
            claimBusy = false
            claimLine = value.state === "done" ? job.name + " is yours again." : sentence(Logic.jobProblem(value, "Taking the name back didn't finish"))
            loadCliMine()
        }, function() {})
    }

    Timer {
        interval: 3000
        repeat: true
        running: root.claimJob !== null
        onTriggered: root.checkClaim()
    }

    /// Moves tokens from the public account to the private one (a proof of a few minutes).
    /// Asks the faucet for free tokens, then checks the balance until they arrive.
    function getFreeTokens() {
        if (faucetBusy)
            return
        faucetBusy = true
        faucetError = ""
        askLater("cli_faucet", [], function(value) {
            faucetBusy = false
            faucetSent = true
            faucetAt = Date.now()
        }, function(message) {
            faucetBusy = false
            faucetError = sentence(message)
        }, 90000)
    }

    Timer {
        // While free tokens are on their way: look every 15 seconds, for up to 10 minutes.
        interval: 15000
        repeat: true
        running: root.faucetSent && !root.canShield && Date.now() - root.faucetAt < 600000
        onTriggered: root.loadCliBalance()
    }

    function moveToPrivate(amount) {
        if (shieldJob !== null)
            return
        shieldProblem = ""
        shieldMoved = false
        shieldJob = { id: "", started: Date.now() }
        askLater("cli_shield", [amount ? String(amount) : shieldAmount], function(value) {
            shieldJob = { id: value.id, started: value.started || Date.now() }
        }, function(message) {
            shieldJob = null
            shieldProblem = sentence(message)
        })
    }

    /// What `names balance` says waits to be claimed: {count, total}, or null.
    readonly property var waitingPayments: cliBalance && cliBalance.waiting ? cliBalance.waiting : null
    property var receiveJob: null        // the claim under way: {id, started}
    property string receiveProblem: ""

    function claimPayments() {
        if (receiveJob !== null)
            return
        receiveProblem = ""
        receiveJob = { id: "", started: Date.now() }
        askLater("cli_receive", [], function(value) {
            receiveJob = { id: value.id, started: value.started || Date.now() }
        }, function(message) {
            receiveJob = null
            receiveProblem = sentence(message)
        })
    }

    function checkReceive() {
        var job = receiveJob
        if (job === null || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (receiveJob !== job || value.state === "running")
                return
            receiveJob = null
            if (value.state === "done")
                say("Claimed. The payments are in your private account now.", false)
            else
                receiveProblem = Logic.jobProblem(value, "Claiming didn't finish")
            loadCliBalance()
        }, function() { job.asking = false })
    }

    function checkShield() {
        var job = shieldJob
        if (job === null || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (shieldJob !== job || value.state === "running")
                return
            shieldJob = null
            if (value.state === "done" && value.moved)
                shieldMoved = true
            else
                shieldProblem = Logic.jobProblem(value, "Moving to private didn't go through")
            loadCliBalance()
        }, function() { job.asking = false })
    }

    /// How the move to private is going, in a line.
    function shieldLine() {
        if (shieldJob === null)
            return ""
        var minutes = Math.max(0, Math.floor((Math.max(clockTick, Date.now()) - shieldJob.started) / 60000))
        return "Proving it on this computer, so the chain can't link your two accounts. Usually 5–12 min" + (minutes > 0 ? ", " + minutes + " so far." : ".")
    }

    Timer {
        interval: 3000
        repeat: true
        running: root.installJob !== null || root.shieldJob !== null || root.receiveJob !== null
        onTriggered: {
            root.clockTick = Date.now()
            root.checkInstall()
            root.checkShield()
            root.checkReceive()
        }
    }

    /// Whether the CLI's private account can pay the deposit, as far as is known.
    function cliCanPay() {
        if (cliBalance === undefined || cliBalance === null || deposit === null)
            return cliBalance === undefined
        var need = deposit.deposit !== null && deposit.deposit !== undefined ? deposit.deposit : deposit.fee
        if (need === undefined || need === null)
            return true
        var have = String(cliBalance.balance), want = String(need)
        return have.length !== want.length ? have.length > want.length : have >= want
    }

    /// The CLI's registration of `name` still finishing (or stopped), or null.
    function cliPendingFor(name) {
        for (var i = 0; i < cliPending.length; i++)
            if (cliPending[i].name === name)
                return cliPending[i]
        return null
    }

    function cliHolds(name) {
        for (var i = 0; i < cliNames.length; i++)
            if (cliNames[i].name === name)
                return true
        return false
    }

    /// Reads the names this device remembers that the open wallet holds. The
    /// chain keeps no list of names, so this is the module's own memory.
    function loadMine() {
        if (registry === "") {
            myNames = []
            pendingNames = []
            return
        }
        // Asked even while the wallet read is failing: the registrations under way are the
        // module's own (pending.json), and their reveals must go out whatever else is slow.
        askLater("mine", [], onNet(function(value) {
            myNames = value.names
            pendingNames = value.pending ? value.pending : []
            if (pendingNames.length > 0)
                revealTimer.start()
        }), function(message) {
            console.log("names_ui: my names:", message)
            // Busy or unanswered: asked again, so the reveals don't stop (SF-D).
            if (Logic.askAgainAfter(message, moduleBusy, moduleNoAnswer))
                againWhenFree("mine", loadMine)
        })
    }

    /// The wallet account that holds a name, or "".
    function holderOf(name) {
        for (var i = 0; i < myNames.length; i++)
            if (myNames[i].name === name)
                return myNames[i].owner
        return ""
    }

    /// A chain time in milliseconds as a time of day.
    function clock(milliseconds) {
        return Qt.formatTime(new Date(milliseconds), "HH:mm")
    }

    /// Registration is two steps: a commitment, then, 15 minutes to 24 hours
    /// later, the reveal. Asking `register` again sends the reveal once it is
    /// due and answers with the stage. This asks for every registration that
    /// is due, one at a time, until each is registered.
    function revealDue() {
        if (bridgeReady === false)
            return
        var item = Logic.revealNext(pendingNames, Date.now(), revealing ? revealAskedAt : 0, 200000, revealRefused)
        if (item !== null) {
            revealing = true
            revealAskedAt = Date.now()
            // Reveal only: never a new commitment (the module refuses one that isn't kept here).
            askLater("reveal_name", [item.name, item.owner, root.namesNet === "0.4" ? "" : "auto"], function(value) {
                revealing = false
                if (value && value.stage === "registered") {
                    say(item.name + " is yours.", false)
                    if (root.namesNet === "0.4")
                        publishAfterRegister(item.name, item.owner)
                    loadMine()
                    if (record !== null && record.name === item.name)
                        read(item.name)
                } else {
                    loadMine()
                }
            }, function(message) {
                revealing = false
                console.log("names_ui: reveal:", message)
                // Refused (not busy, not unanswered): this one waits, the others go ahead (SF-A).
                if (!waitAgain(message)) {
                    var refused = revealRefused
                    refused[item.name] = Date.now() + Logic.REVEAL_REFUSED_RETRY_MS
                    revealRefused = refused
                }
                // Read the registrations again: a failed ask never drops one.
                loadMine()
            }, 180000)
        }
    }
    property real revealAskedAt: 0

    /// LEZ 0.4, a name just registered: its holder account published as its public account, unless
    /// the person unticked that when registering (kept in the module: `publish:<name>`).
    function publishAfterRegister(name, owner) {
        var go = function() { publishAccount(name, owner) }
        if (moduleFeatures.indexOf("app_note") < 0)
            return go()
        askLater("app_note", ["publish:" + name, ""], function(value) {
            if (String(value || "") !== "no")
                go()
        }, go, 10000)
    }
    /// Publishes `account` as the public account of `name` (set_account), so it can be paid.
    function publishAccount(name, account) {
        if (account === "")
            return
        say("Publishing the account people pay " + name + " at…", "working")
        askLater("set_account", [name, account], function() {
            say("People can pay " + name + " at your account " + Logic.idShort(account) + ".", false)
            loadMine()
        }, function(message) {
            say("The account of " + name + " wasn't published: " + sentence(message) + " Names → Let people pay it, to try again.", true)
            loadMine()
        }, 180000)
    }
    // Names whose reveal the module refused this session, and when to ask again (SF-A).
    property var revealRefused: ({})

    // A busy wallet read is asked again after a short wait (loadWallet).
    Timer {
        id: walletRetry
        interval: 5000
        onTriggered: root.loadWallet(false)
    }

    Timer {
        id: revealTimer
        interval: 30000
        repeat: true
        running: root.pendingNames.length > 0
        onTriggered: root.revealDue()
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.pending !== null || root.pendingNames.length > 0
        onTriggered: root.clockTick = Date.now()
    }

    // The names CLI finishes a private registration by itself; this notices
    // when it has, and shows the name once it is registered.
    Timer {
        interval: 60000
        repeat: true
        running: root.cliPending.some(function(entry) { return entry.registering })
        onTriggered: {
            var waiting = root.missing !== "" && root.cliPendingFor(root.missing) !== null ? root.missing : ""
            root.loadCliMine()
            if (waiting !== "")
                root.show(waiting)
        }
    }

    function remembers(name) {
        for (var i = 0; i < myNames.length; i++)
            if (myNames[i].name === name)
                return true
        return false
    }

    /// Reads the wallet's accounts that can pay a fee. Without them the app
    /// still browses; `walletProblem` says what is missing. `announce` shows
    /// the outcome, for when a person asked.
    function loadWallet(announce) {
        if (registry === "")
            return
        askLater("wallet", [], onNet(function(value) {
            var funded = value.accounts.filter(function(account) { return account.funded })
            wallet = funded
            privateWallet = (value.private_accounts || []).filter(function(account) { return account.funded })
            if (senderIndex >= funded.length + privateWallet.length)
                senderIndex = 0
            walletProblem = funded.length > 0 ? "" : "None of the wallet's accounts has funds."
            if (fromIndex >= funded.length)
                fromIndex = 0
            console.log("names_ui: wallet has", funded.length, "funded account(s)")
            loadMine()
            if (announce)
                say(funded.length > 0 ? "Wallet connected." : walletProblem, funded.length === 0)
        }), function(message) {
            if (waitAgain(message)) {
                // Busy is not "no wallet": keep what was read, and ask again soon.
                console.log("names_ui: wallet read waits:", message)
                walletRetry.restart()
                return
            }
            wallet = []
            privateWallet = []
            walletProblem = sentence(message)
            fromIndex = 0
            myNames = []
            console.log("names_ui: no wallet:", message)
            if (announce)
                say(walletProblem, true)
        })
    }

    function holds(account, accounts) {
        for (var i = 0; i < accounts.length; i++)
            if (accounts[i].account === account)
                return true
        return false
    }

    /// Shows what was typed: a name, or the name an account ID goes by.
    function find(typed) {
        var text = String(typed).trim()
        if (text === "")
            return
        say("", false)
        try {
            var looksLikeAccount = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(text) && !/\.lez$/i.test(text) && /[A-Z0-9]/.test(text)
            if (looksLikeAccount) {
                var known = ask("reverse", [text])
                if (known === null) {
                    record = null
                    page = null
                    missing = ""
                    say("That account has no name.", "plain")
                    return
                }
                text = known.name
            }
            show(text)
        } catch (error) {
            sayRead(error.message)
        }
    }

    /// Puts a name on the plate, with every panel closed.
    /// Opens the "Register a name" screen.
    function openRegister() {
        section = "settings"
        gettingTokens = false
        say("", false)
        record = null
        page = null
        missing = ""
        tool = ""
        settingsOpen = false
        registerTaken = ""
        if (!registerChecking)
            registerWait = ""
        registering = true
        registerField.text = ""
        registerField.forceActiveFocus()
    }

    /// Checks the name typed on the "Register a name" screen: a free one opens
    /// its registration, a held one is said to be taken.
    /// Never a call that holds the screen: the button says "Checking…" at once, a busy wallet is
    /// said in amber under it and asked again by itself once, and the answer ends it.
    property bool registerChecking: false
    property string registerWait: ""       // amber, under Check: the wallet is busy, or didn't answer
    function checkRegister(typed, again) {
        var text = String(typed).trim()
        if (text === "" || (registerChecking && again !== true))
            return
        say("", false)
        registerTaken = ""
        registerChecking = true
        if (again !== true)
            registerWait = ""
        var failed = function(message, notTaken) {
            if ((message === moduleBusy || message === moduleNoAnswer) && again !== true) {
                registerWait = "The wallet is busy with another task. Checking again in a moment…"
                againAfter(4000, function() { checkRegister(text, true) })
                return
            }
            registerChecking = false
            registerWait = message === moduleBusy || message === moduleNoAnswer
                ? "The wallet is still busy with another task. Press Check again in a minute."
                : readFailed(message)
        }
        var lookUp = function(name) {
            askLater("resolve", [name], function(record) {
                registerChecking = false
                registerWait = ""
                if (record !== null && record !== undefined) {
                    registerTaken = name
                    return
                }
                show(name)
            }, failed, 30000)
        }
        if (!bridgeReady || !logos.callModuleAsync) {
            failed("This screen needs to run inside Basecamp.")
            return
        }
        logos.callModuleAsync("names", "normalize", [text], function(raw) {
            lastReply = shownReply("names.normalize", raw)
            var answer = Logic.normalizedReply(raw)
            if (answer.kind === "missing") {
                failed(moduleMissing)
                return
            }
            if (answer.kind !== "ok") {
                failed(answer.notTaken ? moduleBusy : moduleNoAnswer)
                return
            }
            if (answer.value === "") {
                registerChecking = false
                registerWait = "A name is 1 to 32 characters of a-z, 0-9 or a hyphen, such as jef."
                return
            }
            lookUp(answer.value)
        }, 15000)
    }

    function show(name) {
        searchField.text = name
        section = "search"
        registering = false
        gettingTokens = false
        tool = ""
        pageSourceOpen = false
        pageAll = false
        pageLink = ""
        forwardedFrom = ""
        read(name)
        // A name that forwards is shown where it points, one hop (SPEC.md §7); a name
        // this wallet holds opens as itself, so its holder can edit it.
        if (record !== null && record.forward && !cliHolds(record.name)) {
            var from = record.name
            read(record.forward)
            if (record !== null)
                forwardedFrom = from
            else
                say(from + " points to " + missing + ", which isn't registered.", true)
        }
    }

    /// The posts of `name`, for its page. Answers at once with the last read
    /// and looks again shortly while a newer one is on its way.
    function loadAuthorPosts(name, more) {
        if (authorOf !== name || !more)
            authorPosts = authorOf === name ? authorPosts : []
        authorOf = name
        var before = more && authorPosts.length > 0 ? authorPosts[authorPosts.length - 1].n : -1
        authorLoading = true
        authorWant = { name: name, before: before, more: more }
        askLater("board_posts_of", [name, before], function(value) {
            if (authorOf !== name || authorWant.before !== before)
                return
            value = value || {}
            var page = value.posts && value.posts.posts ? value.posts.posts : []
            if (value.posts && value.posts.total !== undefined)
                authorTotal = value.posts.total
            if (more) {
                var seen = {}
                authorPosts.forEach(function(post) { seen[post.id] = true })
                authorPosts = authorPosts.concat(page.filter(function(post) { return !seen[post.id] }))
            } else if (page.length > 0 || !value.updating) {
                authorPosts = page
            }
            authorLoading = !!value.updating
            if (value.updating)
                authorRetry.start()
        }, function() { authorLoading = false }, 150000)
    }
    property var authorWant: ({})

    /// Reads a name again without closing the panel the holder is working in.
    function read(name) {
        try {
            var found = ask("resolve", [name])
            if (found === null) {
                record = null
                page = null
                tool = ""
                missing = normalized(name)
                deposit = null
                if (missing !== "") {
                    loadDeposit(missing)
                    loadCliBalance()
                }
                else
                    say("A name is 1 to 32 characters of a-z, 0-9 or a hyphen, such as jef.", true)
                return
            }
            missing = ""
            if (avatarPixels !== null && avatarPixels.name !== found.name)
                avatarPixels = null
            record = found
            loadAvatar(found)
            page = ask("page_view", [found.name])
            if (!mine && tool !== "pay")
                tool = ""
            // A name the wallet holds joins "My names" once it is looked up (the module refuses others).
            if (wallet.length > 0 && !remembers(found.name))
                askLater("keep", [found.name], function() { root.loadMine() }, function() {})
        } catch (error) {
            sayRead(error.message)
        }
    }

    // ── Changing ────────────────────────────────────────────────────────

    /// Sends one change through the names module, then watches the chain until
    /// `settled()` says it shows. Sending is not success: a change can still be
    /// refused, so nothing is reported as done before it is read back.
    function change(method, args, name, settled, done) {
        if (busy)
            return
        if (holderOf(name) === "" && cliHolds(name)) {
            cliChange(method, args, name, settled, done)
            return
        }
        var job = { name: name, settled: settled, done: done, sent: false, tries: 0 }
        pending = job
        say("Asking the wallet to sign…", "working")
        askLater(method, args, function(value) {
            job.sent = true
            job.result = value
            say("Sent. Waiting for it to be confirmed.", "waiting")
        }, function(message) {
            pending = null
            say(message, true)
        })
    }

    /// The same change for a name the names CLI holds: the module starts the
    /// CLI, which proves the change on this machine (minutes), and this polls
    /// the job until it ends, then reads the name back.
    readonly property var cliMethods: ({ add_name: "cli_add_name", set_text: "cli_set_text", set_profile: "cli_profile", set_page: "cli_set_page", register: "cli_register", transfer: "cli_transfer" })

    // ── Handing names on (names transfer / receive-privately) ───────────
    property string transferTarget: ""   // the name whose Transfer card is open
    property bool transferConfirm: false

    function openTransfer(name) {
        transferTarget = name
        transferConfirm = false
        transferToField.text = ""
        say("", false)
    }

    /// Why the Transfer field can't be used, or "" (also "" while it is empty).
    function transferProblem() {
        var to = transferToField.text.trim()
        if (to === "")
            return ""
        if (!/^((Public|Private)\/)?[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to))
            return "That isn't an account. It's Public/ or Private/ and the account ID, or a pasted account address."
        var mine = [cliPublic ? cliPublic.account : "", cliBalance ? cliBalance.account : ""]
        if (mine.indexOf(to.replace(/^(Public|Private)\//, "")) >= 0)
            return "That's one of your own main accounts: the name is already yours."
        return ""
    }

    function transferNow() {
        var name = transferTarget
        var to = transferToField.text.trim()
        transferConfirm = false
        boardStart("cli_transfer", [name, to], "Transferring " + name, function() {
            say(name + " now belongs to " + Logic.keyShown(to.replace(/^(Public|Private)\//, "")) + ". They set it up under Names, in Got a name from someone, or with names receive-privately.", false)
            transferTarget = ""
            loadCliMine()
            loadActivity()
        }, name)
    }

    function takeName(name) {
        boardStart("cli_receive_privately", [name], "Setting up " + name, function() {
            say(name + " is set up: payments to it now arrive privately in a new account of yours.", false)
            takeNameField.text = ""
            loadCliMine()
            loadAccounts()
        }, name)
    }
    function cliChange(method, args, name, settled, done) {
        var cliMethod = cliMethods[method]
        if (cliMethod === undefined) {
            say("For a name the names CLI holds, only its profile, entries and page can be changed here.", true)
            return
        }
        var job = { name: name, kind: method, settled: settled, done: done, cli: true, id: "", asking: false, started: Date.now() }
        pending = job
        say("Starting the names CLI…", "working")
        askLater(cliMethod, args, function(value) {
            job.id = value.id
            job.started = value.started || Date.now()
            var skipped = value.skipped && value.skipped.length > 0 ? " (The avatar was left as it is: give the full path of a PNG file to change it.)" : ""
            say(savingText(job) + skipped, "waiting")
        }, function(message) {
            pending = null
            say(message, true)
        })
    }

    /// What a CLI job is doing, with how long it has taken against how long it takes.
    function savingText(job) {
        var minutes = Math.floor((Date.now() - job.started) / 60000)
        var sofar = minutes > 0 ? ", " + minutes + " min so far" : ""
        if (job.kind === "register")
            return "Step 1 of 3: proving the commitment on this computer. Usually 5–12 min" + sofar + "."
        if (job.kind === "add_name")
            return "Checking on chain that this wallet holds " + job.name + "…"
        return "Proving the change on this computer, so the chain never learns your account. Usually 5–12 min" + sofar + "."
    }

    /// The three steps of a private registration, for the register card and its progress.
    readonly property var registerSteps: [
        { title: "Prove the commitment", time: "5–12 min",
          why: "Your computer builds a zero-knowledge proof. The chain checks it without learning which account is yours." },
        { title: "Wait", time: "until 15 min after step 1",
          why: "The commitment hides which name you picked. The wait means someone who sees your reveal can't take the name first. It doesn't stop someone who learns the name another way around the time you commit." },
        { title: "Prove and register", time: "5–12 min",
          why: "A second proof reveals the name and moves the deposit. Then it's yours." }
    ]

    /// Which step a registration of `name` is on: 1 to 3, or 0 when none is under way.
    function registerStepOf(name) {
        if (pending !== null && pending.cli && pending.kind === "register" && pending.name === name)
            return 1
        var under = cliPendingFor(name)
        if (under === null || under.failed)
            return 0
        return /finishing now/.test(under.state) ? 3 : 2
    }

    /// A short line for a registration under way, from what the CLI says.
    function pendingLine(entry) {
        if (entry.failed)
            return "Stopped. Open it to try again"
        var minutes = /about (\d+) minutes/.exec(entry.state)
        if (/finishing now/.test(entry.state))
            return "Step 3 of 3 · usually 5–12 min"
        return "Step 2 of 3 · " + (minutes ? "waiting " + minutes[1] + " min" : /about a minute/.test(entry.state) ? "waiting 1 min" : "waiting") + ", then about 4"
    }

    function checkCliJob() {
        var job = pending
        if (job === null || !job.cli || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (pending !== job)
                return
            if (value.state === "running") {
                say(savingText(job), "waiting")
                return
            }
            pending = null
            loadCliMine()
            read(job.name)
            if (value.state === "done") {
                var settled = false
                try {
                    settled = job.settled()
                } catch (error) {
                }
                say(settled ? (typeof job.done === "function" ? job.done(null) : job.done)
                    : "The names CLI finished, but the change does not show yet. Look the name up again in a moment.", !settled)
            } else if (job.kind === "add_name") {
                // Said under the Add box, not at the top: a name that isn't this wallet's yet
                // (typed there to register it) gets the way to register it.
                var problem = sentence(Logic.jobProblem(value, "Adding the name didn't finish"))
                addNameResult = { name: job.name, line: problem, notYours: Logic.addNameNotYours(problem) }
                say("", false)
            } else {
                say(sentence(Logic.jobProblem(value, "The names CLI could not save it")), true)
            }
            if (job.kind === "add_name" && value.state === "done")
                addNameResult = { name: job.name, line: "Added " + job.name + ": this wallet holds it.", notYours: false, done: true }
        }, function(message) {
            job.asking = false
            console.log("names_ui: job:", message)
        })
    }
    property var addNameResult: null       // the Add box's last answer: {name, line, notYours, done}
    property bool addNameOpen: false       // "Hold a name that isn't listed? ▸"

    function lastLogLine(text) {
        var lines = String(text || "").split("\n")
        for (var i = lines.length - 1; i >= 0; i--)
            if (lines[i].trim() !== "")
                return lines[i].trim()
        return "it stopped without saying why"
    }

    function checkPending() {
        var job = pending
        if (job.cli || !job.sent)
            return
        var settled = false
        try {
            settled = job.settled()
        } catch (error) {
            // A failed read is not an answer; the next tick asks again. A busy one doesn't count
            // against the tries: the module was on another call, nothing was read.
            if (waitAgain(error.message))
                return
        }
        job.tries += 1
        if (!settled && job.tries < 40)
            return
        pending = null
        loadWallet(false)
        read(job.name)
        if (settled)
            say(typeof job.done === "function" ? job.done(job.result) : job.done, false)
        else
            say("That was sent but has not taken effect, so it may have been refused. Check the name and try again.", true)
    }

    /// Registers through the names CLI, from its private account: nobody can
    /// find who holds the name. The CLI finishes it in the background.
    function registerPrivately() {
        var name = missing
        if (busy)
            return
        // The job ending is the answer: the commitment is made.
        var public_ = registerPublic
        cliChange("register", [name, public_], name, function() {
            return true
        }, "Committed privately. " + name + " finishes registering in about 15 to 20 minutes, in the background"
           + (public_ ? ", then moves to its own public account (about 10 more minutes)." : "."))
    }

    /// Sends the commitment. The reveal follows by itself (see revealDue).
    function registerName() {
        var name = missing
        var owner = wallet[fromIndex].account
        if (busy)
            return
        pending = { name: name, sent: false }
        say("Asking the wallet to sign…", "working")
        // On LEZ 0.4 private payments are off: a name is registered public-only, with no private
        // receiving keys ("auto" would make a private account and publish its keys).
        if (namesNet === "0.4" && moduleFeatures.indexOf("app_note") >= 0)
            askLater("app_note", ["publish:" + name, publishOnRegister ? "yes" : "no"], function() {}, function() {}, 10000)
        askLater("register_name", [name, owner, namesNet === "0.4" ? "" : "auto"], function(value) {
            pending = null
            say("Committed. The reveal goes out by itself in about 15 minutes while Names is open; Wallet, Names shows when.", "waiting")
            missing = ""
            loadMine()
        }, function(message) {
            pending = null
            say(message, true)
        })
    }

    /// Opens the settings of a name picked from "My names".
    function manage(name) {
        show(name)
        if (record !== null && mine)
            openSettings()
    }

    /// Everything the holder can do with the name on the plate: where it
    /// pays, its entries and page, transferring it, releasing it.
    function openSettings() {
        say("", false)
        pageArea.text = page !== null && page.text !== null ? page.text : ""
        previewPage()
        newKeyField.text = ""
        newValueField.text = ""
        newOwnerField.text = ""
        confirmingRelease = false
        editTab = "profile"
        tool = "settings"
    }

    function receivePrivately() {
        var name = record.name
        change("receive_privately", [name, "auto"], name, function() {
            var found = ask("resolve", [name])
            return found.npk !== null && found.account === null
        }, shown(name) + " now takes private payments only.")
    }

    function clearAddresses() {
        var name = record.name
        change("clear_addresses", [name], name, function() {
            var found = ask("resolve", [name])
            return found.account === null && found.npk === null
        }, name + " no longer takes payments.")
    }

    /// Sets one entry; an empty value removes it.
    function saveText(key, value) {
        var name = record.name
        key = key.trim()
        change("set_text", [name, key, value], name, function() {
            var texts = ask("resolve", [name]).texts
            return value === "" ? texts[key] === undefined : texts[key] === value
        }, value === "" ? "Removed " + key + " from " + name + "." : "Saved " + key + " on " + name + ".")
    }

    /// The Profile fields, with the text-entry key each one sets.
    readonly property var profileFields: [
        { key: "display", label: "Display name", hint: "How you want to be shown" },
        { key: "bio", label: "Bio", hint: "A few lines about you", lines: true },
        { key: "avatar", label: "Avatar", hint: "data:image/png;base64,… (names profile --avatar)" },
        { key: "theme", label: "Theme", hint: "light, dark or auto", choices: [["", "Auto"], ["light", "Light"], ["dark", "Dark"]] },
        { key: "color", label: "Colour", hint: "#rrggbb", swatches: ["#96d678", "#f0c562", "#7fa8ff", "#ff9c8c", "#c3a6ff"] },
        { key: "font", label: "Font", hint: "serif, mono or rounded", choices: [["", "Sans"], ["serif", "Serif"], ["mono", "Mono"], ["rounded", "Rounded"]] },
        { key: "cid", label: "File (CID)", hint: "A file in Logos Storage" }
    ]

    /// The Profile fields that differ from the name's entries: the new value,
    /// or an empty string for one that was cleared.
    function profileChanges() {
        var changed = {}
        if (record === null)
            return changed
        for (var i = 0; i < profileFields.length; i++) {
            var key = profileFields[i].key
            var before = record.texts && record.texts[key] !== undefined ? record.texts[key] : ""
            var now = profileDraft[key] !== undefined ? profileDraft[key] : before
            if (now !== before)
                changed[key] = now
        }
        return changed
    }

    /// Sets every changed Profile field in one transaction.
    function saveProfile() {
        var name = record.name
        var entries = profileChanges()
        change("set_profile", [name, JSON.stringify(entries), false], name, function() {
            var texts = ask("resolve", [name]).texts || {}
            for (var key in entries)
                if (entries[key] === "" ? texts[key] !== undefined : texts[key] !== entries[key])
                    return false
            return true
        }, "Saved the profile of " + name + ".")
    }

    /// Publishes the page; an empty body removes it. A body that opens with a
    /// tag is published as HTML, anything else as plain text.
    function savePage(body) {
        var name = record.name
        var kind = /^\s*</.test(body) ? "text/html; charset=utf-8" : "text/plain; charset=utf-8"
        change("set_page", [name, kind, body], name, function() {
            var published = ask("page", [name])
            return body === "" ? published === null : published !== null && published.text === body
        }, body === "" ? "Removed the page of " + name + "." : "Published the page of " + name + ".")
    }

    function transferName() {
        var name = record.name
        var newOwner = newOwnerField.text.trim()
        var before = record.owner_tag
        change("transfer", [name, newOwner], name, function() {
            return ask("resolve", [name]).owner_tag !== before
        }, name + " now belongs to " + shorten(newOwner) + ".")
    }

    function releaseName() {
        var name = record.name
        change("release", [name], name, function() {
            return ask("resolve", [name]) === null
        }, function(result) {
            return result && result.refund ? "Released " + name + ". " + grouped(result.refund) + " returned to you." : "Released " + name + "."
        })
    }

    /// Who can pay: the wallet's funded public accounts, then its private ones.
    function senders() {
        var all = []
        for (var i = 0; i < wallet.length; i++)
            all.push({ account: wallet[i].account, balance: wallet[i].balance, name: wallet[i].name, isPrivate: false })
        for (var j = 0; j < privateWallet.length; j++)
            all.push({ account: privateWallet[j].account, balance: privateWallet[j].balance, name: null, isPrivate: true })
        return all
    }

    // ── Small helpers ───────────────────────────────────────────────────

    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.", false)
    }

    /// A name as people read it in this app: bare, `jef` (SPEC 2).
    function shown(name) {
        return name
    }

    function shorten(id) {
        return id.length > 16 ? id.substr(0, 8) + "…" + id.substr(id.length - 6) : id
    }

    function linkFor(name) {
        return "basecamp://intent/names.open?p=" + Qt.btoa(JSON.stringify({ name: name + ".lez" })).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
    }

    /// Digits grouped in threes, so a balance can be read at a glance.
    function grouped(digits) {
        return String(digits).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    /// What the page is, in a few words: "HTML, 148 bytes".
    function pageSummary() {
        if (page === null)
            return ""
        var size = page.bytes !== undefined && page.bytes !== null ? page.bytes : (page.text !== null ? page.text.length : 0)
        var kind = /html/i.test(page.content_type) ? "HTML" : /^text\//i.test(page.content_type) ? "Text" : "File"
        return kind + ", " + (size < 1024 ? size + " bytes" : (size / 1024).toFixed(1) + " KB")
    }

    /// The line over the page: its title, if it has one, and what it is.
    function pageHeading() {
        if (page === null)
            return ""
        return typeof page.title === "string" && page.title !== "" ? page.title : ""
    }

    /// What the page card draws: for an HTML page, only the markup the names
    /// module sanitized (`html` from page_view), under this app's own small
    /// stylesheet; for a plain-text page, its text, drawn as plain text; for
    /// anything else, nothing. The page's raw source is never rendered.
    function pageBody() {
        return viewBody(page)
    }

    /// What a page card draws for `view`, a page_view answer.
    function viewBody(view) {
        if (view === null)
            return ""
        if (view.kind === "html")
            // A span right before another tag gets a gap after it: pages lay
            // out "label, value" pairs with CSS this app does not apply.
            return typeof view.html === "string" && view.html !== "" ? pageStyle() + view.html.replace(/<\/span>(?=<)/g, "</span>&nbsp;&nbsp;") : ""
        if (view.kind === "text")
            return typeof view.text === "string" ? view.text.substr(0, 65536).replace(/\s+$/, "") : ""
        return ""
    }

    /// Asks the module how the page being written will show; the preview
    /// waits for typing to pause.
    function previewPage() {
        var body = pageArea.text
        if (body === "") {
            pagePreview = null
            return
        }
        askLater("page_preview", [body], function(value) {
            if (pageArea.text === body)
                pagePreview = value
        }, function(message) {
            // Busy keeps the last preview and asks again; a refusal clears it.
            if (waitAgain(message))
                againWhenFree("preview", function() { if (pageArea.text === body) previewPage() })
            else
                pagePreview = null
        })
    }

    /// The app's stylesheet for a page, so it reads in the app's type and
    /// colours. The page's own styles never reach it.
    function pageStyle() {
        return "<style>"
            + "h1,h2,h3,h4{color:" + accent + ";font-weight:600;}"
            + "h1{font-size:26px;} h2{font-size:22px;} h3{font-size:19px;} h4{font-size:17px;}"
            + "code,pre{font-family:" + mono + ";color:" + dim + ";font-size:15px;}"
            + "blockquote,span{color:" + dim + ";}"
            + "th{color:" + dim + ";text-align:left;}"
            + "th,td{padding:2px 18px 2px 0;}"
            + "a{color:" + accent + ";}"
            + "</style>"
    }

    // Basecamp's plugin sandbox refuses every https URL ("scheme https is not allowed in the
    // basecamp plugin sandbox", seen on the M2), so the deposit page is never opened from here:
    // its link is copied, and "Copied." shows on the button for a moment.
    property bool receiveLinkCopied: false
    function copyDepositLink(link) {
        copy(String(link || ""), "The link")
        receiveLinkCopied = true
        console.log("names_ui: receive: the deposit page link copied")
        againAfter(2500, function() { receiveLinkCopied = false })
    }
    property bool receiveQrOpen: false        // Receive's QR, closed by default

    /// Opens the link last pressed in the page in the browser, if it is an
    /// http(s) address (the module lets no other kind through) and Basecamp
    /// lets this app open one.
    function openPageLink() {
        var link = pageLink
        if (!/^https?:\/\/[^\s"'<>]+$/i.test(link))
            return
        var opened = false
        try {
            opened = Qt.openUrlExternally(link)
        } catch (error) {
            opened = false
        }
        if (!opened)
            say("Basecamp did not open the link. Copy it instead.", true)
    }

    /// The page's source as plain text. It is never rendered: a page is its
    /// holder's words, and nothing in it should be able to act inside this app.
    function pageSource() {
        if (page === null || page.text === null || page.text === undefined)
            return "This page is not text."
        return String(page.text).substr(0, 4000).replace(/\s+$/, "")
    }

    /// Asks the module for the pixels of `found`'s avatar, if it has one. A
    /// reply that lands after another name is on the plate is dropped.
    function loadAvatar(found) {
        var name = found.name
        if (!found.texts || typeof found.texts.avatar !== "string" || found.texts.avatar.trim() === "") {
            avatarPixels = null
            return
        }
        askLater("avatar", [name], function(value) {
            if (record === null || record.name !== name)
                return
            var ok = value && value.width > 0 && value.height > 0 && value.width <= 128 && value.height <= 128
                && typeof value.pixels === "string"
            avatarPixels = ok ? { name: name, width: value.width, height: value.height, pixels: value.pixels } : null
        }, function(message) {
            if (waitAgain(message))
                againWhenFree("avatar", function() { if (record !== null && record.name === name) loadAvatar(record) })
            else if (record !== null && record.name === name)
                avatarPixels = null
        })
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

    function profileText(key) {
        return record !== null && record.texts && typeof record.texts[key] === "string" ? record.texts[key].trim() : ""
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

    /// The entries other apps read (chat, inbox, agent, cid and the rest),
    /// without the ones that draw the profile. `alsoHide` leaves out more.
    function textKeys(alsoHide) {
        if (record === null || !record.texts)
            return []
        var hidden = presentationKeys.concat(alsoHide || [])
        return Object.keys(record.texts).filter(function(key) { return hidden.indexOf(key) < 0 })
    }

    /// An entry's key as a label.
    function entryLabel(key) {
        var known = { chat: "Chat", inbox: "Inbox", agent: "Agent", cid: "File (CID)", "logos.account": "Logos account" }
        return known[key] !== undefined ? known[key] : key
    }

    /// How a name can be paid, in a word or two.
    /// A Profile field as typed in Edit, else as the name has it.
    function draftText(key) {
        if (profileDraft[key] !== undefined)
            return String(profileDraft[key]).trim()
        return profileText(key)
    }

    /// The accent the profile being edited would show.
    readonly property string draftFont: draftText("font") === "serif" ? "Georgia" : draftText("font") === "mono" ? mono : draftText("font") === "rounded" ? "Arial Rounded MT Bold" : sans
    readonly property color draftAccent: /^#[0-9a-fA-F]{6}$/.test(draftText("color")) ? readable(draftText("color")) : brass

    /// Who holds the name, in a few words.
    function holderChip() {
        if (record === null)
            return ""
        if (cliHeld) {
            // Asked to be public, still private: cli_mine says why (register-as-public-silently-stays-private).
            for (var i = 0; i < cliNames.length; i++)
                if (cliNames[i].name === record.name && cliNames[i].not_public)
                    return "Yours · held privately · not public yet: " + cliNames[i].not_public
            return "Yours · held privately"
        }
        if (mine)
            return "Yours · public"
        return "Holder not shown"
    }

    /// How the name is paid, in a few words.
    function paymentsChip() {
        if (record === null)
            return ""
        return record.npk !== null && record.account !== null ? "Private or public payments" : record.npk !== null ? "Takes private payments" : record.account !== null ? "Takes public payments" : "Not set up for payments"
    }

    /// The chain this app is on, from the sequencer's address.
    function networkName() {
        if (zonesMany && zoneName !== "")
            return zoneName
        return /testnet/.test(sequencer) ? "LEZ testnet" : /127\.0\.0\.1|localhost/.test(sequencer) ? "Local chain" : "LEZ"
    }

    function paymentsText() {
        if (record === null)
            return ""
        return record.npk !== null && record.account !== null ? "Privately or publicly" : record.npk !== null ? "Privately" : record.account !== null ? "Publicly" : "Not set up yet"
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

    // ── The board (Home) ────────────────────────────────────────────────

    /// The names this wallet can post as: those the names CLI holds. A private
    /// holder posts by a proof (~4 min, free), a public one in a block (gas).
    readonly property var boardNames: cliNames
    /// Your main name: the one set for this device if the wallet still holds it,
    /// else the first name the CLI holds privately that isn't the anon one.
    readonly property string mainName: {
        // The saved main name is known at once; it is shown while the CLI's
        // list of names is still on its way, and checked against it after.
        if (!cliKnown && savedMain !== "")
            return savedMain
        var own = boardNames.filter(function(entry) { return entry.name !== anonName })
        if (savedMain !== "" && own.some(function(entry) { return entry.name === savedMain }))
            return savedMain
        return own.length > 0 ? own[0].name : ""
    }
    readonly property bool anonReady: anonName !== "" && cliHolds(anonName)
    /// Who the post box posts as.
    readonly property string boardAs: anonOn && anonReady ? anonName : mainName
    /// The posting name is being made public: its holder changes and its profile
    /// is put back, so it can't post until that's done.
    readonly property bool boardAsMoving: boardJob !== null && boardJob.kind === "make_public" && boardJob.name === boardAs
    /// First run: no names CLI yet, or the module says plainly there is no wallet on this network
    /// (`cli_status.wallet` false; a busy or failed read never says so). A wallet with no name
    /// is not a first run: it reaches the Wallet, Activity, Settings and request sheets, and the
    /// Names tab (and a card on the Wallet) offers a name. A bridge-only person never needs one.
    readonly property bool needsWelcome: configKnown && registry !== "" && (cliPath === "" || Logic.noWalletSaid(cliStatus))
    /// The "pick a name" card on the Wallet was put away for this run.
    property bool nameCardAway: false
    // The Welcome screen stands in for every section but Settings (where
    // Register is) and Search (where a name checked there opens to register).
    readonly property bool welcomeCovers: needsWelcome && section !== "settings" && section !== "search" && !(section === "wallet" && walletView === "bridge")

    /// Opens a section of the sidebar; a notice from another section doesn't follow.
    function go(which) {
        say("", false)
        if (section === "settings" && which !== "settings")
            clearBackupPasswords()
        section = which
    }

    function setMain(name) {
        askLater("set_main_name", [name], function(value) { savedMain = value || "" }, function(message) { say(message, true) })
    }

    /// Opens Profile: the edit screen for your main name.
    function openProfile() {
        if (mainName === "") {
            say("Register a name first.", true)
            return
        }
        show(mainName)
        if (record !== null && mine)
            openSettings()
        section = "profile"
    }

    /// Makes the anon name (once): a CLI job, then a registration the CLI finishes.
    function makeAnon() {
        boardStart("make_anon", [anonPublic || namesNet === "0.4"], "Setting up your anon name", function() {
            askLater("anon_name", [], function(value) { anonName = value || "" }, function() {})
            loadCliMine()
        })
    }

    function boardToggleLike(post) {
        if (boardAs === "") {
            say("Register a name to like posts.", true)
            return
        }
        var liked = boardLiked.indexOf(post) >= 0
        boardStart("board_toggle_like", [boardAs, post], liked ? "Your unlike" : "Your like", null, boardAs)
        var next = boardLiked.filter(function(item) { return item !== post })
        if (!liked)
            next.push(post)
        boardLiked = next
    }

    property bool terminalNoteOpen: false // Welcome shows the command-line install
    property string payForwardSeen: ""   // "name→target" once the payer was told the name forwards
    property bool payFromPublic: false   // paying an account from the public account rather than the private one
    property bool payingPublicly: false  // the payment under way goes from the public account (one block, no proof)

    /// Whether the Pay field holds an account (`Public/<id>`) rather than a name.
    function payIsAccount(to) {
        return /^Public\//.test(String(to).trim())
    }

    /// Why the account in the Pay field can't be paid, or "" if it looks right.
    function payAccountProblem(to) {
        to = String(to).trim()
        if (!/^Public\/[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(to))
            return "That isn't a public account. It's Public/ and then the account ID, letters and digits."
        if (myPublic !== "" && to === "Public/" + myPublic)
            return "That's your own public account."
        return ""
    }

    /// Whether paying an account goes from the public account: when asked to,
    /// or when the private account is known not to hold enough.
    /// Whether an account is paid from the public account: only when Send's From says so.
    function payGoesPublic(amount) {
        return payFromPublic
    }

    function payName() {
        var to = sendToResolved()
        var amount = sendDigits()
        if (to === "" || amount === "")
            return
        if (payIsAccount(to)) {
            var problem = payAccountProblem(to)
            if (problem !== "") {
                say(problem, true)
                return
            }
            if (boardJob !== null)
                return
            var fromPublic = payGoesPublic(amount)
            payingPublicly = fromPublic
            boardStart("cli_pay_account", [to, amount, !fromPublic], "Your payment to " + shorten(to.substr(7)), function() {
                sendAmount.text = ""
                loadCliBalance()
            }, "-wallet")
            return
        }
        var bare = ""
        // The "your own name" check needs the module's answer: busy stops here, never skips it.
        try {
            bare = normalized(to)
        } catch (error) {
            say(error.message, true)
            return
        }
        if (bare !== "" && cliHolds(bare)) {
            say(bare + " is your own name: the payment would come back to you.", true)
            return
        }
        // A name that forwards is paid where it points: say so first (SPEC.md §7).
        var target = ""
        try {
            var recipient = ask("recipient", [bare !== "" ? bare : to])
            if (recipient && recipient.forwarded_from)
                target = recipient.name
        } catch (error) {
            say(sentence(error.message), true)
            return
        }
        // On LEZ 0.4 only a name's public account is paid: a payment to its private keys
        // is proven on this computer, which isn't offered there yet.
        if (namesNet === "0.4" && recipient && !recipient["public"]) {
            say((target !== "" ? target : to) + " takes only private payments, which stay off on LEZ 0.4 until real proofs have been run there. Switch to LEZ 0.3 to pay it privately.", true)
            return
        }
        if (target !== "" && payForwardSeen !== to + "→" + target) {
            payForwardSeen = to + "→" + target
            say(to + " forwards to " + target + ", so the payment goes to " + target + ". Press Pay again to send it.", "plain")
            return
        }
        payForwardSeen = ""
        boardStart(namesNet === "0.4" ? "cli_pay_public" : "cli_pay", [to, amount], "Your payment to " + (target !== "" ? to + " → " + target : to) + (namesNet === "0.4" ? " on LEZ 0.4, from your public account" : ""), function() {
            sendAmount.text = ""
            loadCliBalance()
        })
    }

    function boardAgo(time) {
        var seconds = Math.max(0, Math.floor((clockTick - time) / 1000))
        if (seconds < 60) return seconds + "s"
        if (seconds < 3600) return Math.floor(seconds / 60) + "m"
        if (seconds < 86400) return Math.floor(seconds / 3600) + "h"
        return dated(time)
    }

    /// Whether the names CLI holds `name` from a public account (posts in a block).
    function heldPublicly(name) {
        return cliNames.some(function(entry) { return entry.name === name && entry["private"] === false })
    }

    /// What the running board change is doing, in a line.
    function boardJobLine(job) {
        var seconds = Math.max(0, Math.floor((Math.max(clockTick, Date.now()) - job.started) / 1000))
        if (job.kind === "cli_pay_account" && payingPublicly)
            return job.label + ": sending it publicly, one block (about 30 seconds), " + seconds + "s so far."
        if (heldPublicly(job.name) && (job.kind === "board_post" || job.kind === "board_toggle_like" || job.kind === "board_hide"))
            return job.label + ": sending it publicly, one block (about 30 seconds), " + seconds + "s so far."
        return job.label + ": proving it on this computer, so the chain never learns your account. Usually 5–12 min, " + Math.floor(seconds / 60) + " so far."
    }

    /// The posts as a tree: each post, then its replies under it (oldest first),
    /// each row `{post, depth}`. A reply whose post isn't loaded stands alone.
    readonly property var boardTree: {
        var byId = {}, children = {}, rows = []
        for (var i = 0; i < boardPosts.length; i++)
            byId[boardPosts[i].id] = boardPosts[i]
        var tops = []
        for (var j = 0; j < boardPosts.length; j++) {
            var p = boardPosts[j]
            if (p.parent_id && byId[p.parent_id]) {
                (children[p.parent_id] = children[p.parent_id] || []).push(p)
            } else {
                tops.push(p)
            }
        }
        if (boardView === "thread")
            tops.sort(function(a, b) { return a.time - b.time })
        function add(post, depth) {
            rows.push({ post: post, depth: depth })
            var replies = (children[post.id] || []).slice().sort(function(a, b) { return a.time - b.time })
            for (var k = 0; k < replies.length; k++)
                add(replies[k], Math.min(depth + 1, 3))
        }
        for (var t = 0; t < tops.length; t++)
            add(tops[t], 0)
        return rows
    }

    /// The top of the thread `id` is in, as far as the loaded posts go.
    function boardRootOf(id) {
        var byId = {}
        for (var i = 0; i < boardPosts.length; i++)
            byId[boardPosts[i].id] = boardPosts[i]
        var at = byId[id]
        for (var hops = 0; at && at.parent_id && byId[at.parent_id] && hops < 50; hops++)
            at = byId[at.parent_id]
        return at ? at.id : id
    }

    /// "Jef: first words of the post", for what a reply answers.
    function boardPreviewOf(id) {
        for (var i = 0; i < boardPosts.length; i++) {
            var p = boardPosts[i]
            if (p.id === id) {
                var who = boardPeople[p.name] && boardPeople[p.name].display !== "" ? boardPeople[p.name].display : p.name
                var text = p.text.replace(/\s+/g, " ").trim()
                return who + ": \u201c" + (text.length > 60 ? text.substr(0, 60) + "\u2026" : text) + "\u201d"
            }
        }
        return "a post"
    }

    function boardFollowing(name) {
        return boardFollows.indexOf(name) >= 0
    }

    /// Loads the open feed or thread. Until the names module answers, it asks
    /// again every ten seconds.
    function boardLoad() {
        if (cliPath === "")
            return
        boardLoading = true
        boardError = ""
        var method = boardView === "thread" ? "board_thread" : "board_feed"
        var args = boardView === "thread" ? [boardThreadOf] : [boardView === "following"]
        askLater(method, args, function(value) {
            value = value || {}
            boardLoading = !!value.updating
            boardError = value.error ? String(value.error) : ""
            boardPosts = value.posts || []
            for (var i = 0; i < boardPosts.length; i++)
                boardLookUp(boardPosts[i].name)
            // A newer read is on its way: look again shortly.
            if (value.updating)
                boardRetry.start()
        }, function(message) {
            boardLoading = false
            boardError = message
            boardRetry.start()
        }, 150000)
        askLater("board_follows", [], function(value) { boardFollows = value || [] }, function() {})
    }

    /// A name's display name, colour and avatar, looked up once for its posts.
    function boardLookUp(name) {
        if (boardPeople[name] !== undefined)
            return
        var known = Object.assign({}, boardPeople)
        known[name] = { display: "", color: "", pixels: null }
        boardPeople = known
        askLater("resolve", [name], function(found) {
            if (found === null)
                return
            var texts = found.texts || {}
            var next = Object.assign({}, boardPeople)
            next[name] = Object.assign({}, next[name], {
                npk: typeof found.npk === "string" ? found.npk.toLowerCase() : "",
                display: typeof texts.display === "string" ? texts.display.trim() : "",
                color: /^#[0-9a-fA-F]{6}$/.test(texts.color || "") ? readable(texts.color) : ""
            })
            boardPeople = next
            if (typeof texts.avatar === "string" && texts.avatar.trim() !== "")
                askLater("avatar", [name], function(value) {
                    if (!(value && value.width > 0 && value.height > 0 && value.width <= 128 && value.height <= 128))
                        return
                    var more = Object.assign({}, boardPeople)
                    more[name] = Object.assign({}, more[name], { pixels: { width: value.width, height: value.height, pixels: value.pixels } })
                    boardPeople = more
                }, function() {})
        }, function() {})
    }

    function boardShow(which, post) {
        section = "posts"
        boardView = which
        boardThreadOf = post || ""
        boardPosts = []
        boardLoad()
    }

    /// Starts a board change as a CLI job and follows it until it ends.
    function boardStart(method, args, label, after, name) {
        if (boardJob !== null)
            return
        say("", false)
        var job = { id: "", label: label, started: Date.now(), after: after, kind: method, name: name || "" }
        boardJob = job
        askLater(method, args, function(value) {
            job.id = value.id
            job.started = value.started || Date.now()
            clockTick = Date.now()
        }, function(message) {
            boardJob = null
            say(message, true)
        })
    }

    function boardCheckJob() {
        var job = boardJob
        if (job === null || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (boardJob !== job || value.state === "running")
                return
            boardJob = null
            if (value.state === "done") {
                var paid = Logic.feePaid(value.fee)
                say(job.label + " done." + (paid !== "" ? " " + paid : ""), false)
                if (job.after)
                    job.after()
                boardLoad()
            } else {
                // The module's own words, never the CLI's "Error:" prefix or its last cause line
                // (send-failure-text-raw-and-no-balance-check, transfer-failure-text-is-last-cause-line).
                say(sentence(Logic.jobProblem(value, "It didn't go through")), true)
            }
        }, function() { job.asking = false })
    }

    // ── Messages ────────────────────────────────────────────────────────

    /// Asks the chat module; `done(value)` or `failed(message)` runs later.
    function chatAsk(method, args, done, failed) {
        if (!(bridgeReady && logos.callModuleAsync)) {
            failed("Messages need Basecamp's chat module.")
            return
        }
        logos.callModuleAsync("chat_module", method, args, function(reply) {
            var value = parsed(reply)
            if (value && typeof value === "object" && value.success === false)
                failed(value.error ? String(value.error) : "the chat module said no")
            else if (value && typeof value === "object" && "value" in value && "success" in value)
                done(value.value)
            else
                done(value)
        }, 20000)
    }

    /// A chat address out of whatever the chat module wrapped it in: the text
    /// itself, or an object holding it. "" if there is none.
    function chatText(value) {
        for (var depth = 0; depth < 3 && value !== null && typeof value === "object"; depth++)
            value = value.address !== undefined ? value.address : value.value !== undefined ? value.value
                  : value.result !== undefined ? value.result : null
        var text = typeof value === "string" ? value.trim() : ""
        return /^[0-9A-Za-z_:+\/=.-]{16,200}$/.test(text) ? text : ""
    }

    function chatLoad() {
        chatAsk("status", [], function(status) {
            chatState = status && status.delivery_state ? String(status.delivery_state) : "unknown"
            chatError = ""
        }, function(message) {
            chatState = ""
            chatError = message
        })
        chatAsk("get_address", [], function(address) { chatMine = chatText(address) }, function() {})
        chatAsk("list_conversations", [], function(list) {
            var all = (list || []).slice()
            all.sort(function(a, b) { return (b.last_activity_ms || 0) - (a.last_activity_ms || 0) })
            chatConvos = all
        }, function() {})
        askLater("dm_names", [], function(value) { dmNames = value || {} }, function() {})
        askLater("chat_names", [], function(value) { chatNames = value || {} }, function() {})
        if (mainName !== "")
            askLater("resolve", [mainName], function(found) {
                mainChat = found && found.texts ? chatText(found.texts.chat) : ""
            }, function() {})
    }

    function chatConvo(id) {
        for (var i = 0; i < chatConvos.length; i++)
            if (chatConvos[i].convo_id === id)
                return chatConvos[i]
        return null
    }

    function chatIsGroup(id) {
        var convo = chatConvo(id)
        return convo !== null && /group/.test(String(convo.kind))
    }

    function chatLabel(id) {
        var convo = chatConvo(id)
        if (convo !== null && convo.name)
            return convo.name
        if (dmNames[id] !== undefined)
            return dmNames[id]
        if (convo !== null && convo.nickname)
            return convo.nickname
        return "New conversation"
    }

    /// Who a chat address is: the name this device knows for it, else short.
    function chatWho(address) {
        if (!address)
            return ""
        if (address === chatMine)
            return mainName !== "" ? mainName : "you"
        return chatNames[address] !== undefined ? chatNames[address] : String(address).substr(0, 8) + "…"
    }

    /// The chat address a name publishes, or "" (then `failed` has said why).
    function chatAddressOf(typed, done) {
        var name = ""
        try {
            name = normalized(typed)
        } catch (error) {
            say(error.message, true)
            return
        }
        if (name === "") {
            say("That isn't a name.", true)
            return
        }
        if (cliHolds(name)) {
            say(name + " is you. Message someone else's name.", true)
            return
        }
        askLater("resolve", [name], function(found) {
            var address = found && found.texts ? chatText(found.texts.chat) : ""
            if (found === null)
                say(name + " isn't registered.", true)
            else if (address === "")
                say(name + " hasn't turned on messages yet.", true)
            else {
                askLater("set_chat_name", [address, name], function(value) { chatNames = value || {} }, function() {})
                done(name, address)
            }
        }, function(message) { say(message, true) })
    }

    function chatMakeGroup() {
        var name = groupName.text.trim()
        if (name === "")
            return
        chatAsk("create_group_conversation", [name, groupDesc.text.trim()], function(created) {
            var convo = created && typeof created === "object" ? (created.convo_id || created.id || "") : String(created || "")
            chatNewGroup = false
            groupName.text = ""
            groupDesc.text = ""
            chatLoad()
            if (convo !== "")
                chatOpen(convo)
        }, function(message) { say("Couldn't make the group: " + message, true) })
    }

    function chatAddMember() {
        var convo = chatOpenId, typed = memberField.text
        chatAddressOf(typed, function(name, address) {
            chatAsk("add_group_member", [convo, address], function() {
                memberField.text = ""
                say(name + " added.", false)
                chatReadMembers()
            }, function(message) { say("Couldn't add " + name + ": " + message, true) })
        })
    }

    function chatReadMembers() {
        if (chatOpenId === "" || !chatIsGroup(chatOpenId)) {
            chatMembers = []
            return
        }
        var convo = chatOpenId
        chatAsk("list_group_members", [convo], function(list) {
            if (chatOpenId === convo)
                chatMembers = list || []
        }, function() {})
    }

    function chatOpen(convo) {
        chatOpenId = convo
        chatMessages = []
        chatMembers = []
        chatReadMessages()
        chatReadMembers()
    }

    function chatReadMessages() {
        if (chatOpenId === "")
            return
        var convo = chatOpenId
        chatAsk("get_messages", [convo], function(list) {
            if (chatOpenId === convo)
                chatMessages = list || []
        }, function(message) { say(message, true) })
    }

    /// Starts a DM with `typed`: its name's published chat address, if it has one.
    function chatStart(typed) {
        chatAddressOf(typed, function(name, address) {
            chatAsk("create_conversation", [address], function(created) {
                var convo = created && typeof created === "object" ? (created.convo_id || created.id || "") : String(created || "")
                if (convo === "") {
                    chatLoad()
                    return
                }
                askLater("set_dm_name", [convo, name], function(value) { dmNames = value || {} }, function() {})
                chatAsk("set_conversation_nickname", [convo, name], function() {}, function() {})
                chatLoad()
                chatOpen(convo)
                dmTo.text = ""
            }, function(message) { say("Couldn't start the conversation: " + message, true) })
        })
    }

    function chatSend() {
        var text = dmText.text.trim()
        if (chatOpenId === "" || text === "")
            return
        chatAsk("send_message", [chatOpenId, text], function() {
            dmText.text = ""
            chatReadMessages()
        }, function(message) { say("Not sent: " + message, true) })
    }

    /// Publishes this installation's chat address on the main name, so others can DM it.
    function chatPublish() {
        if (mainName === "" || chatText(chatMine) === "") {
            say("Messaging hasn't given this computer an address yet; try again in a moment.", true)
            return
        }
        var name = mainName, address = chatMine
        cliChange("set_text", [name, "chat", address], name, function() {
            var found = ask("resolve", [name])
            return found !== null && found.texts && found.texts.chat === address
        }, "People can now message " + name + ".")
    }

    Timer {
        interval: 10000
        repeat: true
        running: root.section === "messages"
        onTriggered: root.chatLoad()
    }
    Timer {
        interval: 3000
        repeat: true
        running: root.section === "messages" && root.chatOpenId !== ""
        onTriggered: root.chatReadMessages()
    }

    /// Picks up a change already running (the app was closed mid-way), so it
    /// shows and blocks what it should until it ends.
    function loadRunningJobs() {
        askLater("running_jobs", [], function(value) {
            // The install and the move to private have their own progress (Welcome).
            var all = value || []
            // A token transfer or receipt has its own progress (Tokens).
            var tokens = all.filter(function(job) { return job.kind === "token_send" || job.kind === "token_receive" })
            if (tokenJob === null && tokens.length > 0)
                tokenJob = { id: tokens[0].id, kind: tokens[0].kind, started: tokens[0].started || Date.now(), proves: true,
                             label: tokens[0].kind === "token_send" ? "Your token transfer" : "Receiving your tokens" }
            var running = all.filter(function(job) { return job.kind !== "install" && job.kind !== "shield" && job.kind !== "token_send" && job.kind !== "token_receive" })
            if (boardJob !== null || running.length === 0)
                return
            var job = running[0]
            var labels = { make_public: "Making " + job.name + " public", post: "Your post", like: "Your like", hide: "Taking it down",
                           anon: "Setting up your anon name", pay: "Your payment", pay_account: "Your payment", register: "Registering " + job.name }
            boardJob = { id: job.id, label: labels[job.kind] || "A change to " + job.name, started: job.started || Date.now(),
                         kind: job.kind, name: job.name, after: function() { loadCliMine() } }
        }, function() {})
    }

    function boardPost() {
        var text = boardComposer.text.trim()
        var cid = boardCid.text.trim()
        if (boardAs === "" || (text === "" && cid === ""))
            return
        var reply = boardReplyTo
        boardStart("board_post", [boardAs, text, JSON.stringify(cid === "" ? [] : [cid]), reply],
                   reply !== "" ? "Your reply" : "Your post", function() {
            boardComposer.text = ""
            boardCid.text = ""
            boardReplyTo = ""
        }, boardAs)
    }

    /// Checks the post being written; the composer waits for typing to pause.
    function boardSpellCheck() {
        var text = boardComposer.text
        if (text.trim() === "") {
            boardSpelling = []
            return
        }
        askLater("spell_check", [text], function(value) {
            if (boardComposer.text === text)
                boardSpelling = value || []
        }, function() { boardSpelling = [] })
    }

    /// Replaces each whole-word `word` in the post with `fix`.
    function boardFixWord(word, fix) {
        var escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
        boardComposer.text = boardComposer.text.replace(new RegExp("\\b" + escaped + "\\b", "g"), fix)
        boardSpellCheck()
    }

    function boardReply(post) {
        if (boardAs === "") {
            say("Register a name privately to reply.", true)
            return
        }
        boardReplyTo = post
        boardComposer.forceActiveFocus()
    }

    function boardFollow(name, undo) {
        askLater("board_follow", [name, undo], function(value) {
            boardFollows = value || []
            if (boardView === "following")
                boardLoad()
        }, function(message) { say(message, true) })
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.boardJob !== null
        onTriggered: { root.clockTick = Date.now(); root.boardCheckJob() }
    }
    Timer {
        interval: 15000
        repeat: true
        running: root.section === "posts" && root.boardJob === null && root.boardView !== "thread"
        onTriggered: { root.clockTick = Date.now(); root.boardLoad() }
    }
    Timer {
        id: authorRetry
        interval: 3000
        onTriggered: if (root.record !== null && root.authorOf === root.record.name) root.loadAuthorPosts(root.authorOf, !!root.authorWant.more)
    }
    Timer {
        id: cliRetry
        interval: 10000
        onTriggered: root.loadCliMine()
    }
    Timer {
        id: boardRetry
        interval: root.boardLoading ? 3000 : 10000
        onTriggered: root.boardLoad()
    }

    Component.onCompleted: {
        if (!bridgeReady)
            return
        loadConfig()
    }

    // A wallet opened after this screen is noticed without a click. Asking is
    // quick while the wallet module runs; when it does not, each ask waits a
    // few seconds inside the names module, so those are spaced well apart.
    Timer {
        interval: /not running/.test(root.walletProblem) ? 30000 : 6000
        repeat: true
        running: root.bridgeReady && root.registry !== "" && root.wallet.length === 0 && !root.busy
        onTriggered: root.loadWallet(false)
    }

    Timer {
        interval: 1500
        repeat: true
        running: root.busy
        onTriggered: root.checkPending()
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.busy && root.pending.cli === true
        onTriggered: root.checkCliJob()
    }

    // `names.open` puts a name on screen for another app, or for a
    // `basecamp://intent/names.open?p=<base64url of {"name": "jef.lez"}>` link.
    // The name arrives from outside, so it is only ever shown, never acted on.
    Connections {
        target: root.bridgeReady ? logos : null
        ignoreUnknownSignals: true
        function onIntentRequested(requestId, intent, params, requesterName) {
            if (intent !== "names.open")
                return
            var wanted = String(params && params.name ? params.name : "")
            searchField.text = wanted
            root.find(wanted)
            logos.respond(requestId, !root.noticeIsError, { name: wanted, registered: root.record !== null }, root.noticeIsError ? root.notice : "")
        }
    }

    // ── Tokens ──────────────────────────────────────────────────────────
    //
    // Tokens of a token program, other than LEZ itself, held by the names
    // CLI's accounts (the module's cli_token_* methods, docs/wallet/TOKENS.md).
    // No token program runs on the public testnet yet, so the person names one.
    // Reads come back at once with the last one the module made; a send or a
    // receipt is a wallet job this screen follows until it ends.

    property string tokenProgram: ""     // the token program the module uses, "" if none
    property bool tokenProgramEditing: false
    property var tokenHoldings: null     // [{account, account_kind, definition, name, balance}]; null until first read
    property var bridgedHoldings: []     // token_source_holdings: [{token_program, app, holdings, updating, error}]
    property var tokenPending: []        // [{position, definition, name, amount, to}]
    property bool tokenUpdating: false   // a newer read is on its way
    property string tokenError: ""       // why the last read failed
    property var tokenJob: null          // {id, kind, label, started, proves, asking}
    property bool tokenFromPrivate: true // the Send form's account: the private one, or the public one
    property string tokenProblem: ""     // why the last send or receipt didn't go through

    /// Opens Tokens, reading what the wallet holds.
    function openTokens() {
        tokenProblem = ""
        tokenProgramEditing = false
        openWallet("home")
    }

    /// Reads holdings and pending credits (the module answers at once, with
    /// what it last read, and reads again in the background).
    function loadTokens() {
        // Tokens of the token sources apps registered (bridged tokens), whatever the wallet's own program.
        askLater("token_source_holdings", [], onNet(function(value) {
            bridgedHoldings = Array.isArray(value) ? value : []
        }), function(message) {
            console.log("names_ui: token sources:", message)
        })
        if (tokenProgram === "")
            return
        askLater("cli_token_holdings", [""], onNet(function(value) {
            tokenHoldings = value.holdings === null || value.holdings === undefined ? null : value.holdings
            tokenUpdating = !!value.updating
            tokenError = value.error ? sentence(value.error) : ""
        }), function(message) {
            tokenUpdating = false
            tokenError = sentence(readFailed(message))
        })
        askLater("cli_token_pending", [], function(value) {
            tokenPending = Array.isArray(value.pending) ? value.pending : []
        }, function(message) {
            console.log("names_ui: token pending:", message)
        })
    }

    /// Has the module use the token program `id`; "" forgets it.
    function saveTokenProgram(id) {
        try {
            if (!call("names", "set_token_program", [id])) {
                say("That is not a program ID.", true)
                return
            }
        } catch (error) {
            say(error.message, true)
            return
        }
        tokenProgram = id
        tokenProgramEditing = false
        tokenHoldings = null
        tokenPending = []
        tokenError = ""
        loadTokens()
    }

    /// Why the Send form can't send as it stands, or "".
    function tokenSendProblem() {
        if (tokenJob !== null)
            return ""
        var definition = sendDefinition
        if (definition === "" || sendAmount.text.trim() === "" || sendToResolved() === "")
            return ""
        if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(definition))
            return "The token is its ID: letters and digits. Pick one you hold above, or paste it."
        var amount = Logic.amountProblem(sendAmount.text)
        return amount !== "" ? amount : Logic.tokenToProblem(sendTo.text)
    }

    /// The name the wallet knows `definition` by, from its holdings or pending credits.
    function tokenNameOf(definition) {
        var rows = (tokenHoldings || []).concat(tokenPending)
        for (var i = 0; i < rows.length; i++)
            if (rows[i].definition === definition && rows[i].name)
                return rows[i].name
        return ""
    }

    function tokenSend() {
        var problem = tokenSendProblem()
        if (problem !== "" || tokenJob !== null)
            return
        var definition = sendDefinition
        var amount = sendDigits()
        var to = sendToResolved()
        // The CLI checks the balance before it proves anything, and says so.
        tokenProblem = ""
        var label = "Sending " + grouped(amount) + " " + Logic.tokenShown(tokenNameOf(definition)) + " to " + to
        startTokenJob("cli_token_send", [definition, amount, to, tokenFromPrivate, false], label, Logic.tokenSendProves(tokenFromPrivate, to), function() {
            sendAmount.text = ""
        })
    }

    function tokenReceive() {
        if (tokenJob !== null || tokenPending.length === 0)
            return
        tokenProblem = ""
        startTokenJob("cli_token_receive", [], tokenPending.length === 1 ? "Receiving 1 credit" : "Receiving " + tokenPending.length + " credits", true, null)
    }

    /// Starts a token job and follows it until it ends.
    function startTokenJob(method, args, label, proves, after) {
        var job = { id: "", kind: method === "cli_token_send" ? "token_send" : "token_receive", label: label, started: Date.now(),
                    proves: proves, count: tokenPending.length, after: after }
        tokenJob = job
        askLater(method, args, function(value) {
            job.id = value.id
            job.started = value.started || Date.now()
            clockTick = Date.now()
        }, function(message) {
            tokenJob = null
            tokenProblem = sentence(message)
        })
    }

    function tokenCheckJob() {
        var job = tokenJob
        if (job === null || job.id === "" || job.asking)
            return
        job.asking = true
        askLater("job", [job.id], function(value) {
            job.asking = false
            if (tokenJob !== job || value.state === "running")
                return
            tokenJob = null
            if (value.state === "done") {
                if (job.kind === "token_receive")
                    say(value.nothing ? "There was nothing to receive." : "Received " + (value.applied === 1 ? "1 credit" : value.applied + " credits") + ".", false)
                else
                    say(job.label + ": done" + (value.block !== null && value.block !== undefined ? ", in block " + value.block : "") + ".", false)
                if (job.after)
                    job.after()
            } else {
                tokenProblem = sentence(Logic.jobProblem(value, "It didn't go through"))
            }
            loadTokens()
        }, function() { job.asking = false })
    }

    /// What the running token job is doing, honestly about how long it takes.
    function tokenJobLine(job) {
        var seconds = Math.max(0, Math.floor((Math.max(clockTick, Date.now()) - job.started) / 1000))
        if (!job.proves)
            return job.label + ": sending it publicly, one block (about 30 seconds), " + seconds + "s so far."
        var each = job.kind === "token_receive" && job.count > 1 ? " each" : ""
        return job.label + ": proving it on this computer, so the chain never learns the private side. This takes 7-12 minutes" + each
            + " on a fast computer; " + Math.floor(seconds / 60) + " min so far. You can keep using Names meanwhile."
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.tokenJob !== null
        onTriggered: { root.clockTick = Date.now(); root.tokenCheckJob() }
    }
    // While the module reads the wallet's tokens, ask again until the read ends.
    Timer {
        interval: 3000
        repeat: true
        running: root.section === "tokens" && root.tokenProgram !== "" && (root.tokenUpdating || (root.tokenHoldings === null && root.tokenError === ""))
        onTriggered: root.loadTokens()
    }

    // ── Apps asking the wallet (Wallet Provider SDK) ────────────────────
    //
    // Other apps ask the names module for access, payments and program calls
    // (docs/wallet/PROVIDER-SDK.md). Only this app can see and answer those
    // requests: each shows here as a sheet over everything, first the account
    // to use, then what will be signed or sent. Nothing is sent before Approve.

    property var appRequests: []          // pending_requests: the oldest first
    property var appChosen: []            // the accounts ticked on the sheet
    property string appChosenFor: ""      // which request, in which state, those ticks are for (Logic.requestTickKey)
    property bool appUnderstood: false    // an unverified program's warning was read
    property bool appSheetDetails: false  // the short sheet's Details open
    // The short sheet: what is approved in a few words, one line of who and which account, one
    // warning line with its tick when one is required, Approve / Not now; every other fact the
    // sheet has stays under Details. For the bridge's claim flow and the other one-step asks.
    readonly property bool appSheetCompact: appRequest !== null && ["relayed_call", "programs", "secret", "connect"].indexOf(appRequest.kind) >= 0
        && (appRequest.kind !== "connect" || appChosen.length > 0) && appRequest.zone_in_use !== false
    // This app's own claim screen is what asked (the bridge, for the file being claimed).
    readonly property bool appForClaim: appRequest !== null && appRequest.app === "lez_bridge" && walletView === "claim" && claimCheck !== null && claimCheck.ok === true
    // This app's own Claim or Receive screen is what asked (the bridge, inside its flow): the
    // request is that screen's step, drawn there and not as a sheet over it. Leaving the screen
    // leaves it pending, and the sheet shows it wherever the person goes in Names.
    readonly property bool appInline: appRequest !== null && appSheetCompact && appRequest.app === "lez_bridge" && section === "wallet"
        && ((walletView === "claim" && ["asking", "needs", "running", "uncertain"].indexOf(claimStageNow) >= 0)
            || (walletView === "ethreceive" && receiveNeeds !== null)
            || (walletView === "sendback" && (sbNeeds !== null || (sbFocusStatus !== null && sbFocusStatus.stage === "approve"))))
    function appSheetTitle(request) {
        if (request === null)
            return ""
        var bridge = request.app === "lez_bridge"
        if (request.kind === "relayed_call" && request.sheet && request.sheet.burn)
            return "Bridge " + request.sheet.burn.amount_text + " " + request.sheet.burn.symbol + " to Ethereum?"
        if (request.kind === "relayed_call")
            return appForClaim && claimCheck.deposit ? "Claim " + claimCheck.deposit.amount_text + " from the bridge?" : request.app + " asks you to sign"
        // Adding one account to what the app already sees (grant_add_account): the others stay.
        if (request.kind === "connect" && request.adding === true)
            return bridge ? "Let the bridge see this account too?" : request.app + " also wants to see " + String(request.account_hint || "").split("/")[0] + "/" + Logic.idShort(request.account_hint || "")
        if (request.kind === "connect")
            return bridge ? "Let the bridge see this account?" : request.app + " asks to see an account"
        if (request.kind === "programs")
            return bridge ? "Allow the bridge's programs?" : request.app + " asks to allow its programs"
        if (request.kind === "secret")
            return bridge ? (String(request.scope || "") === "RFP022/exit" ? "Allow the bridge's exit keys?" : "Allow the bridge's deposit keys?") : request.app + " asks for keys of its own"
        return appRequestTitle(request)
    }
    function appSheetLine(request) {
        if (request === null)
            return ""
        if (request.kind === "relayed_call" && request.sheet && request.sheet.burn)
            return "From your account " + Logic.idShort(request.account || "") + " · the bridge's relay pays the fee"
        if (request.kind === "relayed_call")
            return appForClaim ? "Into your account " + Logic.idShort(claimAccount) + " · the bridge pays the fee"
                : "Signs with your account " + Logic.idShort(request.account || (appChosen[0] || "")) + " · another account pays the fee"
        if (request.kind === "connect")
            return "Account " + appChosen.map(Logic.idShort).join(", ") + " · asked by " + request.app
        if (request.kind === "programs")
            return "So it can ask you to sign its claims · asked by " + request.app
        if (request.kind === "secret")
            return "From your account " + Logic.idShort(request.account || "") + " · asked by " + request.app
        return ""
    }
    /// The short sheet's tick is an ordinary one (brass) for a declaration; red only when the app
    /// may be a fake or a call can't be read.
    function appSheetWarnSoft(request) {
        return request !== null && request.kind === "programs" && !appPublisherFailed(request)
    }
    /// The one amber thing that isn't a wait: the consent box of a call the wallet truly cannot
    /// decode (a relayed call that isn't a decoded burn; provider_public.rs says so in its sheet).
    function appSheetCaution(request) {
        if (request === null || request === undefined)
            return false
        if (request.kind === "relayed_call")
            return !(request.sheet && request.sheet.burn)
        return request.kind === "call"
    }
    /// Why Approve is disabled, said under it (Logic.approveWhy).
    function appApproveWhy() {
        return Logic.approveWhy({ request: appRequest, busy: appBusy, chosen: appChosen, risky: appRequest !== null && appRequestRisky(appRequest), understood: appUnderstood })
    }
    function appSheetWarning(request) {
        if (request === null || !appRequestRisky(request))
            return ""
        if (appPublisherFailed(request))
            return "This app may not be who it says. Approve only if you trust it."
        // A declaration signs nothing: it lets the app ask later, each call shown first. The
        // warning is that the Program Registry can't vouch for the programs, not that a call is
        // unreadable.
        if (request.kind === "programs")
            return "Allow " + (request.app === "lez_bridge" ? "the LEZ Bridge's" : request.app + "'s") + " programs to ask this wallet to sign claims. Only allow programs you trust."
        // A burn the wallet decoded: its one warning (it can't be undone).
        if (request.kind === "relayed_call" && request.sheet && request.sheet.burn && (request.sheet.warnings || []).length > 0)
            return String(request.sheet.warnings[0])
        return "The wallet can't decode this call; it shows only its hash. Tick to approve it anyway, if you trust " + (request.app === "lez_bridge" ? "the bridge" : request.app) + "."
    }
    property bool appBusy: false          // an answer is on its way to the module
    property bool appNeedsChoice: false   // no account could be ticked for this request: the short sheet shows the choice
    property string appError: ""
    property string appErrorKind: "problem"   // "waiting" when appError is a busy / no-answer
    property var appGrants: []            // [{app, accounts, since}]
    readonly property var appRequest: appRequests.length > 0 ? appRequests[0] : null

    // "Checking who publishes this app… about N s": when each sheet's publisher check was first
    // seen still going (by request id), and a clock that moves only while one is.
    property var publisherSince: ({})
    property double publisherNow: Date.now()
    function publisherChecking(request) {
        return request !== null && request !== undefined && request.publisher && typeof request.publisher === "object" && request.publisher.status === "checking"
    }
    Timer {
        interval: 1000
        repeat: true
        triggeredOnStart: true
        running: root.publisherChecking(root.appRequest) || root.publisherChecking(root.inscribeRequest)
        onTriggered: {
            var since = root.publisherSince, changed = false
            ;[root.appRequest, root.inscribeRequest].forEach(function(request) {
                if (root.publisherChecking(request) && since[request.id] === undefined) {
                    since[request.id] = Date.now()
                    changed = true
                }
            })
            if (changed)
                root.publisherSince = since
            root.publisherNow = Date.now()
        }
    }
    /// The sheet's publisher line: the countdown while it is checked, else Logic.publisherLine.
    function publisherShown(request) {
        if (request === null)
            return null
        if (publisherChecking(request))
            return { text: Logic.publisherWaitLine(publisherSince[request.id], publisherNow, 15000), tone: "plain" }
        return Logic.publisherLine(request.publisher)
    }

    /// Reads the requests waiting for the person. On the first sight of a
    /// request, the account it already has (or the first offered) is ticked.
    function loadAppRequests() {
        askLater("pending_requests", [], function(value) {
            appRequests = Array.isArray(value) ? value : []
            var request = appRequest
            if (request === null) {
                appChosenFor = ""
                appChosen = []
                return
            }
            // Ticked again (and the warning's tick undone) on a new request and on any change of
            // what the sheet knows about it: the publisher check or a program label coming back
            // turns "checking" into a warning or not (M-2).
            var key = Logic.requestTickKey(request)
            if (appChosenFor !== key) {
                if (appChosenFor !== "")
                    console.log("names_ui: request " + request.id + " sheet changed: " + key)
                appChosenFor = key
                appUnderstood = false
                appSheetDetails = false
                appError = ""
                // Access for the bridge while this app's claim screen has its file: that account only.
                appChosen = Logic.preTicked(request, appPublisherFailed(request), request.app === "lez_bridge" ? claimAccount : "")
                // Nothing could be ticked for the person: the accounts show on the short sheet
                // itself, so Approve never waits on a choice hidden under Details.
                appNeedsChoice = appChosen.length === 0 && (request.choices || []).length > 0
            }
            if (request.kind === "transfer")
                loadFee()
        }, function(message) {
            console.log("names_ui: pending_requests:", message)
        }, 20000)
    }

    function loadAppGrants() {
        askLater("app_grants", [], function(value) {
            appGrants = Array.isArray(value) ? value : []
        }, function(message) {
            console.log("names_ui: app_grants:", message)
            if (waitAgain(message))
                againWhenFree("grants", loadAppGrants)
        })
    }

    /// Ticks `account` for the request on the sheet: one account for a
    /// payment or call, any number for access.
    function chooseAppAccount(account) {
        if (appRequest === null)
            return
        if (appRequest.kind !== "connect") {
            appChosen = [account]
            return
        }
        var chosen = appChosen.slice()
        var at = chosen.indexOf(account)
        if (at >= 0)
            chosen.splice(at, 1)
        else
            chosen.push(account)
        appChosen = chosen
    }

    /// Whether the sheet's request may be approved as it stands.
    function appCanApprove() {
        return Logic.canApprove({ request: appRequest, busy: appBusy, chosen: appChosen, risky: appRequest !== null && appRequestRisky(appRequest), understood: appUnderstood })
    }

    /// A program call, or a token transfer, with warnings: its program's
    /// source isn't verified, it goes to the Names registry, the token has no name.
    function appRequestRisky(request) {
        // An app whose publisher check failed may be a fake, whatever it asks (M-2).
        if (appPublisherFailed(request))
            return true
        // A burn the wallet decoded (it says what it burns): its one warning is a line, not a tick.
        if (request.kind === "relayed_call" && request.sheet && request.sheet.burn && request.sheet.refused !== true)
            return false
        // A declaration of a program the Program Registry doesn't vouch for needs the tick too.
        if (request.kind === "private_call" || request.kind === "relayed_call" || request.kind === "programs")
            return !!request.sheet && (request.sheet.warnings || []).length > 0
        return (request.kind === "call" || appIsToken(request)) && ((request.warnings || []).length > 0)
    }

    /// Whether the app claims a publisher that doesn't vouch for it.
    function appPublisherFailed(request) {
        var publisher = request ? request.publisher : null
        return !!publisher && ["mismatch", "not_published", "no_such_name"].indexOf(publisher.status) >= 0
    }

    /// Whether a transfer request moves a token rather than LEZ.
    function appIsToken(request) {
        return request !== null && request.kind === "transfer" && request.asset_kind === "token"
    }

    /// What a transfer request sends, in words: "1,000 LEZ" or "1,000 Demo".
    function appSends(request) {
        return grouped(request.amount) + " " + (appIsToken(request) ? Logic.tokenShown(request.token_name) : request.asset)
    }

    /// The token sheet's summary: "Send 1,000 Demo from Private/… to ada".
    function appTokenLine(request) {
        if (!appIsToken(request))
            return ""
        var from = appChosen.length > 0 ? accountShort(appChosen[0]) : "the account you choose below"
        return "Send " + appSends(request) + " from " + from + " to " + (request.destination_kind === "name" ? request.destination : accountShort(request.destination))
    }

    function answerAppRequest(approve) {
        var request = appRequest
        if (request === null || appBusy)
            return
        appBusy = true
        appError = ""
        var method = approve ? "approve_request" : "reject_request"
        var chosen = appChosen.slice()
        var args = approve ? [request.id, chosen.join(",")] : [request.id]
        askLater(method, args, function(value) {
            appBusy = false
            if (approve && appIsToken(request))
                say("Sending " + appSends(request) + " to " + request.destination + " for " + request.app + ". "
                    + (Logic.tokenSendProves(/^Private\//.test(chosen[0] || ""), request.destination) ? "It's proven on this computer first: 7-12 minutes." : "One block, about 30 seconds."), "waiting")
            else if (approve && request.kind === "transfer")
                say("Sending " + grouped(request.amount) + " " + request.asset + " to " + request.destination + " for " + request.app + ". It takes a few minutes.", "waiting")
            else if (approve && request.kind === "call")
                say("Sent for " + request.app + ".", false)
            else if (approve && request.kind === "private_call")
                say("Proving it for " + request.app + " on this computer: about 11 minutes.", "waiting")
            else if (approve && request.kind === "secret")
                say(request.app + " can now get its own values from " + accountShort(request.account) + ".", false)
            else if (approve && request.kind === "token_source")
                say("Its tokens now show in your holdings.", false)
            else if (approve && request.kind === "relayed_call")
                say("Signed for " + request.app + ". Its relay sends it and pays the fee.", false)
            else if (approve && request.kind === "programs")
                say(request.app + " may now ask you to sign calls to these programs; you approve each call.", false)
            else if (approve)
                say(request.app + " can now see the accounts you chose.", false)
            loadAppRequests()
            loadAppGrants()
        }, function(message) {
            appBusy = false
            appError = sentence(message)
            appErrorKind = Logic.noticeKind(appError, true)
            loadAppRequests()
        }, approve && request.kind === "call" ? 180000 : 30000)
    }

    function revokeApp(app) {
        askLater("revoke_app", [app], function() {
            say(app + " no longer has access to your accounts.", false)
            loadAppGrants()
        }, function(message) { say(message, true) })
    }

    /// The heading of a request's sheet.
    function appRequestTitle(request) {
        if (request === null)
            return ""
        if (request.kind === "connect")
            return request.app + " wants to see your accounts"
        if (request.kind === "transfer")
            return request.app + " asks you to send " + appSends(request)
        if (request.kind === "private_call")
            return request.app + " asks for a private transaction from " + accountShort(request.account)
        if (request.kind === "secret")
            return request.app + " asks for secret values of its own"
        if (request.kind === "token_source")
            return request.app + " asks to show its tokens in your holdings"
        if (request.kind === "relayed_call")
            return request.app + " asks you to sign a call another account pays for"
        if (request.kind === "programs")
            return request.app + " names the programs it will ask you to sign for"
        return request.app + " asks you to sign a program call"
    }

    /// An account as `Public/CCVd76Tk…sapKnk`: its kind in full, its ID shortened.
    function accountShort(account) {
        var slash = String(account).indexOf("/")
        return slash < 0 ? shorten(String(account)) : account.substr(0, slash + 1) + shorten(account.substr(slash + 1))
    }

    /// What the program registry says about a call's program, in words.
    function programStatus(label) {
        if (!label || !label.registry)
            return "Unknown: the Program Registry isn't installed"
        if (label.stale)
            return "Changed: it runs other code than was registered"
        if (label.verified)
            return "Verified: this computer rebuilt it from its source"
        return label.kind === "unregistered" ? "Unknown: not in the Program Registry" : "Not verified on this computer"
    }

    /// Minutes left before a request expires.
    function appExpiresIn(request) {
        var left = Math.max(0, Math.round((request.expires - Math.max(clockTick, Date.now())) / 60000))
        return left <= 1 ? "Expires in about a minute." : "Expires in " + left + " minutes."
    }

    // ── An app asking for an inscription on the Logos Blockchain ────────
    //
    // A testimonial an app wants inscribed (propose_inscription). It has its own
    // sheet: it is paid from one of the node's chain accounts, never a LEZ
    // account, and its fee is the node's own figure for the account chosen.
    // Pricing holds none of the account's funds, so it is asked as soon as an
    // account is ticked.

    property var inscribeRequests: []       // pending_inscriptions: the oldest first
    property string inscribeFrom: ""        // the chain account ticked on the sheet
    property string inscribeFor: ""         // which request that tick is for
    property var inscribePlan: null         // chain_inscribe's plan for that account
    property string inscribePlanFor: ""     // "<request id> <key>" the plan (or problem) is for
    property string inscribePlanAsked: ""   // the same, for the plan being read now
    property string inscribeProblem: ""     // why the node wouldn't price it for that account
    property bool inscribeUnderstood: false // an unvouched app's warning was read
    property bool inscribeBusy: false       // an answer is on its way to the module
    property string inscribeError: ""
    readonly property var inscribeRequest: inscribeRequests.length > 0 ? inscribeRequests[0] : null

    /// Reads the inscriptions waiting for the person. On the first sight of
    /// one, the first account that holds anything is ticked and priced.
    function loadInscribeRequests() {
        askLater("pending_inscriptions", [], function(value) {
            inscribeRequests = Array.isArray(value) ? value : []
            var request = inscribeRequest
            if (request === null) {
                inscribeFor = ""
                inscribeFrom = ""
                inscribePlan = null
                inscribePlanFor = ""
                inscribeProblem = ""
                return
            }
            var choices = request.choices || []
            // Picked again (and the warning's tick undone) on a new request and whenever its
            // publisher check changes: nothing is ticked while it is "checking" or failed (MF-1).
            var inscribeKey = Logic.requestTickKey(request)
            if (inscribeFor !== inscribeKey) {
                inscribeFor = inscribeKey
                inscribeUnderstood = false
                inscribeError = ""
                // Nothing is ticked for an app that may be a fake: choosing is deliberate (M-2).
                var publisherStatus = request.publisher && typeof request.publisher === "object" ? request.publisher.status : ""
                inscribeFrom = appPublisherFailed(request) || publisherStatus === "checking" ? "" : Logic.inscribeFirstChoice(choices)
            } else if (inscribeFrom !== "" && !request.node
                       && !choices.some(function(choice) { return choice.key === inscribeFrom })) {
                // The account left the node's wallet since it was ticked.
                inscribeFrom = ""
            }
            loadInscribePlan()
        }, function(message) {
            console.log("names_ui: pending_inscriptions:", message)
        }, 20000)
    }

    /// Asks the node what the sheet's inscription costs from the ticked account:
    /// once for each request and account, and again after a refusal that time
    /// mends (one still on its way from that account).
    function loadInscribePlan() {
        var request = inscribeRequest
        if (request === null || inscribeFrom === "" || inscribeBusy || request.node)
            return
        var wanted = request.id + " " + inscribeFrom
        if (inscribePlanAsked === wanted || (inscribePlanFor === wanted && inscribePlan !== null))
            return
        if (inscribePlanFor !== wanted) {
            inscribePlan = null
            inscribeProblem = ""
            inscribePlanFor = wanted
        }
        inscribePlanAsked = wanted
        askLater("chain_inscribe", [inscribeFrom, request.text, false], function(plan) {
            if (inscribePlanAsked === wanted)
                inscribePlanAsked = ""
            if (inscribePlanFor !== wanted)
                return
            inscribePlan = plan
            inscribeProblem = ""
        }, function(message) {
            if (inscribePlanAsked === wanted)
                inscribePlanAsked = ""
            if (inscribePlanFor !== wanted)
                return
            inscribePlan = null
            // The module being busy isn't the account's problem: it is asked again.
            inscribeProblem = waitAgain(message) ? "" : sentence(message)
        }, 30000)
    }

    function chooseInscribeAccount(key) {
        if (inscribeRequest === null || inscribeBusy)
            return
        inscribeFrom = key
        inscribeError = ""
        loadInscribePlan()
    }

    function inscribeCanApprove() {
        // Not while the Program Registry is still asked who publishes the app (the module refuses too).
        if (inscribeRequest !== null && inscribeRequest.publisher && inscribeRequest.publisher.status === "checking")
            return false
        return Logic.inscribeCanApprove({
            request: inscribeRequest, chosen: inscribeFrom, busy: inscribeBusy,
            plan: inscribePlanFor === (inscribeRequest ? inscribeRequest.id + " " + inscribeFrom : "") ? inscribePlan : null,
            problem: inscribeProblem,
            risky: inscribeRequest !== null && appPublisherFailed(inscribeRequest), understood: inscribeUnderstood })
    }

    function answerInscribeRequest(approve) {
        var request = inscribeRequest
        if (request === null || inscribeBusy || (approve && !inscribeCanApprove()))
            return
        inscribeBusy = true
        inscribeError = ""
        var from = inscribeFrom
        askLater(approve ? "approve_inscription" : "reject_request", approve ? [request.id, from] : [request.id], function(value) {
            inscribeBusy = false
            if (approve && value && value.state === "done")
                say("Inscribed for " + request.app + ", from " + Logic.keyShown(from) + ". It paid a fee of " + grouped(value.fee) + ". It's final in a few minutes.", false)
            else if (approve)
                say("The inscription for " + request.app + " may not have been sent. Your node's history shows whether it was.", true)
            loadInscribeRequests()
            loadChain()
            loadActivity()
        }, function(message) {
            inscribeBusy = false
            // No answer to an approval isn't a refusal: the node may still be proving and
            // sending it. The request leaves the list once the module has its outcome.
            inscribeError = approve && message === moduleNoAnswer
                ? "No answer yet: the inscription may still be on its way. Look at the history below before approving anything again."
                : sentence(message)
            // What it costs may have changed with what was refused.
            inscribePlanFor = ""
            loadInscribeRequests()
            loadActivity()
        // The node may take three minutes to fund it and three more to take it.
        }, approve ? 420000 : 30000)
    }

    Timer {
        interval: 3000
        repeat: true
        running: root.bridgeReady && root.configKnown && !root.appBusy
        triggeredOnStart: true
        onTriggered: { root.clockTick = Date.now(); root.loadAppRequests() }
    }
    // Its own timer: an inscription being sent doesn't stop payment requests being read.
    Timer {
        interval: 3000
        repeat: true
        running: root.bridgeReady && root.configKnown && !root.inscribeBusy
        triggeredOnStart: true
        onTriggered: root.loadInscribeRequests()
    }

    // Copy goes through a hidden TextEdit (QML has no clipboard of its own): plain text only.
    // Known limit (WALLET-0.17.md): Qt hands macOS the copied text lazily, which Screen Sharing's
    // shared clipboard may not pick up; pasting on the same Mac works.
    TextEdit { id: clipboard; visible: false; textFormat: TextEdit.PlainText }

    // ── Parts ───────────────────────────────────────────────────────────

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
        readonly property color fill: tint
        signal pressed()
        implicitWidth: actionText.implicitWidth + 28
        implicitHeight: 38
        radius: 6
        color: !live ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(fill, 1.08) : fill) : (actionArea.containsMouse ? root.raised : "transparent")
        border.width: strong && live ? 0 : 1
        border.color: activeFocus ? root.brass : root.line
        opacity: live ? 1 : 0.55
        activeFocusOnTab: live
        Keys.onReturnPressed: if (live) action.pressed()
        Keys.onSpacePressed: if (live) action.pressed()
        Text {
            id: actionText
            anchors.centerIn: parent
            text: action.label
            color: action.strong && action.live ? root.slate : root.ink
            font.family: root.sans
            font.pixelSize: root.px(18)
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
        readonly property color tint: iconArea.containsMouse || activeFocus ? root.brass : root.ink
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

    /// The small heading over the list of names.
    component ListHead: RowLayout {
        property string label: ""
        property string count: ""
        Layout.fillWidth: true
        Text {
            Layout.fillWidth: true
            text: parent.label
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(18)
            font.weight: Font.Bold
        }
        Text {
            text: parent.count
            color: root.dim
            font.family: root.sans
            font.pixelSize: root.px(17)
        }
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

    /// A quiet text button, for the small things beside the main actions.
    component Link: Text {
        id: link
        signal pressed()
        color: linkArea.containsMouse ? root.brass : root.ink
        font.family: root.sans
        font.pixelSize: root.px(17)
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
        /// Up to 39 digits rather than 15, for token amounts (128 bits).
        property bool longNumbers: false
        property bool code: false
        /// Shown as dots while typed (a recovery phrase).
        property bool secret: false
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
            validator: field.numbersOnly ? (field.longNumbers ? longDigits : digits) : null
            echoMode: field.secret ? TextInput.Password : TextInput.Normal
            onAccepted: field.accepted()
            onEditingFinished: field.edited()
            // A long value set from outside shows its start, not its end.
            onTextChanged: if (!activeFocus) cursorPosition = 0
        }
        RegularExpressionValidator { id: digits; regularExpression: /[0-9]{0,15}/ }
        RegularExpressionValidator { id: longDigits; regularExpression: /[0-9]{0,39}/ }
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
            font.pixelSize: root.px(17)
            wrapMode: Text.Wrap
        }
    }

    /// The box one task is done in: paying, editing, transferring, deleting.
    component Panel: Rectangle {
        default property alias content: panelColumn.data
        property string title: ""
        Layout.fillWidth: true
        implicitHeight: panelColumn.implicitHeight + 36
        radius: 8
        color: root.card
        border.width: 1
        border.color: root.line
        ColumnLayout {
            id: panelColumn
            anchors.fill: parent
            anchors.margins: 18
            spacing: 12
            Text {
                text: parent.parent.title
                color: root.ink
                font.family: root.sans
                font.pixelSize: root.px(19)
                font.weight: Font.DemiBold
            }
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
            font.pixelSize: root.px(15)
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
    /// One choice in a list (the Bridge's From and To): the same height and padding for
    /// every row, a dot for the chosen one.
    component PickRow: Rectangle {
        id: pick
        property string title: ""
        property string sub: ""
        property string amount: ""
        property bool current: false
        property bool enabledNow: true
        /// The face of a dropdown: a chevron instead of a dot, opened or not.
        property bool dropdown: false
        property bool open: false
        /// An option in an open dropdown: a little shorter.
        property bool compact: false
        signal pressed()
        Layout.fillWidth: true
        implicitHeight: root.px(pick.compact ? 46 : 54)
        radius: 8
        color: pick.dropdown ? root.raised : pick.current ? root.raised : pickArea.containsMouse && pick.enabledNow ? root.hover : "transparent"
        border.width: 1
        border.color: pick.dropdown ? (pick.open ? root.brass : root.line) : pick.current ? root.brass : root.line
        opacity: pick.enabledNow ? 1 : 0.5
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: root.px(14)
            anchors.rightMargin: root.px(14)
            spacing: root.px(12)
            Rectangle {
                visible: !pick.dropdown
                Layout.preferredWidth: root.px(16)
                Layout.preferredHeight: root.px(16)
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: pick.current ? root.brass : root.dim
                Rectangle {
                    anchors.centerIn: parent
                    width: parent.width / 2
                    height: width
                    radius: width / 2
                    color: root.brass
                    visible: pick.current
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Text { Layout.fillWidth: true; text: pick.title; elide: Text.ElideRight; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: pick.current ? Font.DemiBold : Font.Normal }
                Text { Layout.fillWidth: true; visible: pick.sub !== ""; text: pick.sub; elide: Text.ElideRight; color: root.dim; font.family: root.sans; font.pixelSize: root.px(13) }
            }
            Text { visible: pick.amount !== ""; text: pick.amount; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold }
            Text { visible: pick.dropdown; text: "›"; rotation: pick.open ? 270 : 90; color: root.dim; font.family: root.sans; font.pixelSize: root.px(20) }
        }
        MouseArea {
            id: pickArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: pick.enabledNow ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (pick.enabledNow) pick.pressed()
        }
    }

    /// One history entry: what, where, how much, how it stands, when.
    component ActivityRow: RowLayout {
        id: act
        property var entry: ({})
        property bool small: false
        // The account whose page shows the row, if any: its own direction and amount.
        property string account: ""
        readonly property var line: Logic.activityLine(act.entry, Math.max(root.clockTick, Date.now()), act.account)
        Layout.fillWidth: true
        Layout.leftMargin: act.small ? 0 : root.px(6)
        Layout.rightMargin: act.small ? 0 : root.px(6)
        Layout.topMargin: root.px(act.small ? 4 : 6)
        Layout.bottomMargin: root.px(act.small ? 4 : 6)
        spacing: root.px(10)
        Text {
            Layout.preferredWidth: root.px(18)
            horizontalAlignment: Text.AlignHCenter
            text: act.line.sign === "+" ? "↓" : act.line.sign === "-" ? "↑" : "⇄"
            color: act.line.sign === "+" && !act.line.quiet ? root.good : root.dim
            font.family: root.sans
            font.pixelSize: root.px(act.small ? 14 : 16)
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1
            Text { Layout.fillWidth: true; text: act.line.title; elide: Text.ElideRight; color: root.ink; font.family: root.sans; font.pixelSize: root.px(act.small ? 14 : 15) }
            // The line wraps rather than being cut: its accounts are already shortened, and
            // cutting its middle took "into Public" out of it (activity-bridge-subtitle-elided-mid-text).
            Text {
                Layout.fillWidth: true
                text: act.line.sub + " · " + act.line.ago
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                color: root.dim
                font.family: root.sans
                font.pixelSize: root.px(12)
            }
        }
        ColumnLayout {
            spacing: 1
            Layout.maximumWidth: root.px(240)
            Text {
                Layout.alignment: Qt.AlignRight
                visible: act.line.amount !== ""
                text: root.walletHidden ? "••••••" : act.line.sign + act.line.amount
                color: act.line.sign === "+" && !act.line.quiet ? root.good : root.ink
                font.family: root.sans
                font.weight: Font.DemiBold
                font.pixelSize: root.px(act.small ? 14 : 15)
            }
            Text {
                Layout.alignment: Qt.AlignRight
                Layout.maximumWidth: root.px(240)
                visible: act.line.state !== ""
                text: act.line.state
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignRight
                color: act.line.tone === "good" ? root.good : act.line.tone === "bad" ? root.alarm : act.line.tone === "warn" ? root.waitTone : root.dim
                font.family: root.sans
                font.pixelSize: root.px(12)
            }
        }
    }

    /// The LEZ Bridge's setup where it is needed (the claim screen, Receive, the LEZ 0.4 card): one
    /// line and at most one button, from the one state machine (Logic.bridgeSetupState). `then`: the
    /// screen the person goes back to when the step is over.
    /// What a request asks the wallet, and Approve / Not now: the sheet over everything for
    /// another app's request, or, for this app's own Claim and Receive (`inline`), the step itself
    /// on that screen (root.appInline). The same request, checks and answer either way.
    // The accounts a request offers, one row each (ticked ones in brass); on the full sheet and,
    // when nothing could be ticked, on the short one.
    component AccountChoices: ColumnLayout {
        id: choicesBox
        property var request: null
        property string kind: ""
        Layout.fillWidth: true
        spacing: 8
            Repeater {
                model: Logic.sheetChoices(choicesBox.request, choicesBox.request !== null && choicesBox.request.app === "lez_bridge" ? root.claimAccount : "")
                Rectangle {
                    id: choiceRow
                    required property var modelData
                    readonly property bool ticked: root.appChosen.indexOf(modelData.account) >= 0
                    Layout.fillWidth: true
                    implicitHeight: root.px(44)
                    radius: 6
                    color: ticked ? root.raised : "transparent"
                    border.width: 1
                    border.color: ticked ? root.brass : root.line
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 10
                        Rectangle {
                            Layout.preferredWidth: 18
                            Layout.preferredHeight: 18
                            radius: choicesBox.kind === "connect" ? 4 : 9
                            color: choiceRow.ticked ? root.brass : "transparent"
                            border.width: 2
                            border.color: choiceRow.ticked ? root.brass : root.dim
                        }
                        Text { text: root.accountShort(choiceRow.modelData.account); color: root.ink; font.family: root.mono; font.pixelSize: root.px(16) }
                        Text { text: choiceRow.modelData.label; color: root.dim; font.family: root.sans; font.pixelSize: root.px(15); elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { visible: !!choiceRow.modelData.granted; text: "Shared with this app"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.chooseAppAccount(choiceRow.modelData.account)
                    }
                }
            }
    }

    component ApproveBody: ColumnLayout {
        id: sheetBody
        property bool inline: false
        readonly property var request: root.appRequest
        readonly property string kind: request !== null ? request.kind : ""
        Layout.fillWidth: true
        spacing: 14
        // The short sheet (appSheetCompact): everything a person needs to decide, above the fold.
        Text {
            visible: root.appSheetCompact
            Layout.fillWidth: true
            text: root.appSheetTitle(sheetBody.request)
            textFormat: Text.PlainText
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(sheetBody.inline ? 20 : 24)
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
        Hint { visible: root.appSheetCompact; color: root.ink; textFormat: Text.PlainText; text: root.appSheetLine(sheetBody.request) }
        Hint { visible: root.appSheetCompact && root.publisherChecking(sheetBody.request); font.pixelSize: root.px(15); text: root.publisherShown(sheetBody.request) !== null ? root.publisherShown(sheetBody.request).text : "" }
        Tick {
            visible: root.appSheetCompact && root.appSheetWarning(sheetBody.request) !== ""
            checked: root.appUnderstood
            caution: root.appSheetCaution(sheetBody.request)
            text: root.appSheetWarning(sheetBody.request)
            onToggled: root.appUnderstood = !root.appUnderstood
        }
        // No account could be ticked for the person: the choice is here, not only under Details.
        ColumnLayout {
            visible: root.appSheetCompact && root.appNeedsChoice
            Layout.fillWidth: true
            spacing: 8
            Hint { color: root.ink; text: "Choose the account:" }
            AccountChoices { request: sheetBody.request; kind: sheetBody.kind }
        }
        // A decoded burn's warning (it can't be undone), as a line: nothing to tick.
        Hint {
            readonly property var burnSheet: sheetBody.request !== null && sheetBody.request.kind === "relayed_call" && sheetBody.request.sheet && sheetBody.request.sheet.burn ? sheetBody.request.sheet : null
            visible: root.appSheetCompact && burnSheet !== null && (burnSheet.warnings || []).length > 0 && root.appSheetWarning(sheetBody.request) === ""
            color: root.ink
            textFormat: Text.PlainText
            text: burnSheet !== null && (burnSheet.warnings || []).length > 0 ? String(burnSheet.warnings[0]) : ""
        }
        RowLayout {
            visible: root.appSheetCompact
            spacing: 10
            Action {
                label: root.appBusy ? "Answering…" : sheetBody.request !== null && sheetBody.request.checking === true ? "Waiting for the check…" : "Approve"
                strong: true
                enabledNow: root.appCanApprove()
                onPressed: root.answerAppRequest(true)
            }
            Action { label: "Reject"; enabledNow: !root.appBusy; onPressed: root.answerAppRequest(false) }
        }
        WhyLine { visible: root.appSheetCompact && text !== "" && !root.appBusy; text: root.appApproveWhy() }
        StatusLine { visible: root.appSheetCompact && root.appError !== ""; kind: root.appErrorKind; text: root.appError }
        Link {
            visible: root.appSheetCompact
            text: root.appSheetDetails ? "Details ▾" : "Details ▸"
            color: root.dim
            font.pixelSize: root.px(15)
            onPressed: root.appSheetDetails = !root.appSheetDetails
        }
        // The full sheet: shown as it is, or under Details on the short sheet.
        ColumnLayout {
            visible: !root.appSheetCompact || root.appSheetDetails
            Layout.fillWidth: true
            spacing: 14
        Text {
            visible: !root.appSheetCompact
            Layout.fillWidth: true
            text: root.appRequestTitle(sheetBody.request)
            textFormat: Text.PlainText
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(24)
            font.weight: Font.DemiBold
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
        }
        Hint {
            font.pixelSize: root.px(15)
            text: sheetBody.request === null ? "" : "Asked by the app “" + sheetBody.request.app + "”, as Basecamp names it. "
                + root.appExpiresIn(sheetBody.request) + (root.appRequests.length > 1 ? " " + (root.appRequests.length - 1) + " more waiting." : "")
        }
        // Who publishes the app, checked against its LNS name by the Program Registry.
        Text {
            readonly property var line: root.publisherShown(sheetBody.request)
            visible: line !== null
            Layout.fillWidth: true
            text: line === null ? "" : line.text
            textFormat: Text.PlainText
            color: line === null ? root.dim : line.tone === "good" ? root.good : line.tone === "warn" ? root.ink : root.dim
            font.family: root.sans
            font.pixelSize: root.px(16)
            font.weight: line !== null && line.tone !== "plain" ? Font.DemiBold : Font.Normal
            wrapMode: Text.WordWrap
        }
        // Still asking the Program Registry: Approve waits for the answer (M-2).
        Text {
            visible: sheetBody.request !== null && sheetBody.request.checking === true && String(sheetBody.request.checking_what || "").indexOf("who publishes") !== 0
            Layout.fillWidth: true
            text: sheetBody.request === null ? "" : "Checking " + String(sheetBody.request.checking_what || "") + " with the Program Registry… Approve waits for the answer."
            textFormat: Text.PlainText
            color: root.dim
            font.family: root.sans
            font.pixelSize: root.px(16)
            wrapMode: Text.WordWrap
        }

        // Access: why, and what sharing means.
        Hint {
            visible: sheetBody.kind === "connect" && sheetBody.request.reason !== ""
            textFormat: Text.PlainText
            color: root.ink
            text: sheetBody.kind === "connect" ? "It says: “" + sheetBody.request.reason + "”" : ""
        }
        Hint {
            visible: sheetBody.kind === "connect"
            text: "Choose the accounts it may see. It will see their addresses and balances, and can then ask you to approve payments. It can't send anything without you."
        }

        // A token transfer: in one line, then the token, its program and how that program stands.
        Hint {
            visible: root.appIsToken(sheetBody.request)
            textFormat: Text.PlainText
            color: root.ink
            font.pixelSize: root.px(19)
            font.weight: Font.DemiBold
            text: root.appTokenLine(sheetBody.request)
        }
        ColumnLayout {
            visible: root.appIsToken(sheetBody.request)
            Layout.fillWidth: true
            spacing: 6
            Fact { label: "Token"; value: root.appIsToken(sheetBody.request) ? Logic.tokenShown(sheetBody.request.token_name) + " (named by its maker; not LEZ)" : "" }
            Fact { label: "Token ID"; code: true; value: root.appIsToken(sheetBody.request) ? root.shorten(sheetBody.request.asset) : ""; full: root.appIsToken(sheetBody.request) ? sheetBody.request.asset : "" }
            Fact {
                label: "Program"
                value: !root.appIsToken(sheetBody.request) ? ""
                    : sheetBody.request.program_label && sheetBody.request.program_label.text ? sheetBody.request.program_label.text
                    : root.shorten(sheetBody.request.token_program)
                full: root.appIsToken(sheetBody.request) ? sheetBody.request.token_program : ""
            }
            Fact { label: "Source"; value: root.appIsToken(sheetBody.request) ? root.programStatus(sheetBody.request.program_label) : "" }
            Fact {
                label: "Time"
                value: !root.appIsToken(sheetBody.request) ? ""
                    : Logic.tokenSendProves(/^Private\//.test(root.appChosen[0] || ""), sheetBody.request.destination)
                    ? "Proven on this computer first: 7-12 minutes"
                    : "One block, about 30 seconds; public: everyone sees who sent it"
            }
            Hint { visible: root.appIsToken(sheetBody.request) && Logic.tokenSendProves(/^Private\//.test(root.appChosen[0] || ""), sheetBody.request.destination) && root.proofWarning !== ""; color: root.ink; text: root.proofWarning }
            Repeater {
                model: root.appIsToken(sheetBody.request) ? (sheetBody.request.warnings || []) : []
                Hint {
                    required property string modelData
                    textFormat: Text.PlainText
                    color: root.ink
                    text: "!  " + modelData
                }
            }
        }

        // A payment: asset, amount, destination.
        ColumnLayout {
            visible: sheetBody.kind === "transfer"
            Layout.fillWidth: true
            spacing: 6
            Fact { visible: root.zoneFact(sheetBody.request) !== ""; label: "Zone"; value: root.zoneFact(sheetBody.request) }
            Fact { visible: !root.appIsToken(sheetBody.request); label: "Asset"; value: sheetBody.kind === "transfer" ? sheetBody.request.asset + " (the native token)" : "" }
            Fact { label: "Amount"; value: sheetBody.kind === "transfer" ? root.appSends(sheetBody.request) : "" }
            Fact {
                label: "To"
                code: sheetBody.kind === "transfer" && sheetBody.request.destination_kind === "account"
                value: sheetBody.kind !== "transfer" ? "" : sheetBody.request.destination_kind === "name" ? sheetBody.request.destination + " (a name)" : sheetBody.request.destination
                full: sheetBody.kind === "transfer" ? sheetBody.request.destination : ""
            }
            Fact {
                label: "Network fee"
                value: {
                    if (sheetBody.kind !== "transfer")
                        return ""
                    var fromPrivate = /^Private\//.test(root.appChosen[0] || "")
                    // A token transfer runs the token program: its gas is sized when it's tried.
                    if (root.appIsToken(sheetBody.request) && !Logic.tokenSendProves(fromPrivate, sheetBody.request.destination))
                        return "Sized when it's tried, just before sending"
                    var estimate = Logic.feeEstimate(root.feeQuote, fromPrivate || sheetBody.request.destination_kind === "name")
                    return estimate !== "" ? estimate : "Reading the chain's fees…"
                }
            }
        }

        // A program call: what it is, what it can do, what the app says.
        ColumnLayout {
            visible: sheetBody.kind === "call"
            Layout.fillWidth: true
            spacing: 6
            Fact { visible: root.zoneFact(sheetBody.request) !== ""; label: "Zone"; value: root.zoneFact(sheetBody.request) }
            Fact {
                label: "Program"
                value: sheetBody.kind !== "call" ? "" : sheetBody.request.program_label && sheetBody.request.program_label.text ? sheetBody.request.program_label.text : root.shorten(sheetBody.request.program)
            }
            Fact { label: "Program ID"; code: true; value: sheetBody.kind === "call" ? root.shorten(sheetBody.request.program) : ""; full: sheetBody.kind === "call" ? sheetBody.request.program : "" }
            Fact {
                label: "Source"
                value: sheetBody.kind === "call" ? root.programStatus(sheetBody.request.program_label) : ""
            }
            Fact { label: "Data"; value: sheetBody.kind === "call" ? sheetBody.request.instruction_len + " bytes" : "" }
            Fact { label: "Gas"; value: "Sized when it's tried, just before sending; if the call would fail, nothing is sent" }
            Part { text: "What it can do" }
            Repeater {
                model: sheetBody.kind === "call" ? (sheetBody.request.effects || []) : []
                Hint {
                    required property string modelData
                    textFormat: Text.PlainText
                    color: root.ink
                    text: "•  " + modelData
                }
            }
            Repeater {
                model: sheetBody.kind === "call" ? (sheetBody.request.warnings || []) : []
                Hint {
                    required property string modelData
                    textFormat: Text.PlainText
                    color: root.ink
                    text: "!  " + modelData
                }
            }
            Hint {
                visible: sheetBody.kind === "call" && sheetBody.request.app_summary !== ""
                textFormat: Text.PlainText
                font.pixelSize: root.px(15)
                text: sheetBody.kind === "call" ? "The app describes it as: “" + sheetBody.request.app_summary + "” (its own words, not checked)" : ""
            }
        }

        // A private call: what the wallet's own simulation shows it does. The app's
        // words are shown apart and never decide anything.
        ColumnLayout {
            visible: sheetBody.kind === "private_call"
            Layout.fillWidth: true
            spacing: 6
            readonly property var sheet: sheetBody.kind === "private_call" && sheetBody.request.sheet ? sheetBody.request.sheet : null
            Fact { visible: root.zoneFact(sheetBody.request) !== ""; label: "Zone"; value: root.zoneFact(sheetBody.request) }
            Fact { label: "From"; code: true; value: sheetBody.kind === "private_call" ? root.accountShort(sheetBody.request.account) : ""; full: sheetBody.kind === "private_call" ? sheetBody.request.account : "" }
            StatusLine {
                visible: sheetBody.kind === "private_call" && sheetBody.request.check !== "ready"
                kind: sheetBody.kind === "private_call" && sheetBody.request.check === "failed" ? "problem" : "working"
                text: sheetBody.kind !== "private_call" ? ""
                    : sheetBody.request.check === "failed" ? "The wallet couldn't simulate it, so it can't be approved: " + (sheetBody.request.check_problem || "")
                    : "Simulating it against the chain to see what it really does…"
            }
            Part { visible: parent.sheet !== null; text: "What it does (simulated by your wallet)" }
            Repeater {
                model: parent.sheet ? parent.sheet.lines : []
                Hint { required property string modelData; textFormat: Text.PlainText; color: root.ink; text: "•  " + modelData }
            }
            Repeater {
                model: parent.sheet ? parent.sheet.refusals : []
                StatusLine { required property string modelData; kind: "problem"; text: modelData }
            }
            Part { visible: parent.sheet !== null; text: "Programs" }
            Repeater {
                model: parent.sheet ? parent.sheet.programs : []
                Fact {
                    required property var modelData
                    label: "Program"
                    value: (modelData.label && modelData.label.text ? modelData.label.text + " · " : "") + root.shorten(modelData.program) + " · " + root.programStatus(modelData.label)
                           + (modelData.new_for_app ? " · new for this app" : "")
                }
            }
            Part { visible: parent.sheet !== null && parent.sheet.public_changes.length > 0; text: "Public changes" }
            Repeater {
                model: parent.sheet ? parent.sheet.public_changes : []
                Hint { required property string modelData; textFormat: Text.PlainText; color: root.ink; text: "•  " + modelData }
            }
            Part { visible: parent.sheet !== null; text: "Privacy" }
            Repeater {
                model: parent.sheet ? parent.sheet.privacy : []
                Hint { required property string modelData; textFormat: Text.PlainText; color: root.ink; text: "•  " + modelData }
            }
            Repeater {
                model: parent.sheet ? parent.sheet.warnings : []
                Hint { required property string modelData; textFormat: Text.PlainText; color: root.ink; text: "!  " + modelData }
            }
            Fact { label: "Data"; value: sheetBody.kind === "private_call" ? sheetBody.request.message_len + " bytes, proven, not published" : "" }
            Hint { visible: sheetBody.kind === "private_call"; textFormat: Text.PlainText; color: root.ink; text: sheetBody.kind === "private_call" ? sheetBody.request.prove_expect : "" }
            Hint { visible: sheetBody.kind === "private_call" && root.proofWarning !== ""; color: root.ink; text: root.proofWarning }
            Hint {
                visible: sheetBody.kind === "private_call" && sheetBody.request.app_summary !== ""
                textFormat: Text.PlainText
                font.pixelSize: root.px(15)
                text: sheetBody.kind === "private_call" ? "The app describes it as: “" + sheetBody.request.app_summary + "” (its own words, not checked)" : ""
            }
        }

        // A secret grant: what the app gets, and what it can't do with it.
        ColumnLayout {
            visible: sheetBody.kind === "secret"
            Layout.fillWidth: true
            spacing: 6
            Fact { label: "Account"; code: true; value: sheetBody.kind === "secret" ? root.accountShort(sheetBody.request.account) : ""; full: sheetBody.kind === "secret" ? sheetBody.request.account : "" }
            Fact { label: "For"; value: sheetBody.kind === "secret" ? sheetBody.request.scope : "" }
            Hint { visible: sheetBody.kind === "secret"; textFormat: Text.PlainText; color: root.ink; text: sheetBody.kind === "secret" ? sheetBody.request.explanation : "" }
        }

        // A call someone else pays for: the wallet's own build, in plain words (LEZ 0.4 only).
        ColumnLayout {
            id: relayedBox
            visible: sheetBody.kind === "relayed_call"
            Layout.fillWidth: true
            spacing: 6
            readonly property var shown: Logic.relayedSheet(sheetBody.kind === "relayed_call" ? sheetBody.request.sheet : null)
            Repeater {
                model: relayedBox.shown.lines
                Hint { required property var modelData; textFormat: Text.PlainText; color: root.ink; text: modelData }
            }
            Repeater {
                model: relayedBox.shown.warnings
                Hint { required property var modelData; textFormat: Text.PlainText; color: root.ink; text: "!  " + modelData }
            }
            Hint { textFormat: Text.PlainText; color: root.ink; text: "Privacy: " + relayedBox.shown.privacy }
            Hint { textFormat: Text.PlainText; text: relayedBox.shown.after }
            Hint { visible: relayedBox.shown.says !== ""; textFormat: Text.PlainText; text: "The app describes it as: “" + relayedBox.shown.says + "” (its own words, not checked)" }
            Repeater {
                model: relayedBox.shown.why
                StatusLine { required property var modelData; kind: "problem"; text: "Refused: " + modelData }
            }
        }

        // Programs an app declares it will ask the wallet to sign for.
        ColumnLayout {
            visible: sheetBody.kind === "programs"
            Layout.fillWidth: true
            spacing: 6
            // Each program in full, with what the Program Registry says of it.
            Repeater {
                model: sheetBody.kind === "programs" && sheetBody.request.sheet ? sheetBody.request.sheet.programs : []
                ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 2
                    Hint { textFormat: Text.PlainText; color: root.ink; font.family: root.mono; text: modelData.program }
                    Hint {
                        textFormat: Text.PlainText
                        text: modelData.label && modelData.label.text ? "Program Registry: " + Logic.clean(modelData.label.text) + (modelData.label.verified ? " (source verified)" : " (source not verified)")
                            : modelData.label && modelData.label.registry ? "Not in the Program Registry." : "The Program Registry isn't installed, so this program has no label."
                    }
                }
            }
            Repeater {
                model: sheetBody.kind === "programs" && sheetBody.request.sheet ? sheetBody.request.sheet.warnings : []
                Hint { required property var modelData; textFormat: Text.PlainText; color: root.ink; text: "!  " + Logic.clean(modelData) }
            }
            Hint { visible: sheetBody.kind === "programs" && sheetBody.request.app_summary !== ""; textFormat: Text.PlainText; text: sheetBody.kind === "programs" ? "The app says why: “" + Logic.clean(sheetBody.request.app_summary) + "” (its own words, not checked)" : "" }
            Hint { visible: sheetBody.kind === "programs"; textFormat: Text.PlainText; color: root.ink; text: "Approving signs nothing. It lets this app ask you to sign calls to these programs only, each of them shown to you first. Settings → Connected apps takes it back." }
        }

        // A token source: which tokens it adds to the holdings view.
        ColumnLayout {
            visible: sheetBody.kind === "token_source"
            Layout.fillWidth: true
            spacing: 6
            Fact { label: "Token program"; code: true; value: sheetBody.kind === "token_source" ? root.shorten(sheetBody.request.token_program) : ""; full: sheetBody.kind === "token_source" ? sheetBody.request.token_program : "" }
            Repeater {
                model: sheetBody.kind === "token_source" ? sheetBody.request.definitions : []
                Fact {
                    required property var modelData
                    label: modelData.symbol !== "" ? modelData.symbol : "Token"
                    value: modelData.name + " · " + modelData.decimals + " decimals · " + root.shorten(modelData.definition) + (modelData.origin !== "" ? " · " + modelData.origin : "")
                }
            }
            Hint { visible: sheetBody.kind === "token_source"; textFormat: Text.PlainText; color: root.ink; text: "The names and decimals are the app's words. Approving only lists these tokens with your holdings; it moves nothing." }
        }

        // The account to use.
        Part { text: sheetBody.kind === "connect" ? "Accounts it may see" : sheetBody.kind === "call" || sheetBody.kind === "relayed_call" ? "Sign with" : sheetBody.kind === "private_call" || sheetBody.kind === "secret" ? "Account" : sheetBody.kind === "token_source" || sheetBody.kind === "programs" ? "Confirm" : "Pay from" }
        AccountChoices { request: sheetBody.request; kind: sheetBody.kind }
        StatusLine {
            visible: sheetBody.request !== null && (sheetBody.request.choices || []).length === 0
            kind: "problem"
            text: sheetBody.kind === "call" ? "No account can sign this yet. Your wallet's public accounts are being read; if none shows, create your wallet first (Welcome, Create my wallet)."
                : "Your wallet has no accounts yet. Create your wallet first (Welcome, Create my wallet)."
        }

        }

        // A risky call needs a second, deliberate tick.
        Tick {
            visible: !root.appSheetCompact && sheetBody.request !== null && root.appRequestRisky(sheetBody.request)
            checked: root.appUnderstood
            caution: root.appSheetCaution(sheetBody.request)
            text: root.appIsToken(sheetBody.request) ? "I've read the warnings above and still want to send this token."
                : "I've read the warnings above and trust " + (sheetBody.request !== null ? sheetBody.request.app : "this app") + " with this account."
            onToggled: root.appUnderstood = !root.appUnderstood
        }

        ColumnLayout {
            visible: sheetBody.request !== null && sheetBody.request.zone_in_use === false
            Layout.fillWidth: true
            spacing: 8
            StatusLine {
                kind: "problem"
                text: sheetBody.request === null || sheetBody.request.zone_in_use !== false ? ""
                    : sheetBody.request.zone_removed ? "This was asked for a zone that has been removed since. Reject it, and the app can ask again."
                    : "This was asked for " + sheetBody.request.zone_name + ", and the wallet is using " + root.zoneName + " now. Switch to " + sheetBody.request.zone_name + " to approve it, or reject it."
            }
            Action {
                visible: sheetBody.request !== null && !sheetBody.request.zone_removed
                label: root.zoneBusy ? "Switching…" : "Switch to " + (sheetBody.request !== null && sheetBody.request.zone_name ? sheetBody.request.zone_name : "it")
                enabledNow: !root.zoneBusy && !root.appBusy
                onPressed: root.zoneUse(sheetBody.request.zone)
            }
            StatusLine { visible: root.zoneNoteBad; kind: "problem"; text: root.zoneNote }
        }
        RowLayout {
            visible: !root.appSheetCompact
            Layout.topMargin: 8
            spacing: 10
            Action {
                label: sheetBody.request !== null && sheetBody.request.checking === true ? "Waiting for the check…" : sheetBody.kind === "connect" ? "Share" : sheetBody.kind === "call" ? "Approve and sign" : sheetBody.kind === "private_call" ? "Approve, prove and send" : sheetBody.kind === "secret" ? "Allow" : sheetBody.kind === "token_source" ? "Show these tokens" : sheetBody.kind === "relayed_call" ? "Approve" : sheetBody.kind === "programs" ? "Allow these programs" : "Approve and send"
                strong: true
                enabledNow: root.appCanApprove()
                onPressed: root.answerAppRequest(true)
            }
            Action {
                label: "Reject"
                enabledNow: !root.appBusy
                onPressed: root.answerAppRequest(false)
            }
        }
        WhyLine { visible: !root.appSheetCompact && text !== "" && !root.appBusy; text: root.appApproveWhy() }
        StatusLine { visible: !root.appSheetCompact && root.appBusy; kind: sheetBody.kind === "call" ? "waiting" : "working"; text: sheetBody.kind === "call" ? "Trying it, then signing and sending it. This waits for its block, up to two minutes…" : "Answering it…" }
        StatusLine { visible: !root.appSheetCompact && root.appError !== ""; kind: root.appErrorKind; text: root.appError }
    }

    /// The steps in one row, "✓ Arrived  ✓ Checked  3 Approve  4 Claim": done ones ticked, the
    /// current one marked, later ones dim. Wraps when the pane is narrow.
    component StepRow: Flow {
        id: stepRow
        property var steps: []
        Layout.fillWidth: true
        spacing: 6
        Repeater {
            model: stepRow.steps
            Row {
                id: stepCell
                required property var modelData
                required property int index
                spacing: 6
                Text { visible: stepCell.index > 0; anchors.verticalCenter: parent.verticalCenter; text: "›"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(16) }
                Rectangle {
                    readonly property bool now: stepCell.modelData.state === "now"
                    implicitWidth: stepLabel.implicitWidth + 22
                    implicitHeight: root.px(32)
                    width: implicitWidth
                    height: implicitHeight
                    radius: height / 2
                    color: now ? root.raised : "transparent"
                    border.width: now ? 2 : 1
                    border.color: now ? root.ink : root.line
                    Text {
                        id: stepLabel
                        anchors.centerIn: parent
                        text: (stepCell.modelData.state === "done" ? "✓ " : stepCell.modelData.n + " ") + stepCell.modelData.short
                        color: stepCell.modelData.state === "done" ? root.ink : parent.now ? root.ink : root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(15)
                        font.weight: parent.now ? Font.Bold : Font.Medium
                    }
                }
            }
        }
    }

    /// A panel for something of yours under way (a receive, a bridge to Ethereum): a neutral box;
    /// its status lines carry the colour (STYLE.md).
    component UnderwayPanel: Rectangle {
        default property alias content: underwayColumn.data
        Layout.fillWidth: true
        implicitHeight: underwayColumn.implicitHeight + 36
        radius: 10
        color: root.raised
        border.width: 1
        border.color: root.line
        ColumnLayout {
            id: underwayColumn
            x: 24
            y: 18
            width: parent.width - 44
            spacing: 10
        }
    }
    /// Check now (a button: "Checking…" at once, then the answer) and "Checked just now", with
    /// "next check in N min" only when a check by itself is due (Logic.bridgeNextCheck).
    component ReceiveCheckLine: RowLayout {
        spacing: 12
        Action { readonly property var b: Logic.checkButton(root.receivePolling, root.receiveAskNow); label: b.label; enabledNow: b.enabled; onPressed: root.receiveCheckNow() }
        Text { text: root.receiveCheckLine; visible: text !== "" && text !== "Checking…"; color: root.receiveCheckFailed && !root.receivePolling ? root.waitTone : root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
    }
    component SendBackCheckLine: RowLayout {
        spacing: 12
        Action { readonly property var b: Logic.checkButton(root.sbPolling, root.sbAskNow); label: b.label; enabledNow: b.enabled && !root.sbAsking; onPressed: root.sbCheckNow() }
        Text { text: Logic.receiveCheckLine(root.sbPolling, root.sbCheckedAt, root.sbNextShown, root.receiveNow, root.sbCheckFailed); visible: text !== "" && text !== "Checking…"; color: root.sbCheckFailed && !root.sbPolling ? root.waitTone : root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
    }

    component BridgeSetupBox: RowLayout {
        id: setupBox
        property string then: ""
        // Yellow only where it holds up what the screen is for (Receive, Claim, an install);
        // the card's Update for a bridge that still receives is a plain button.
        property bool primary: true
        readonly property var setup: root.bridgeSetup
        Layout.fillWidth: true
        spacing: 12
        visible: setupBox.setup.state !== ""
        StatusLine {
            Layout.fillWidth: true
            kind: Logic.setupKind(setupBox.setup.state)
            text: setupBox.setup.line
        }
        Action {
            visible: setupBox.setup.button !== ""
            label: setupBox.setup.button
            strong: setupBox.primary && setupBox.setup.action !== "settings" && !root.bridgeSetupOptional
            onPressed: root.bridgeSetupAction(setupBox.setup.action, setupBox.then)
        }
    }

    /// A QR code drawn from the module's rows of "0"/"1" (QML has no QR library), with its quiet zone.
    component QrCode: Canvas {
        id: qr
        property var rows: []
        property real side: 180
        implicitWidth: side
        implicitHeight: side
        onRowsChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.fillStyle = "#ffffff"
            ctx.fillRect(0, 0, width, height)
            var n = rows && rows.length ? rows.length : 0
            if (n === 0)
                return
            var cell = width / (n + 8)
            ctx.fillStyle = "#000000"
            for (var y = 0; y < n; y++)
                for (var x = 0; x < rows[y].length; x++)
                    if (rows[y].charAt(x) === "1")
                        ctx.fillRect((x + 4) * cell, (y + 4) * cell, Math.ceil(cell), Math.ceil(cell))
        }
    }

    component Tab: Rectangle {
        id: tab
        property string label: ""
        property bool current: false
        /// Labels sit at the left in a tab that fills its row (the sidebar).
        property bool alignLeft: false
        signal pressed()
        implicitHeight: 44
        implicitWidth: tabText.implicitWidth + 32
        radius: 8
        color: tab.current ? root.raised : tabArea.containsMouse ? root.hover : "transparent"
        Text {
            id: tabText
            anchors.verticalCenter: parent.verticalCenter
            anchors.horizontalCenter: tab.alignLeft ? undefined : parent.horizontalCenter
            anchors.left: tab.alignLeft ? parent.left : undefined
            anchors.leftMargin: 14
            text: tab.label
            color: tab.current ? root.ink : root.dim
            font.family: root.sans
            font.pixelSize: root.px(17)
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
    /// A Welcome step's heading: its number, ticked once done, and its title.
    component StepHead: RowLayout {
        id: stepHead
        property int number: 1
        property bool done: false
        property string title: ""
        spacing: 12
        Rectangle {
            implicitWidth: root.px(30)
            implicitHeight: root.px(30)
            radius: height / 2
            color: stepHead.done ? root.good : root.brass
            Text { anchors.centerIn: parent; text: stepHead.done ? "✓" : String(stepHead.number); color: root.slate; font.family: root.sans; font.pixelSize: root.px(15); font.weight: Font.DemiBold }
        }
        Text { text: stepHead.title; color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.DemiBold }
    }

    component Card: Rectangle {
        default property alias content: cardColumn.data
        implicitHeight: cardColumn.implicitHeight + 52
        radius: 12
        color: root.card
        border.width: 1
        border.color: root.line
        ColumnLayout {
            id: cardColumn
            anchors.fill: parent
            anchors.margins: 26
            spacing: 14
        }
    }

    /// The steps of a private registration; `active` (1 to 3) marks the one
    /// under way, the ones before it done. 0 shows them as a plan.
    component Steps: ColumnLayout {
        id: steps
        property int active: 0
        property string note: ""
        spacing: 12
        Repeater {
            model: root.registerSteps
            RowLayout {
                id: stepRow
                required property var modelData
                required property int index
                readonly property bool done: steps.active > index + 1
                readonly property bool now: steps.active === index + 1
                Layout.fillWidth: true
                spacing: 14
                Rectangle {
                    Layout.alignment: Qt.AlignTop
                    width: 30
                    height: 30
                    radius: 15
                    color: stepRow.now ? root.brass : stepRow.done ? root.good : "transparent"
                    border.width: stepRow.now || stepRow.done ? 0 : 1
                    border.color: root.line
                    Text {
                        anchors.centerIn: parent
                        text: stepRow.done ? "✓" : String(stepRow.index + 1)
                        color: stepRow.now || stepRow.done ? root.slate : root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(15)
                        font.weight: Font.DemiBold
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        Layout.fillWidth: true
                        text: stepRow.modelData.title + "  ·  " + (stepRow.now && steps.note !== "" ? steps.note : stepRow.modelData.time)
                        color: stepRow.now ? root.waitTone : root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(17)
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.fillWidth: true
                        text: stepRow.modelData.why
                        color: root.faint
                        font.family: root.sans
                        font.pixelSize: root.px(14)
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }

    /// A small label over a value.
    component Figure: ColumnLayout {
        property string label: ""
        property string value: ""
        property string note: ""
        spacing: 4
        Text { text: parent.label; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
        Text { text: parent.value; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.DemiBold }
        Text { visible: parent.note !== ""; Layout.fillWidth: true; text: parent.note; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14); wrapMode: Text.Wrap }
    }

    /// A line of explanation under a control.
    /// Which network an action or view runs on, in the words the app uses everywhere:
    /// `net` is "0.3" (LEZ 0.3, the public Logos testnet), "0.4" (LEZ 0.4, the bridge test
    /// network), "chain" (the Logos Blockchain), "both" (the Logos Blockchain into LEZ 0.3) or
    /// "0.3+chain" (the balances of LEZ 0.3 and of the Logos Blockchain) or "messages" (chat).
    component RunsOn: Text {
        property string net: root.namesNet
        property string prefix: "Runs on"
        Layout.fillWidth: true
        textFormat: Text.StyledText
        wrapMode: Text.Wrap
        color: root.dim
        font.family: root.sans
        font.pixelSize: root.px(14)
        text: prefix + " " + (net === "0.4" ? "<b>LEZ 0.4</b> · bridge test network"
            : net === "chain" ? "<b>the Logos Blockchain</b>"
            : net === "both" ? "<b>the Logos Blockchain</b> and <b>LEZ 0.3</b> · public Logos testnet"
            : net === "0.3+chain" ? "<b>LEZ 0.3</b> · public Logos testnet, and on <b>the Logos Blockchain</b>"
            : net === "messages" ? "<b>Logos Messaging</b>; LNS names are looked up on " + (root.namesNet === "0.4" ? "<b>LEZ 0.4</b> · bridge test network" : "<b>LEZ 0.3</b> · public Logos testnet")
            : "<b>LEZ 0.3</b> · public Logos testnet")
    }

    /// The names network switch: LEZ 0.3 | LEZ 0.4. Shown only beside actions that work on
    /// both; everything else says the one it runs on (RunsOn).
    component NetSwitch: ColumnLayout {
        Layout.fillWidth: true
        spacing: 6
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Accessible.name: "Network"
            Repeater {
                model: [Logic.NET_03, Logic.NET_04]
                Rectangle {
                    id: netChoice
                    required property var modelData
                    readonly property bool current: root.namesNet === modelData.id
                    Layout.fillWidth: true
                    implicitHeight: netChoiceText.implicitHeight + root.px(18)
                    radius: 8
                    color: current ? root.raised : netChoiceArea.containsMouse ? root.hover : "transparent"
                    border.width: 1
                    border.color: current ? root.ink : root.line
                    opacity: root.netSwitching && !current ? 0.5 : 1
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.version + ", " + modelData.name
                    Accessible.checked: current
                    Column {
                        id: netChoiceText
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.left: parent.left
                        anchors.leftMargin: root.px(14)
                        spacing: 1
                        Text { text: netChoice.modelData.version; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: netChoice.current ? Font.DemiBold : Font.Normal }
                        Text { text: netChoice.modelData.name; color: root.dim; font.family: root.sans; font.pixelSize: root.px(13) }
                    }
                    MouseArea {
                        id: netChoiceArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.setWalletNet(netChoice.modelData.id)
                    }
                }
            }
        }
        Text {
            visible: root.netProblem !== "" || root.netSwitching
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            color: root.netProblem !== "" ? root.alarm : root.dim
            font.family: root.sans
            font.pixelSize: root.px(14)
            text: root.netProblem !== "" ? root.netProblem : "Switching…"
        }
    }

    /// The mark at the start of a status line, drawn (no font has all four): working three dots,
    /// waiting a clock, problem a cross in a ring, done a tick; `caution` an "!" (STYLE.md).
    component StatusMark: Canvas {
        id: statusMark
        property string kind: "working"
        readonly property color tint: kind === "caution" ? root.cautionTone : root.kindColor(kind)
        implicitWidth: root.px(16)
        implicitHeight: root.px(16)
        onKindChanged: requestPaint()
        onTintChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            var w = width, h = height, c = w / 2
            ctx.strokeStyle = tint
            ctx.fillStyle = tint
            ctx.lineWidth = Math.max(1.5, w / 10)
            ctx.lineCap = "round"
            ctx.lineJoin = "round"
            if (kind === "working") {
                for (var i = 0; i < 3; i++) {
                    ctx.beginPath()
                    ctx.arc(w * (0.2 + i * 0.3), c, w / 11, 0, Math.PI * 2)
                    ctx.fill()
                }
                return
            }
            if (kind === "done") {
                ctx.beginPath()
                ctx.moveTo(w * 0.14, h * 0.55)
                ctx.lineTo(w * 0.40, h * 0.80)
                ctx.lineTo(w * 0.88, h * 0.22)
                ctx.stroke()
                return
            }
            ctx.beginPath()
            ctx.arc(c, c, c - ctx.lineWidth / 2, 0, Math.PI * 2)
            ctx.stroke()
            ctx.beginPath()
            if (kind === "waiting") {
                ctx.moveTo(c, h * 0.24)
                ctx.lineTo(c, c)
                ctx.lineTo(w * 0.72, h * 0.62)
                ctx.stroke()
            } else if (kind === "problem") {
                ctx.moveTo(w * 0.34, h * 0.34)
                ctx.lineTo(w * 0.66, h * 0.66)
                ctx.moveTo(w * 0.66, h * 0.34)
                ctx.lineTo(w * 0.34, h * 0.66)
                ctx.stroke()
            } else {
                ctx.moveTo(c, h * 0.26)
                ctx.lineTo(c, h * 0.58)
                ctx.stroke()
                ctx.beginPath()
                ctx.arc(c, h * 0.74, w / 14, 0, Math.PI * 2)
                ctx.fill()
            }
        }
    }

    /// A status line: one of four kinds (Logic.STATUS_KINDS), one colour, one mark, one size,
    /// directly under the control or step it belongs to. `kind` "" draws a plain line (no mark,
    /// no colour) for a note that is none of the four. A problem offers its one next action as a
    /// button (`actionLabel`, signal `act`); a wait may too (Check again).
    component StatusLine: ColumnLayout {
        id: statusLine
        property string kind: "working"
        property string text: ""
        property string actionLabel: ""
        property bool actionStrong: false
        property bool actionEnabled: true
        signal act()
        Layout.fillWidth: true
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            StatusMark {
                visible: statusLine.kind !== ""
                kind: statusLine.kind
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: root.px(3)
                Layout.preferredWidth: root.px(16)
                Layout.preferredHeight: root.px(16)
            }
            Text {
                Layout.fillWidth: true
                text: statusLine.text
                textFormat: Text.PlainText
                color: statusLine.kind === "" ? root.ink : statusLine.kind === "working" ? root.dim : root.kindColor(statusLine.kind)
                font.family: root.sans
                font.weight: statusLine.kind === "" || statusLine.kind === "working" ? Font.Medium : Font.DemiBold
                font.pixelSize: root.statusPx()
                lineHeight: 1.25
                wrapMode: Text.Wrap
            }
        }
        Action {
            visible: statusLine.actionLabel !== ""
            Layout.leftMargin: statusLine.kind !== "" ? root.px(24) : 0
            label: statusLine.actionLabel
            strong: statusLine.actionStrong
            enabledNow: statusLine.actionEnabled
            onPressed: statusLine.act()
        }
    }

    /// The line under a disabled button that says why it can't be pressed (STYLE.md: a disabled
    /// button always has its reason). Hidden when there is none.
    component WhyLine: Text {
        Layout.fillWidth: true
        visible: text !== ""
        textFormat: Text.PlainText
        color: root.dim
        font.family: root.sans
        font.pixelSize: root.px(15)
        wrapMode: Text.Wrap
    }

    /// The way back from a page: a real button, the same place and size on every page.
    component BackButton: Rectangle {
        id: back
        property string label: "Wallet"
        signal pressed()
        implicitWidth: backText.implicitWidth + root.px(28)
        implicitHeight: root.px(36)
        radius: 6
        color: backArea.pressed ? root.hover : backArea.containsMouse ? root.raised : "transparent"
        border.width: 1
        border.color: activeFocus ? root.brass : root.line
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Back to " + label
        Keys.onReturnPressed: back.pressed()
        Keys.onSpacePressed: back.pressed()
        Text {
            id: backText
            anchors.centerIn: parent
            text: "‹  " + back.label
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(17)
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

    /// Every page below the wallet starts with this: the back button, the title (one size) and,
    /// when given, the network the page runs on.
    component PageHeader: ColumnLayout {
        id: pageHeader
        property string title: ""
        property string backLabel: "Wallet"
        property string net: ""
        signal back()
        Layout.fillWidth: true
        spacing: 10
        BackButton { label: pageHeader.backLabel; onPressed: pageHeader.back() }
        Text {
            Layout.topMargin: 4
            Layout.fillWidth: true
            text: pageHeader.title
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(28)
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
        RunsOn { visible: pageHeader.net !== ""; net: pageHeader.net !== "" ? pageHeader.net : root.namesNet }
    }

    /// The step row and the current step's title, the same on Claim, Receive and Send back.
    /// Done pills ticked, the current one outlined, later ones dim; the title says the current
    /// step (never with a tick unless the whole flow is done).
    component StepHeader: ColumnLayout {
        id: stepHeader
        property var steps: []
        readonly property var current: {
            var now = stepHeader.steps.filter(function(x) { return x.state === "now" })[0]
            return now !== undefined ? now : stepHeader.steps.length > 0 ? stepHeader.steps[stepHeader.steps.length - 1] : null
        }
        readonly property bool finished: current !== null && current.state === "done"
        Layout.fillWidth: true
        spacing: 12
        StepRow { steps: stepHeader.steps }
        Text {
            visible: stepHeader.current !== null
            Layout.fillWidth: true
            text: stepHeader.current === null ? "" : "Step " + stepHeader.current.n + " of " + stepHeader.current.total + " · " + stepHeader.current.title
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(22)
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
    }

    /// One choice among a few (a token, an amount, an account): a chip, outlined in brass when
    /// chosen. Never a filled button: the screen's one primary is the action after the choice.
    component Choice: Rectangle {
        id: choice
        property string label: ""
        property bool chosen: false
        signal pressed()
        implicitWidth: choiceText.implicitWidth + root.px(30)
        implicitHeight: root.px(38)
        radius: 6
        color: chosen ? root.raised : choiceArea.containsMouse ? root.hover : "transparent"
        border.width: chosen ? 2 : 1
        border.color: chosen ? root.brass : activeFocus ? root.brass : root.line
        activeFocusOnTab: true
        Accessible.role: Accessible.RadioButton
        Accessible.name: label
        Accessible.checked: chosen
        Keys.onReturnPressed: choice.pressed()
        Keys.onSpacePressed: choice.pressed()
        Text {
            id: choiceText
            anchors.centerIn: parent
            text: (choice.chosen ? "● " : "") + choice.label
            color: root.ink
            font.family: root.sans
            font.pixelSize: root.px(17)
            font.weight: choice.chosen ? Font.DemiBold : Font.Medium
        }
        MouseArea {
            id: choiceArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: choice.pressed()
        }
    }

    /// A consent tick box: an ordinary box with plain words. `caution` (the amber border and an
    /// "!") only when the wallet truly cannot decode what is approved. Never red.
    component Tick: RowLayout {
        id: tick
        property bool checked: false
        property string text: ""
        property bool caution: false
        signal toggled()
        Layout.fillWidth: true
        spacing: 10
        Rectangle {
            Layout.preferredWidth: root.px(20)
            Layout.preferredHeight: root.px(20)
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: root.px(1)
            radius: 4
            color: tick.checked ? (tick.caution ? root.cautionTone : root.brass) : "transparent"
            border.width: 2
            border.color: tick.caution ? root.cautionTone : tick.checked ? root.brass : root.dim
            Text { anchors.centerIn: parent; visible: tick.checked; text: "✓"; color: root.slate; font.family: root.sans; font.pixelSize: root.px(14); font.weight: Font.Bold }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tick.toggled() }
        }
        StatusMark { visible: tick.caution; kind: "caution"; Layout.alignment: Qt.AlignTop; Layout.topMargin: root.px(3); Layout.preferredWidth: root.px(16); Layout.preferredHeight: root.px(16) }
        Text {
            Layout.fillWidth: true
            text: tick.text
            textFormat: Text.PlainText
            color: root.ink
            font.family: root.sans
            font.weight: Font.Medium
            font.pixelSize: root.px(17)
            lineHeight: 1.25
            wrapMode: Text.Wrap
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tick.toggled() }
        }
    }

    component Hint: Text {
        Layout.fillWidth: true
        color: root.dim
        font.family: root.sans
        font.weight: Font.Medium
        font.pixelSize: root.px(17)
        lineHeight: 1.25
        wrapMode: Text.Wrap
    }

    /// The heading of one part of the edit panel.
    component Part: Text {
        Layout.topMargin: 10
        color: root.ink
        font.family: root.sans
        font.pixelSize: root.px(18)
        font.weight: Font.Bold
    }

    /// The wallet's funded accounts, one of which pays. With `withPrivate`
    /// the private accounts follow the public ones and the choice is the
    /// payment's sender rather than a name's holder.
    component Accounts: ColumnLayout {
        id: accountsList
        property bool withPrivate: false
        readonly property int chosen: withPrivate ? root.senderIndex : root.fromIndex
        Layout.fillWidth: true
        spacing: 6
        Repeater {
            model: accountsList.withPrivate ? root.senders() : root.wallet
            Rectangle {
                id: accountRow
                required property var modelData
                required property int index
                Layout.fillWidth: true
                height: 36
                radius: 6
                color: accountsList.chosen === index ? root.raised : "transparent"
                border.width: 1
                border.color: accountsList.chosen === index ? root.brass : root.line
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 12
                    spacing: 10
                    Seal {
                        Layout.preferredWidth: 20
                        Layout.preferredHeight: 20
                        owner: accountRow.modelData.account
                    }
                    Text {
                        text: root.shorten(accountRow.modelData.account)
                        color: root.ink
                        font.family: root.mono
                        font.pixelSize: root.px(17)
                    }
                    Text {
                        visible: !!accountRow.modelData.isPrivate
                        text: "Private"
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(16)
                    }
                    Text {
                        visible: !!accountRow.modelData.name
                        text: accountRow.modelData.name ? accountRow.modelData.name : ""
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(17)
                    }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: root.grouped(accountRow.modelData.balance)
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(17)
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (accountsList.withPrivate)
                            root.senderIndex = accountRow.index
                        else
                            root.fromIndex = accountRow.index
                    }
                }
            }
        }
    }

    /// One labelled fact on the plate, with a button to copy the full value.
    component Fact: RowLayout {
        id: fact
        property string label: ""
        property string value: ""
        property string full: ""
        property bool code: false
        Layout.fillWidth: true
        spacing: 12
        Text {
            Layout.preferredWidth: 120
            text: fact.label
            color: root.dim
            font.family: root.sans
            font.weight: Font.Medium
            font.pixelSize: root.px(18)
        }
        Text {
            Layout.fillWidth: true
            text: fact.value
            textFormat: Text.PlainText
            color: root.ink
            font.family: fact.code ? root.mono : root.sans
            font.weight: Font.Medium
            font.pixelSize: fact.code ? 17 : 18
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            maximumLineCount: 3
            elide: Text.ElideRight
        }
        Link {
            visible: fact.full !== ""
            text: "Copy"
            onPressed: root.copy(fact.full, fact.label)
        }
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

    // ── Layout ──────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Who you are, and where to go.
        Rectangle {
            Layout.preferredWidth: 260
            Layout.fillHeight: true
            color: root.panel

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 18
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    Layout.bottomMargin: 14
                    spacing: 12
                    Avatar {
                        Layout.preferredWidth: 48
                        Layout.preferredHeight: 48
                        visible: root.mainName !== ""
                        pixels: root.boardPeople[root.mainName] ? root.boardPeople[root.mainName].pixels : null
                        owner: root.mainName
                        letter: root.mainName.charAt(0)
                        ring: root.boardPeople[root.mainName] && root.boardPeople[root.mainName].color !== "" ? root.boardPeople[root.mainName].color : root.brass
                        Component.onCompleted: if (root.mainName !== "") root.boardLookUp(root.mainName)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Text {
                            Layout.fillWidth: true
                            text: root.mainName !== "" ? root.mainName : root.needsWelcome ? "Welcome" : "No name yet"
                            textFormat: Text.PlainText
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(19)
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            visible: root.mainName !== ""
                            text: "main name"
                            color: root.faint
                            font.family: root.sans
                            font.pixelSize: root.px(14)
                        }
                    }
                }

                Tab { Layout.fillWidth: true; alignLeft: true; label: "Wallet"; current: root.section === "wallet" && root.walletView !== "names"; onPressed: root.openWallet("home") }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Activity"; current: root.section === "activity"; onPressed: { root.go("activity"); root.loadActivity() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Names"; current: root.section === "wallet" && root.walletView === "names"; onPressed: { root.openWallet("names"); root.loadCliMine() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Messages"; current: root.section === "messages"; onPressed: { root.go("messages"); root.chatLoad() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Profile"; current: root.section === "profile"; onPressed: { root.say("", false); root.openProfile() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Search"; current: root.section === "search"; onPressed: { root.go("search"); searchField.forceActiveFocus() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Settings"; current: root.section === "settings"; onPressed: { root.go("settings"); root.registering = false; root.gettingTokens = false; root.record = null; root.missing = ""; root.loadAccounts() } }

                Item { Layout.fillHeight: true }

                RowLayout {
                    visible: root.sequencer !== ""
                    spacing: 8
                    Rectangle { width: 8; height: 8; radius: 4; color: root.zoneHome ? root.good : root.brass }
                    Text {
                        Layout.fillWidth: true
                        text: root.networkName()
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: root.zoneHome ? root.faint : root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(14)
                        MouseArea { anchors.fill: parent; enabled: root.zonesMany; cursorShape: Qt.PointingHandCursor; onClicked: root.go("settings") }
                    }
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }

        // First run: install the CLI, then pick your name.
        Flickable {
            visible: root.welcomeCovers
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: welcome.implicitHeight + 72
            clip: true
            ColumnLayout {
                id: welcome
                x: 56
                y: 48
                width: Math.min(parent.width - 112, 640)
                spacing: 18
                Text { text: "Welcome"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(36); font.weight: Font.DemiBold }
                Hint { font.pixelSize: root.px(18); text: "Get a name, then post, pay and be paid as it. " + (root.namesNet === "0.4" ? "Three" : "Four") + " steps, all here." }
                // Which network these steps are for: switchable here, before anything else, so a
                // wallet-less start on LEZ 0.3 never hides LEZ 0.4.
                NetSwitch {}

                // 1. Install the names program.
                Card {
                    Layout.fillWidth: true
                    StepHead { number: 1; done: root.cliPath !== ""; title: "Install" }
                    Hint {
                        visible: root.cliPath === "" && root.namesNet === "0.3"
                        text: "Names needs a small program that proves your changes on this computer, so the chain never learns your account. About 90 MB, checked against the release signature before it's used."
                    }
                    Hint {
                        visible: root.cliPath === "" && root.namesNet === "0.4"
                        text: Logic.FULL_TOOL_DOWNLOAD ? "LEZ 0.4 needs its own names tool. About 100 MB, checked against its SHA-256 before it runs." : "LEZ 0.4 needs its own names tool. It comes with Names: Install puts it in place."
                    }
                    RowLayout {
                        visible: root.cliPath === ""
                        spacing: 12
                        Action {
                            label: root.installJob !== null ? "Installing…" : "Install"
                            strong: true
                            enabledNow: root.installJob === null
                            onPressed: root.installCli()
                        }
                        Hint {
                            visible: root.installJob !== null
                            Layout.fillWidth: true
                            color: root.waitTone
                            font.pixelSize: root.px(16)
                            text: root.installJob === null ? "" : root.installJob.line
                        }
                    }
                    StatusLine { visible: root.installProblem !== ""; kind: Logic.noticeKind(root.installProblem, true); text: root.installProblem }
                    Hint {
                        visible: root.cliPath !== ""
                        text: "Installed." + (root.cliStatus !== null && root.cliStatus.cli ? " " + root.cliStatus.cli : "")
                        font.pixelSize: root.px(15)
                        wrapMode: Text.WrapAnywhere
                    }
                    StatusLine { visible: root.installWarning !== ""; kind: Logic.noticeKind(root.installWarning, true); text: root.installWarning }
                    Link {
                        visible: root.cliPath !== "" && root.installWarning !== ""
                        text: "Install the prover"
                        font.pixelSize: root.px(15)
                        onPressed: root.installCli()
                    }
                    // For people who already have the CLI, or want it in Terminal too.
                    Link {
                        visible: root.cliPath === "" && root.namesNet === "0.3" && !root.terminalNoteOpen
                        Layout.topMargin: 4
                        text: "Already use the command line?"
                        color: root.dim
                        font.pixelSize: root.px(14)
                        onPressed: root.terminalNoteOpen = true
                    }
                    ColumnLayout {
                        visible: root.cliPath === "" && root.namesNet === "0.3" && root.terminalNoteOpen
                        Layout.fillWidth: true
                        spacing: 6
                        Hint { font.pixelSize: root.px(14); text: "Install it from Terminal instead, then come back:" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Text {
                                Layout.fillWidth: true
                                text: "curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh && names setup"
                                color: root.dim
                                font.family: root.mono
                                font.pixelSize: root.px(13)
                                elide: Text.ElideRight
                            }
                            Link { text: "Copy"; font.pixelSize: root.px(14); onPressed: root.copy("curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh && names setup", "The command") }
                        }
                        Link { text: "I've installed it"; font.pixelSize: root.px(14); onPressed: root.loadConfig() }
                    }
                }

                // 2. Create the wallet.
                Card {
                    Layout.fillWidth: true
                    opacity: root.cliPath !== "" ? 1 : 0.5
                    StepHead { number: 2; done: root.hasCliWallet && !root.oldWallet; title: "Create my wallet" }
                    StatusLine {
                        visible: root.cliOutdated
                        kind: "problem"
                        text: Logic.cliOutdatedLine(root.namesNet, root.cliStatus !== null && root.cliStatus.cli_line ? String(root.cliStatus.cli_line) : "")
                    }
                    StatusLine {
                        visible: root.oldWallet
                        kind: "problem"
                        text: "Your " + (root.namesNet === "0.4" ? "LEZ 0.4" : "LEZ 0.3") + " wallet was made for an earlier version of that network and no longer opens. Enter its recovery phrase to bring its accounts back. The old files are kept. Tokens don't carry over."
                    }
                    // No wallet here yet: making a new one, or the same accounts as a wallet you have
                    // (one recovery phrase gives the same public accounts on LEZ 0.3 and LEZ 0.4).
                    Hint {
                        visible: !root.hasCliWallet && !root.oldWallet && root.cliPath !== ""
                        text: "Restore if you already have a wallet on LEZ 0.3 and want the same accounts here. Create a new one if you don't."
                    }
                    Link {
                        visible: !root.hasCliWallet && !root.oldWallet && root.cliPath !== "" && !root.restoreOpen
                        text: "Restore from my recovery phrase"
                        font.pixelSize: root.px(16)
                        onPressed: root.restoreOpen = true
                    }
                    Link {
                        visible: !root.hasCliWallet && !root.oldWallet && root.cliPath !== ""
                        text: "Restore from a backup file"
                        font.pixelSize: root.px(16)
                        onPressed: root.openBackup("restore")
                    }
                    Field {
                        id: restorePhraseField
                        visible: root.oldWallet || (root.restoreOpen && !root.hasCliWallet)
                        Layout.fillWidth: true
                        secret: true
                        hint: "Your recovery phrase"
                    }
                    Action {
                        visible: root.oldWallet || (root.restoreOpen && !root.hasCliWallet)
                        label: root.restoreBusy ? "Restoring…" : root.oldWallet ? "Bring my wallet back" : "Restore my wallet"
                        strong: true
                        enabledNow: !root.restoreBusy
                        onPressed: { root.restoreWallet(restorePhraseField.text); restorePhraseField.text = "" }
                    }
                    StatusLine { visible: root.restoreError !== ""; kind: Logic.noticeKind(root.restoreError, true); text: root.restoreError }
                    Hint { visible: root.restoreDone !== ""; text: root.restoreDone }
                    Hint {
                        visible: !root.hasCliWallet
                        text: "Your wallet lives on this computer. It has a public account, for getting tokens, and a private one, for your names."
                    }
                    Action {
                        visible: !root.hasCliWallet && !root.oldWallet
                        label: root.walletBusy ? "Creating…" : "Create my wallet"
                        strong: root.cliPath !== ""
                        enabledNow: root.cliPath !== "" && !root.walletBusy
                        onPressed: root.createWallet()
                    }
                    StatusLine { visible: root.walletError !== ""; kind: Logic.noticeKind(root.walletError, true); text: root.walletError }
                    Fact {
                        visible: root.hasCliWallet
                        label: "Public account"
                        code: true
                        value: root.myPublic
                        full: root.myPublic !== "" ? "Public/" + root.myPublic : ""
                    }
                    Fact {
                        visible: root.hasCliWallet && root.cliPublic !== null
                        label: "Holds"
                        value: root.cliPublic !== null ? root.grouped(root.cliPublic.balance) : ""
                    }
                    Hint {
                        visible: root.namesNet === "0.3" && root.hasCliWallet && !root.canShield && !root.hasPrivateFunds
                        font.pixelSize: root.px(16)
                        text: "Get " + root.grouped(root.faucetAmount) + " free test tokens to start: your name's deposit, and the fees to hold it privately."
                    }
                    // LEZ 0.4 has no faucet in the app (cli_faucet refuses there), and the Logos node
                    // bridge below deposits on LEZ 0.3's terms: neither is offered on LEZ 0.4.
                    Hint {
                        visible: root.namesNet === "0.4" && root.hasCliWallet
                        font.pixelSize: root.px(16)
                        text: "LEZ 0.4 has no faucet. Coins come from the network's operator, and bridged Sepolia tokens arrive through the LEZ Bridge app. Your public account above is where to have them sent."
                    }
                    Action {
                        visible: root.namesNet === "0.3" && root.hasCliWallet && !root.canShield && !root.hasPrivateFunds && !root.faucetSent
                        label: root.faucetBusy ? "Asking…" : "Get free tokens"
                        strong: true
                        enabledNow: !root.faucetBusy
                        onPressed: root.getFreeTokens()
                    }
                    Hint {
                        visible: root.faucetSent && !root.canShield
                        font.pixelSize: root.px(16)
                        color: root.waitTone
                        text: "Sent. They arrive in a minute or two; this checks by itself."
                    }
                    StatusLine { visible: root.faucetError !== ""; kind: Logic.noticeKind(root.faucetError, true); text: root.faucetError }
                    RowLayout {
                        visible: root.hasCliWallet
                        spacing: 18
                        Link { visible: root.namesNet === "0.3"; text: "Or bridge from my Logos node"; font.pixelSize: root.px(16); onPressed: root.openGetTokens() }
                        Link { text: "Check again"; color: root.dim; font.pixelSize: root.px(16); onPressed: root.loadCliBalance() }
                    }
                }

                // 3. Move tokens to the private account. Not on LEZ 0.4: private payments are off
                // there, and its names are registered from the public account.
                Card {
                    visible: root.namesNet === "0.3"
                    Layout.fillWidth: true
                    opacity: root.hasCliWallet ? 1 : 0.5
                    StepHead { number: 3; done: root.hasPrivateFunds && root.shieldJob === null; title: "Move to private" }
                    Hint {
                        text: "Moves " + (root.canShield ? root.grouped(root.shieldAmount) : "your tokens") + " from your public account to your private one, so your names and payments stay private. Your computer proves it, usually 5–12 minutes."
                    }
                    Hint {
                        visible: root.hasCliWallet && root.cliPublic !== null && !root.canShield && root.shieldJob === null && !root.hasPrivateFunds
                        font.pixelSize: root.px(16)
                        color: root.ink
                        text: root.cliPublic === null ? "" : "Your public account holds " + root.grouped(root.cliPublic.balance) + ". It needs at least " + root.grouped(root.shieldMinimum) + " first: "
                            + (root.cliOutdated ? "a few names' deposits." : "a name's 1,000,000 deposit plus two private transactions' fees, about 5,100,000 each.")
                    }
                    Action {
                        label: root.shieldJob !== null ? "Moving…" : "Move to private"
                        strong: root.canShield
                        enabledNow: root.hasCliWallet && root.canShield && root.shieldJob === null
                        onPressed: root.moveToPrivate()
                    }
                    Hint { visible: root.shieldJob !== null; color: root.waitTone; font.pixelSize: root.px(16); text: root.shieldLine() }
                    StatusLine { visible: root.shieldProblem !== "" && root.shieldJob === null; kind: Logic.noticeKind(root.shieldProblem, true); text: root.shieldProblem }
                    Hint {
                        visible: root.hasPrivateFunds && root.shieldJob === null
                        font.pixelSize: root.px(16)
                        color: root.good
                        text: root.hasPrivateFunds ? "Your private account holds " + root.grouped(root.cliBalance.balance) + "." : ""
                    }
                }

                // 4. Pick a name.
                Card {
                    Layout.fillWidth: true
                    opacity: root.hasCliWallet ? 1 : 0.5
                    StepHead { number: root.namesNet === "0.4" ? 3 : 4; done: false; title: "Pick your name" }
                    Hint {
                        text: root.namesNet === "0.4"
                            ? "On LEZ 0.4 a name is registered from your public account, in the open. It takes a deposit (1,000,000 for most names, more for 3–4 characters), returned when you release it; if you transfer it, the deposit goes with it, and about 20 minutes to register."
                            : "Held privately: nobody can find which account holds it. It takes a deposit (1,000,000 for most names, more for 3–4 characters), returned when you release it; if you transfer it, the deposit goes with it, and about 20 minutes to register."
                    }
                    Action {
                        label: "Choose my name"
                        strong: root.hasPrivateFunds || (root.namesNet === "0.4" && root.cliPublic !== null && root.cliPublic.balance !== "0")
                        enabledNow: root.cliPath !== "" && root.hasCliWallet
                        onPressed: root.openRegister()
                    }
                }
            }
        }

        // Activity: everything the Wallet sent or saw arrive.
        Flickable {
            visible: root.section === "activity" && !root.needsWelcome
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: activityPage.implicitHeight + 72
            clip: true
            ColumnLayout {
                id: activityPage
                x: 32
                y: 32
                width: Math.min(parent.width - 64, 680)
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    Text { text: "Activity"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    Link { text: "Refresh"; color: root.faint; font.pixelSize: root.px(15); onPressed: { root.loadActivity(); root.loadCliBalance(); root.loadChain() } }
                }
                Hint { font.pixelSize: root.px(14); text: "What this app sent on the Logos Blockchain and LEZ, followed until it settles, and money seen arriving. Kept on this computer: neither chain is asked for history." }
                Row {
                    spacing: 8
                    Repeater {
                        model: [{ key: "", label: "All" }, { key: "logos", label: "Logos Blockchain" }, { key: "lez", label: "LEZ" }, { key: "bridge", label: "Bridges" }]
                        Tab {
                            required property var modelData
                            label: modelData.label
                            current: root.activityFilter === modelData.key
                            onPressed: root.activityFilter = modelData.key
                        }
                    }
                }
                Hint {
                    visible: root.activityShown.length === 0
                    font.pixelSize: root.px(15)
                    text: "Nothing yet. What you send, bridge or receive shows here."
                }
                Repeater {
                    model: root.activityShown
                    ActivityRow { required property var modelData; entry: modelData }
                }
            }
        }

        // Messages: name-to-name DMs.
        Item {
            visible: root.section === "messages" && !root.needsWelcome
            Layout.fillWidth: true
            Layout.fillHeight: true
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 32
                spacing: 14
                RowLayout {
                    Layout.fillWidth: true
                    Text { text: "Messages"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: root.chatState === "" ? "" : "Network: " + root.chatState
                        color: root.faint
                        font.family: root.sans
                        font.pixelSize: root.px(14)
                    }
                }
                RunsOn { net: "messages" }
                // Not connected: the Chat app starts the chat module.
                Rectangle {
                    visible: root.chatError !== ""
                    Layout.fillWidth: true
                    implicitHeight: chatDown.implicitHeight + 24
                    radius: root.px(8)
                    color: root.noticeFill
                    RowLayout {
                        id: chatDown
                        x: 14; y: 12
                        width: parent.width - 28
                        Text { Layout.fillWidth: true; text: "Messaging isn't running yet. Open the Chat app once, then come back."; color: root.ink; font.family: root.sans; font.pixelSize: root.px(15); wrapMode: Text.Wrap }
                        Action { label: "Open Chat"; onPressed: if (logos.request) logos.request("basecamp.apps.launch", { app: "chat_ui" }, function() { root.chatLoad() }) }
                    }
                }
                // Let people message your name.
                Rectangle {
                    visible: root.chatError === "" && root.chatMine !== "" && root.mainName !== "" && root.mainChat !== root.chatMine
                    Layout.fillWidth: true
                    implicitHeight: chatOff.implicitHeight + 24
                    radius: 8
                    color: root.card
                    border.width: 1
                    border.color: root.line
                    RowLayout {
                        id: chatOff
                        x: 14; y: 12
                        width: parent.width - 28
                        Text {
                            Layout.fillWidth: true
                            text: "People can't message " + root.mainName + " yet. Turning it on adds this computer's chat address to " + root.mainName + "'s profile, so anyone who looks the name up can start a DM."
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: root.px(15)
                            wrapMode: Text.Wrap
                        }
                        Action { label: "Turn on messages"; strong: true; changes: true; onPressed: root.chatPublish() }
                    }
                }
                Hint {
                    visible: root.chatError === "" && root.chatMine === "" && root.chatState !== ""
                    color: root.waitTone
                    text: "Messaging is starting up (network: " + root.chatState + "). Once it's connected you can turn on messages for " + (root.mainName !== "" ? root.mainName : "your name") + "."
                }
                Hint {
                    visible: root.chatError === "" && root.mainChat !== "" && root.mainChat === root.chatMine
                    font.pixelSize: root.px(14)
                    text: "Messages are on: anyone who looks up " + root.mainName + " can message you here."
                }
                StatusLine {
                    visible: root.notice !== "" && root.section === "messages"
                    kind: root.noticeKind
                    text: root.notice
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 20
                    // Conversations.
                    ColumnLayout {
                        Layout.preferredWidth: 280
                        Layout.fillHeight: true
                        spacing: 10
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Field { id: dmTo; Layout.fillWidth: true; size: 16; hint: "Message a name"; onAccepted: root.chatStart(dmTo.text) }
                            Action { label: "Start"; enabledNow: dmTo.text.trim() !== ""; onPressed: root.chatStart(dmTo.text) }
                        }
                        Link { text: root.chatNewGroup ? "Cancel new group" : "+ New group"; color: root.brass; font.pixelSize: root.px(15); onPressed: root.chatNewGroup = !root.chatNewGroup }
                        ColumnLayout {
                            visible: root.chatNewGroup
                            Layout.fillWidth: true
                            spacing: 6
                            Field { id: groupName; Layout.fillWidth: true; size: 15; hint: "Group name" }
                            Field { id: groupDesc; Layout.fillWidth: true; size: 15; hint: "What it's about (optional)" }
                            Action { label: "Make group"; strong: true; enabledNow: groupName.text.trim() !== ""; onPressed: root.chatMakeGroup() }
                        }
                        Hint { visible: root.chatConvos.length === 0 && root.chatError === ""; font.pixelSize: root.px(14); text: "No conversations yet." }
                        ListView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            spacing: 4
                            model: root.chatConvos
                            delegate: Rectangle {
                                id: convoRow
                                required property var modelData
                                width: ListView.view.width
                                height: 54
                                radius: 8
                                color: root.chatOpenId === modelData.convo_id ? root.raised : convoArea.containsMouse ? root.hover : "transparent"
                                MouseArea { id: convoArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.chatOpen(convoRow.modelData.convo_id) }
                                ColumnLayout {
                                    x: 12; y: 7
                                    width: parent.width - 24
                                    spacing: 2
                                    Text { Layout.fillWidth: true; text: (/group/.test(String(convoRow.modelData.kind)) ? "# " : "") + root.chatLabel(convoRow.modelData.convo_id); color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold; elide: Text.ElideRight }
                                    Text {
                                        Layout.fillWidth: true
                                        text: convoRow.modelData.preview ? convoRow.modelData.preview : convoRow.modelData.message_count + " messages" + (convoRow.modelData.last_activity_ms ? " · " + root.boardAgo(convoRow.modelData.last_activity_ms) : "")
                                        textFormat: Text.PlainText
                                        color: root.faint
                                        font.family: root.sans
                                        font.pixelSize: root.px(13)
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }
                    }
                    Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }
                    // The open conversation.
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 10
                        Hint { visible: root.chatOpenId === ""; text: "Pick a conversation, message a name, or start a group." }
                        // A group: its members, and adding one.
                        ColumnLayout {
                            visible: root.chatOpenId !== "" && root.chatIsGroup(root.chatOpenId)
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                Layout.fillWidth: true
                                text: "# " + root.chatLabel(root.chatOpenId) + "  ·  " + root.chatMembers.length + " member" + (root.chatMembers.length === 1 ? "" : "s")
                                    + (root.chatMembers.length > 0 ? ": " + root.chatMembers.map(function(m) { return root.chatWho(m.address) + (m.pending ? " (invited)" : "") }).join(", ") : "")
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: root.px(14)
                                wrapMode: Text.Wrap
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Field { id: memberField; Layout.fillWidth: true; size: 15; hint: "Add someone by name"; onAccepted: root.chatAddMember() }
                                Action { label: "Add"; enabledNow: memberField.text.trim() !== ""; onPressed: root.chatAddMember() }
                            }
                        }
                        RowLayout {
                            visible: root.chatOpenId !== "" && root.dmNames[root.chatOpenId] === undefined && !root.chatIsGroup(root.chatOpenId)
                            Layout.fillWidth: true
                            spacing: 6
                            Text { text: "Who is this?"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                            Field { id: dmLabel; Layout.fillWidth: true; size: 15; hint: "Their name, if you know it" }
                            Action {
                                label: "Save"
                                enabledNow: dmLabel.text.trim() !== ""
                                onPressed: askLater("set_dm_name", [root.chatOpenId, dmLabel.text], function(value) { root.dmNames = value || {}; dmLabel.text = "" }, function(message) { root.say(message, true) })
                            }
                        }
                        ListView {
                            id: messageList
                            visible: root.chatOpenId !== ""
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            spacing: 8
                            model: root.chatMessages
                            onCountChanged: positionViewAtEnd()
                            delegate: Item {
                                id: bubbleRow
                                required property var modelData
                                width: ListView.view.width
                                height: bubble.height
                                Rectangle {
                                    id: bubble
                                    readonly property bool named: !bubbleRow.modelData.from_self && root.chatIsGroup(root.chatOpenId) && !!bubbleRow.modelData.sender
                                    anchors.right: bubbleRow.modelData.from_self ? parent.right : undefined
                                    width: Math.min(Math.max(bubbleText.implicitWidth, senderText.implicitWidth) + 28, parent.width * 0.75)
                                    height: bubbleText.implicitHeight + 20 + (named ? senderText.implicitHeight + 2 : 0)
                                    radius: 12
                                    color: bubbleRow.modelData.from_self ? root.ownBubble : root.raised
                                    Text {
                                        id: senderText
                                        visible: bubble.named
                                        x: 14; y: 8
                                        text: root.chatWho(bubbleRow.modelData.sender)
                                        color: root.brass
                                        font.family: root.sans
                                        font.pixelSize: root.px(13)
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        id: bubbleText
                                        x: 14; y: bubble.named ? senderText.implicitHeight + 10 : 10
                                        width: parent.width - 28
                                        text: bubbleRow.modelData.content
                                        textFormat: Text.PlainText
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: root.px(16)
                                        wrapMode: Text.Wrap
                                    }
                                }
                            }
                        }
                        RowLayout {
                            visible: root.chatOpenId !== ""
                            Layout.fillWidth: true
                            spacing: 8
                            Field { id: dmText; Layout.fillWidth: true; size: 16; hint: "Write a message"; onAccepted: root.chatSend() }
                            Action { label: "Send"; strong: true; enabledNow: dmText.text.trim() !== ""; onPressed: root.chatSend() }
                        }
                        Hint { visible: root.chatOpenId !== ""; font.pixelSize: root.px(13); text: "End-to-end encrypted over Logos messaging. Sent at once, no proof needed." }
                    }
                }
            }
        }

        // Wallet: every balance, then Send, Receive, Stake, Add token and Bridge.
        Flickable {
            visible: root.section === "wallet" && (!root.needsWelcome || root.walletView === "bridge")
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: walletPage.implicitHeight + 72
            clip: true
            ColumnLayout {
                id: walletPage
                x: 56
                y: 40
                width: Math.min(parent.width - 112, 860)
                spacing: 16

                // ── Home: what you hold ──
                ColumnLayout {
                    visible: root.walletView === "home"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 14
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Text { text: "Wallet"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                        Link { text: root.chainReading || root.cliBalance === undefined ? "Reading…" : "Refresh"; color: root.faint; font.pixelSize: root.px(15); onPressed: root.walletRefresh() }
                        // Show or hide every amount, as wallets do for a screen others can see.
                        Rectangle {
                            id: eyeButton
                            implicitWidth: root.px(38)
                            implicitHeight: root.px(38)
                            radius: 8
                            color: eyeArea.containsMouse ? root.hover : "transparent"
                            border.width: 1
                            border.color: root.line
                            Accessible.name: root.walletHidden ? "Show balances" : "Hide balances"
                            Canvas {
                                anchors.centerIn: parent
                                width: root.px(20)
                                height: root.px(20)
                                property bool closed: root.walletHidden
                                onClosedChanged: requestPaint()
                                onPaint: {
                                    var c = getContext("2d")
                                    c.reset()
                                    c.strokeStyle = root.dim
                                    c.lineWidth = 1.8
                                    var w = width, h = height
                                    c.beginPath()
                                    c.moveTo(1, h / 2)
                                    c.quadraticCurveTo(w / 2, -h * 0.15, w - 1, h / 2)
                                    c.quadraticCurveTo(w / 2, h * 1.15, 1, h / 2)
                                    c.stroke()
                                    c.beginPath()
                                    c.arc(w / 2, h / 2, w * 0.16, 0, Math.PI * 2)
                                    c.stroke()
                                    if (closed) {
                                        c.beginPath()
                                        c.moveTo(2, 2)
                                        c.lineTo(w - 2, h - 2)
                                        c.stroke()
                                    }
                                }
                            }
                            MouseArea {
                                id: eyeArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.walletHidden = !root.walletHidden
                            }
                        }
                    }
                    // Which network the balances below are on: the names network switch.
                    NetSwitch {}
                    RunsOn { net: root.walletNet === "0.4" ? "0.4" : "0.3+chain"; prefix: "Balances on" }
                    // Right after a switch (or at start), until the wallet answers: never Welcome or
                    // "Create my wallet" before the module says plainly there is no wallet.
                    Hint {
                        visible: root.cliPath !== "" && root.cliStatus === null
                        color: root.dim
                        text: "Reading your wallet…"
                    }
                    // A configured wallet folder or CLI that isn't reachable (a drive not
                    // connected): said plainly; no other wallet is used meanwhile (SF-B).
                    StatusLine {
                        visible: root.cliStatus !== null && typeof root.cliStatus.unavailable === "string"
                        kind: "problem"
                        text: root.cliStatus !== null && root.cliStatus.unavailable ? root.sentence(root.cliStatus.unavailable) : ""
                    }
                    // A wallet with no name yet: a name is offered, never required (the bridge needs none).
                    Card {
                        visible: root.cliKnown && root.allMine.length === 0 && root.cliPending.length === 0 && !root.nameCardAway && root.zoneHome
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "No name yet" }
                        Hint { text: "A name lets people pay and message you by it. You don't need one to hold, send or bridge tokens." }
                        RowLayout {
                            spacing: 14
                            Action { label: "Pick a name"; strong: true; onPressed: root.openWallet("names") }
                            Link { text: "Not now"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.nameCardAway = true }
                        }
                    }

                    // ── LEZ 0.4, the bridge test network: balances, and claiming a deposit ──
                    Card {
                        visible: root.walletNet === "0.4"
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Text { Layout.fillWidth: true; text: "Bridge test network"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.DemiBold }
                            Text { text: "LEZ 0.4"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
                        }
                        Hint { font.pixelSize: root.px(15); text: "Our own chain for the bridge, where bridged Sepolia tokens arrive. It is reset now and then." }
                        // The card's one yellow button: Receive, unless a receive has arrived (its
                        // Claim is then the one) or the bridge setup blocks everything.
                        Flow {
                            Layout.fillWidth: true
                            spacing: 10
                            Action { label: "Receive from Ethereum"; strong: !root.cardSetupBlocks && !arrivingPanel.anyClaim; onPressed: root.openEthReceive() }
                            Action { label: "Claim a bridge deposit"; onPressed: root.openClaim() }
                            Action { label: "Bridge to Ethereum"; onPressed: root.openSendBack("", null) }
                        }
                        // Arriving from Ethereum: this wallet's receives in their own panel, in the
                        // third tone; amount large, the account short, the state under it; Claim
                        // in the panel once it has arrived. A row opens Receive at its step.
                        UnderwayPanel {
                            id: arrivingPanel
                            readonly property var view: root.receiveHasStatus ? Logic.receiveRowsShown(root.receives, root.receiveStatuses, root.receiveClaimedAt, root.receiveNow, root.receiveShowAll)
                                : { rows: root.receives.map(function(x) { return { id: x.id, symbol: x.symbol, amount: x.amount, account: x.account, progress: { stage: "old", card: "The LEZ Bridge here can't follow it: update it.", tone: "plain", claim: false } } }), hidden: 0 }
                            readonly property bool anyClaim: view.rows.some(function(x) { return x.progress.claim })
                            visible: view.rows.length > 0 || view.hidden > 0
                            Text { text: "Arriving from Ethereum"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold; font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6 }
                            Repeater {
                                model: arrivingPanel.view.rows
                                ColumnLayout {
                                    id: arrivingRow
                                    required property var modelData
                                    required property int index
                                    readonly property var p: modelData.progress
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Rectangle { visible: arrivingRow.index > 0; Layout.fillWidth: true; Layout.bottomMargin: 8; implicitHeight: 1; color: root.line }
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 14
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 2
                                            RowLayout {
                                                spacing: 10
                                                Text { text: arrivingRow.modelData.amount + " " + arrivingRow.modelData.symbol; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.Bold }
                                                Text { Layout.alignment: Qt.AlignBaseline; text: "into " + Logic.idShort(arrivingRow.modelData.account) + (arrivingRow.modelData.where ? " · " + arrivingRow.modelData.where : ""); color: root.dim; font.family: root.sans; font.pixelSize: root.px(15) }
                                            }
                                            StatusLine {
                                                kind: Logic.toneKind(arrivingRow.p.tone)
                                                text: arrivingRow.p.card
                                            }
                                            Hint {
                                                visible: arrivingRow.p.stage === "claimed"
                                                font.pixelSize: root.px(15)
                                                text: arrivingRow.p.stage === "claimed" ? Logic.receiveBalanceLine(root.bridgeNetBlocks, arrivingRow.modelData.account, arrivingRow.modelData.symbol) : ""
                                            }
                                        }
                                        Action { visible: arrivingRow.p.claim && root.bridgeSetupClear; label: "Claim"; strong: true; onPressed: root.claimReceive(arrivingRow.modelData.id) }
                                        Link { visible: !arrivingRow.p.claim && arrivingRow.p.stage !== "old"; text: "Open ›"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.openEthReceive(arrivingRow.modelData.id) }
                                    }
                                }
                            }
                            Link {
                                visible: arrivingPanel.view.hidden > 0 || root.receiveShowAll
                                text: root.receiveShowAll ? "Show fewer" : "Show " + arrivingPanel.view.hidden + (arrivingPanel.view.hidden === 1 ? " more code" : " more codes") + " not deposited"
                                color: root.dim
                                font.pixelSize: root.px(15)
                                onPressed: root.receiveShowAll = !root.receiveShowAll
                            }
                            ReceiveCheckLine { visible: root.receiveHasStatus }
                        }
                        // Bridging to Ethereum: this wallet's send-backs, in the same tone; the wait
                        // large. A row opens Send back at its stage.
                        UnderwayPanel {
                            visible: root.sbRowsNow.length > 0
                            Text { text: "Bridging to Ethereum"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold; font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6 }
                            Repeater {
                                model: root.sbRowsNow
                                ColumnLayout {
                                    id: backRow
                                    required property var modelData
                                    required property int index
                                    readonly property var p: modelData.progress
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Rectangle { visible: backRow.index > 0; Layout.fillWidth: true; Layout.bottomMargin: 8; implicitHeight: 1; color: root.line }
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 14
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 2
                                            RowLayout {
                                                spacing: 10
                                                Text { text: backRow.modelData.amount; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.Bold }
                                                Text { Layout.alignment: Qt.AlignBaseline; text: "from " + Logic.idShort(backRow.modelData.account) + " to " + Logic.idShort(backRow.modelData.recipient); color: root.dim; font.family: root.sans; font.pixelSize: root.px(15) }
                                            }
                                            StatusLine {
                                                kind: Logic.toneKind(backRow.p.tone)
                                                text: backRow.p.card
                                            }
                                        }
                                        Link { text: "Open ›"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.openSendBack("", null, backRow.modelData.id) }
                                    }
                                }
                            }
                            SendBackCheckLine { visible: root.sbRowsNow.some(function(x) { return x.progress.stage !== "released" }) }
                        }
                        BridgeSetupBox { visible: root.walletView === "home" && root.bridgeSetup.state !== ""; primary: root.cardSetupBlocks }
                        StatusLine {
                            visible: root.bridgeNetStatusNow.tone === "bad" || root.bridgeNetStatusNow.tone === "warn"
                            kind: root.bridgeNetStatusNow.tone === "bad" ? "problem" : "waiting"
                            text: root.bridgeNetProblem !== "" ? root.bridgeNetProblem : root.bridgeNetStatusNow.line
                        }
                        Hint {
                            visible: !(root.bridgeNetStatusNow.tone === "bad" || root.bridgeNetStatusNow.tone === "warn")
                            font.pixelSize: root.px(15)
                            text: root.bridgeNetProblem !== "" ? root.bridgeNetProblem : root.bridgeNetStatusNow.line
                        }
                        Hint { visible: root.bridgeNetStatusNow.reset !== ""; font.pixelSize: root.px(15); color: root.ink; text: root.bridgeNetStatusNow.reset }
                        Hint {
                            visible: root.bridgeNet !== null && root.bridgeNet.read && root.bridgeNet.read.token_problem
                            font.pixelSize: root.px(15)
                            color: root.ink
                            text: root.bridgeNet !== null && root.bridgeNet.read && root.bridgeNet.read.token_problem ? "Can't read the bridge's token list, so bridged tokens aren't shown." : ""
                        }
                        Hint {
                            visible: root.bridgeNetAccounts.length === 0
                            font.pixelSize: root.px(15)
                            text: root.cliBalance === undefined ? "Reading your accounts…" : "Your wallet has no public account to look up here."
                        }
                        // One block per public account: native LEZ, then each bridged token.
                        Repeater {
                            model: root.bridgeNetView.shown
                            ColumnLayout {
                                id: netAccount
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 6
                                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: root.line }
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.topMargin: 4
                                    spacing: 14
                                    Text { Layout.fillWidth: true; text: netAccount.modelData.title; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); font.weight: Font.DemiBold }
                                    Link { visible: netAccount.modelData.watched; text: "Stop watching"; color: root.dim; font.pixelSize: root.px(14); onPressed: root.bridgeNetWatch(netAccount.modelData.account, false) }
                                    // A LEZ 0.3 account the 0.4 wallet can't sign for has no Copy.
                                    Link { visible: netAccount.modelData.copyable !== false; text: "Copy"; color: root.dim; font.pixelSize: root.px(14); onPressed: root.copy(netAccount.modelData.account, "The account ID") }
                                }
                                Hint { visible: !netAccount.modelData.found; font.pixelSize: root.px(15); text: "Nothing on this network." }
                                Repeater {
                                    model: netAccount.modelData.assets
                                    RowLayout {
                                        id: netAsset
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: 12
                                        ColumnLayout {
                                            spacing: 0
                                            Text { text: netAsset.modelData.symbol; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold }
                                            Text { text: netAsset.modelData.name; textFormat: Text.PlainText; color: root.dim; font.family: root.sans; font.pixelSize: root.px(13) }
                                        }
                                        Item { Layout.fillWidth: true }
                                        Text {
                                            text: root.walletHidden ? "••••••" : netAsset.modelData.amount
                                            color: root.ink
                                            font.family: root.sans
                                            font.pixelSize: root.px(18)
                                            font.weight: Font.DemiBold
                                            horizontalAlignment: Text.AlignRight
                                        }
                                    }
                                }
                                Hint { visible: netAccount.modelData.other > 0; font.pixelSize: root.px(14); text: "Also holds " + netAccount.modelData.other + (netAccount.modelData.other === 1 ? " other token" : " other tokens") + " that the bridge didn't make. Not shown." }
                            }
                        }
                        // The wallet's accounts this network has never seen, in one line.
                        RowLayout {
                            visible: root.bridgeNetView.empty > 0 || root.bridgeNetShowEmpty
                            Layout.fillWidth: true
                            spacing: 12
                            Hint {
                                font.pixelSize: root.px(15)
                                text: root.bridgeNetShowEmpty ? "" : "Nothing on this network for " + (root.bridgeNetView.empty === 1 ? "1 other public account." : root.bridgeNetView.empty + " other public accounts.")
                            }
                            Link { text: root.bridgeNetShowEmpty ? "Hide empty accounts" : "Show them"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.bridgeNetShowEmpty = !root.bridgeNetShowEmpty }
                        }
                        Hint {
                            visible: root.bridgeNet !== null && root.bridgeNet.not_read > 0
                            font.pixelSize: root.px(14)
                            text: root.bridgeNet !== null && root.bridgeNet.not_read > 0 ? root.bridgeNet.not_read + (root.bridgeNet.not_read === 1 ? " more account wasn't" : " more accounts weren't") + " read: at most 24 are read here." : ""
                        }
                        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: root.line }
                        // Any public account can be watched here, the wallet's or not.
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Field { id: bridgeNetWatchField; Layout.fillWidth: true; size: 15; code: true; hint: "Watch a public account: paste its ID"; onAccepted: root.bridgeNetWatch(bridgeNetWatchField.text, true) }
                            Action { label: "Watch"; onPressed: root.bridgeNetWatch(bridgeNetWatchField.text, true) }
                        }
                        StatusLine { visible: root.bridgeNetWatchProblem !== ""; kind: Logic.noticeKind(root.bridgeNetWatchProblem, true); text: root.bridgeNetWatchProblem }
                        Hint { font.pixelSize: root.px(14); text: "Private balances on the bridge test network are not shown yet." }
                        Hint { font.pixelSize: root.px(14); text: "Sending on the bridge test network isn't in the wallet yet, apart from paying a name and bridging to Ethereum." }
                    }
                    // What runs on LEZ 0.3 only, unavailable while LEZ 0.4 is shown.
                    ColumnLayout {
                        visible: root.walletNet === "0.4"
                        Layout.fillWidth: true
                        spacing: 8
                        RowLayout {
                            spacing: 10
                            Action { label: "LNS names"; onPressed: root.walletView = "names" }
                            Action { label: "Send"; enabledNow: false }
                            Action { label: "Receive"; enabledNow: false }
                            Action { label: "Add a token"; enabledNow: false }
                        }
                        RunsOn { net: "0.4"; prefix: "LNS names run on" }
                        Hint { font.pixelSize: root.px(15); text: "Send, Receive and Add a token run on LEZ 0.3 only, the public Logos testnet. Switch to LEZ 0.3 to use them. A name can be paid on LEZ 0.4 from its page (Pay)." }
                    }

                    // Which zone the LEZ balances below are on, once there is more than one.
                    RowLayout {
                        visible: root.walletNet === "0.3" && root.zonesMany
                        Layout.fillWidth: true
                        spacing: 10
                        Hint {
                            textFormat: Text.PlainText
                            color: root.zoneHome ? root.dim : root.ink
                            font.pixelSize: root.px(15)
                            text: "Zone in use: " + root.zoneName + (root.zoneHome ? "" : ". LEZ balances and sends here are on it; your names stay on " + root.homeName + ".")
                        }
                        Link { text: "Zones"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.go("settings") }
                    }
                    Card {
                        visible: root.walletNet === "0.3" && !root.zoneHome && root.cliStatus !== null && !root.cliStatus.wallet
                        Layout.fillWidth: true
                        Hint { textFormat: Text.PlainText; color: root.ink; text: root.zoneName + " has no wallet yet. A zone's wallet is its own: new accounts, and a recovery phrase of its own to write down." }
                        Action { label: root.walletBusy ? "Making it…" : "Make this zone's wallet"; strong: true; enabledNow: !root.walletBusy && root.cliPath !== ""; onPressed: root.createWallet() }
                        StatusLine { visible: root.walletError !== ""; kind: Logic.noticeKind(root.walletError, true); text: root.walletError }
                    }
                    StatusLine { visible: root.sendingLine() !== ""; kind: "waiting"; text: root.sendingLine() }
                    StatusLine { visible: root.notice !== "" && root.sendingLine() === ""; kind: root.noticeKind; text: root.notice }
                    // Payments to your names' private keys wait until you claim them (SC-1).
                    Card {
                        visible: root.walletNet === "0.3" && (root.waitingPayments !== null || root.receiveJob !== null || root.receiveProblem !== "")
                        Layout.fillWidth: true
                        Hint {
                            visible: root.waitingPayments !== null
                            color: root.ink
                            text: root.waitingPayments === null ? "" : (root.waitingPayments.count === "1" ? "A payment is" : root.waitingPayments.count + " payments are")
                                + " to your names waiting: " + root.amountShown(root.waitingPayments.total) + ". Claiming takes about 9 minutes each and reserves up to 8,239,751 from your fullest private account; about 3,169,135 of that comes back later as a small credit."
                        }
                        Action {
                            visible: root.waitingPayments !== null
                            label: root.receiveJob !== null ? "Claiming…" : "Claim"
                            strong: true
                            enabledNow: root.receiveJob === null
                            onPressed: root.claimPayments()
                        }
                        Hint { visible: root.receiveJob !== null; color: root.waitTone; font.pixelSize: root.px(15); text: "Proving the claim on this computer. Several minutes for each payment." }
                        StatusLine { visible: root.receiveProblem !== "" && root.receiveJob === null; kind: Logic.noticeKind(root.receiveProblem, true); text: root.receiveProblem }
                    }
                    // A read the sequencer only partly answered, or why it failed: never "no balances".
                    Hint { visible: root.walletNet === "0.3" && root.balanceNote !== "" && root.cliBalance !== undefined; color: root.ink; font.pixelSize: root.px(15); text: root.balanceNote }
                    Hint {
                        visible: root.walletNet === "0.3" && root.walletGroups.length === 0 && (root.balanceNote === "" || root.cliBalance === undefined)
                        text: root.cliBalance === undefined ? "Reading your balances…" : "No balances yet. Make a wallet in Settings, or start your node in Basecamp's Blockchain app."
                    }

                    // One card per chain: its total, then (open) one row per address.
                    Repeater {
                        model: root.walletNet === "0.3" ? root.walletGroups : []
                        Rectangle {
                            id: groupCard
                            required property var modelData
                            readonly property bool open: root.walletOpen[modelData.id] !== false
                            Layout.fillWidth: true
                            implicitHeight: groupColumn.implicitHeight
                            radius: 12
                            color: root.card
                            border.width: 1
                            border.color: root.line
                            clip: true
                            ColumnLayout {
                                id: groupColumn
                                width: parent.width
                                spacing: 0
                                readonly property int hiddenRows: groupCard.modelData.rows.length - root.groupRows(groupCard.modelData).length
                                // The chain's line: press it to open or close.
                                Rectangle {
                                    Layout.fillWidth: true
                                    implicitHeight: groupHead.implicitHeight + root.px(32)
                                    color: groupArea.containsMouse ? root.hover : "transparent"
                                    radius: 12
                                    RowLayout {
                                        id: groupHead
                                        anchors.left: parent.left
                                        anchors.right: parent.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.leftMargin: root.px(18)
                                        anchors.rightMargin: root.px(18)
                                        spacing: root.px(14)
                                        Rectangle {
                                            implicitWidth: root.px(42)
                                            implicitHeight: root.px(42)
                                            radius: height / 2
                                            color: groupCard.modelData.id === "chain" ? root.orange : groupCard.modelData.id === "lez" ? root.brass : root.raised
                                            Text { anchors.centerIn: parent; text: groupCard.modelData.letter; color: groupCard.modelData.id === "tokens" ? root.ink : root.slate; font.family: root.sans; font.pixelSize: root.px(18); font.weight: Font.Bold }
                                        }
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 2
                                            Text { text: groupCard.modelData.title; color: root.ink; font.family: root.sans; font.pixelSize: root.px(18); font.weight: Font.DemiBold }
                                            Text {
                                                Layout.fillWidth: true
                                                elide: Text.ElideRight
                                                text: groupCard.modelData.note
                                                color: groupCard.modelData.tone === "alarm" ? root.alarm : groupCard.modelData.tone === "stake" ? root.orange : root.dim
                                                font.family: root.sans
                                                font.pixelSize: root.px(14)
                                            }
                                        }
                                        Text {
                                            // Different tokens don't add up to one total.
                                            visible: groupCard.modelData.id !== "tokens"
                                            text: root.amountShown(groupCard.modelData.total)
                                            color: root.ink
                                            font.family: root.sans
                                            font.pixelSize: root.px(24)
                                            font.weight: Font.Bold
                                        }
                                        // Press the card to open or close it.
                                        Text {
                                            text: "›"
                                            rotation: groupCard.open ? 90 : 0
                                            color: root.ink
                                            font.family: root.sans
                                            font.pixelSize: root.px(30)
                                            Layout.preferredWidth: root.px(22)
                                            horizontalAlignment: Text.AlignHCenter
                                            Behavior on rotation { NumberAnimation { duration: 120 } }
                                        }
                                    }
                                    MouseArea {
                                        id: groupArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: root.walletToggle(groupCard.modelData.id)
                                    }
                                }
                                // The node is off: say what to do.
                                RowLayout {
                                    visible: groupCard.modelData.tone === "alarm"
                                    Layout.fillWidth: true
                                    Layout.leftMargin: root.px(74)
                                    Layout.rightMargin: root.px(18)
                                    Layout.bottomMargin: root.px(16)
                                    spacing: 10
                                    Hint { font.pixelSize: root.px(15); text: "Start it in Basecamp's Blockchain app, then Refresh." }
                                    Action { label: "Open the Blockchain app"; onPressed: if (logos.request) logos.request("basecamp.apps.launch", { app: "blockchain_ui" }, function() { root.loadChain() }) }
                                }
                                // Empty accounts, folded away until asked for.
                                Link {
                                    visible: groupCard.open && (groupColumn.hiddenRows > 0 || (root.walletShowEmpty && groupCard.modelData.id === "lez"))
                                    Layout.leftMargin: root.px(76)
                                    Layout.topMargin: root.px(4)
                                    Layout.bottomMargin: root.px(12)
                                    text: root.walletShowEmpty ? "Hide empty accounts" : "Show " + groupColumn.hiddenRows + (groupColumn.hiddenRows === 1 ? " empty account" : " empty accounts")
                                    color: root.dim
                                    font.pixelSize: root.px(15)
                                    onPressed: root.walletShowEmpty = !root.walletShowEmpty
                                }
                                // Its addresses.
                                Repeater {
                                    model: groupCard.open ? root.groupRows(groupCard.modelData) : []
                                    ColumnLayout {
                                        id: addressRow
                                        required property var modelData
                                        readonly property bool open: root.walletRowOpen === modelData.id
                                        Layout.fillWidth: true
                                        spacing: 0
                                        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: root.line }
                                        // Press the row (not its copy button) for its actions.
                                        Rectangle {
                                            Layout.fillWidth: true
                                            implicitHeight: rowLine.implicitHeight + root.px(24)
                                            color: rowArea.containsMouse ? root.hover : "transparent"
                                            MouseArea {
                                                id: rowArea
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: root.openAccount(addressRow.modelData)
                                            }
                                        RowLayout {
                                            id: rowLine
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.verticalCenter: parent.verticalCenter
                                            anchors.leftMargin: root.px(30)
                                            anchors.rightMargin: root.px(18)
                                            spacing: root.px(12)
                                            // A name's own picture when it has one; else its seal, or a letter.
                                            Avatar {
                                                visible: addressRow.modelData.name !== undefined && addressRow.modelData.name !== ""
                                                Layout.preferredWidth: root.px(34)
                                                Layout.preferredHeight: root.px(34)
                                                pixels: visible && root.boardPeople[addressRow.modelData.name] ? root.boardPeople[addressRow.modelData.name].pixels : null
                                                owner: visible ? addressRow.modelData.name : ""
                                                letter: visible ? addressRow.modelData.name.charAt(0) : ""
                                            }
                                            Rectangle {
                                                visible: addressRow.modelData.name === undefined || addressRow.modelData.name === ""
                                                implicitWidth: root.px(34)
                                                implicitHeight: root.px(34)
                                                radius: height / 2
                                                color: root.raised
                                                border.width: 2
                                                border.color: addressRow.modelData.kind === "chain" ? root.orange : addressRow.modelData.fromPrivate ? root.brass : root.line
                                                Text { anchors.centerIn: parent; text: addressRow.modelData.title.charAt(0).toUpperCase(); color: root.ink; font.family: root.sans; font.pixelSize: root.px(14); font.weight: Font.Bold }
                                            }
                                            ColumnLayout {
                                                Layout.fillWidth: true
                                                spacing: 1
                                                Text { text: addressRow.modelData.title; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); font.weight: Font.DemiBold }
                                                Text {
                                                    Layout.fillWidth: true
                                                    elide: Text.ElideRight
                                                    textFormat: Text.StyledText
                                                    text: root.subShown(addressRow.modelData.sub)
                                                    color: root.dim
                                                    font.family: root.sans
                                                    font.pixelSize: root.px(13)
                                                }
                                            }
                                            // Copy lives on the account's page, not on every row.
                                            Rectangle {
                                                visible: false
                                                implicitWidth: root.px(34)
                                                implicitHeight: root.px(34)
                                                radius: 8
                                                color: copyArea.containsMouse ? root.hover : "transparent"
                                                border.width: 1
                                                border.color: root.line
                                                Accessible.name: "Copy the address"
                                                Text { anchors.centerIn: parent; text: "⧉"; color: root.dim; font.pixelSize: root.px(17) }
                                                MouseArea {
                                                    id: copyArea
                                                    anchors.fill: parent
                                                    hoverEnabled: true
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: root.copy(addressRow.modelData.address, addressRow.modelData.title + "'s address")
                                                }
                                            }
                                            Text {
                                                text: root.amountShown(addressRow.modelData.balance)
                                                color: root.ink
                                                font.family: root.sans
                                                font.pixelSize: root.px(18)
                                                font.weight: Font.DemiBold
                                                horizontalAlignment: Text.AlignRight
                                                Layout.minimumWidth: root.px(150)
                                            }
                                            Text { text: "›"; color: root.dim; font.pixelSize: root.px(20); Layout.preferredWidth: root.px(16) }
                                        }
                                        }
                                    }
                                }
                                // The Logos Blockchain: a new account (a key on the node), with a label.
                                ColumnLayout {
                                    visible: groupCard.open && groupCard.modelData.id === "chain" && groupCard.modelData.tone !== "alarm"
                                    Layout.fillWidth: true
                                    Layout.leftMargin: root.px(30)
                                    Layout.rightMargin: root.px(18)
                                    Layout.topMargin: root.px(8)
                                    Layout.bottomMargin: root.px(14)
                                    spacing: 8
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 10
                                        Field { id: chainNewLabel; Layout.fillWidth: true; size: 15; hint: "Label (optional), such as Savings"; onAccepted: root.chainAccountNew(chainNewLabel.text) }
                                        Action {
                                            label: root.chainBusy ? "Making…" : "New account"
                                            enabledNow: !root.chainBusy
                                            onPressed: root.chainAccountNew(chainNewLabel.text)
                                        }
                                    }
                                    StatusLine { visible: root.chainAccountNote !== ""; kind: root.chainAccountNoteIsError ? Logic.noticeKind(root.chainAccountNote, true) : ""; text: root.chainAccountNote }
                                }
                            }
                        }
                    }

                    // Tokens sent to your name's private keys, waiting to be taken in.
                    Rectangle {
                        visible: root.walletNet === "0.3" && root.tokenProgram !== "" && root.tokenPending.length > 0
                        Layout.fillWidth: true
                        implicitHeight: waitingRow.implicitHeight + root.px(24)
                        radius: 12
                        color: root.noticeFill
                        border.width: 1
                        border.color: root.brass
                        RowLayout {
                            id: waitingRow
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.leftMargin: root.px(16)
                            anchors.rightMargin: root.px(16)
                            spacing: 12
                            Hint {
                                color: root.ink
                                font.pixelSize: root.px(16)
                                text: (root.tokenPending.length === 1 ? "1 token payment is" : root.tokenPending.length + " token payments are") + " waiting for you. Taking each in is proved on this computer, 7-12 minutes."
                            }
                            Action { label: "Take it in"; strong: true; enabledNow: root.tokenJob === null; onPressed: root.tokenReceive() }
                        }
                    }

                    RowLayout {
                        visible: root.walletNet === "0.3"
                        Layout.topMargin: 6
                        spacing: 10
                        Action { visible: !root.cliOutdated; label: "Add a token"; onPressed: { root.addTokenProblem = ""; root.walletView = "add"; addTokenField.forceActiveFocus() } }
                        Item { Layout.fillWidth: true }
                    }
                    Hint {
                        visible: root.walletNet === "0.3"
                        font.pixelSize: root.px(14)
                        text: "Private accounts are seen only by you; public ones and the Logos Blockchain by anyone. LEZ and the Logos Blockchain are separate chains with their own addresses; the bridge moves tokens into LEZ."
                    }
                }

                // ── Send ──
                ColumnLayout {
                    visible: root.walletView === "send"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    PageHeader { title: root.sendKind === "chain" ? "Send on the Logos Blockchain" : "Send " + root.sendTitle; net: root.sendKind === "chain" ? "chain" : root.namesNet; onBack: root.walletView = "home" }
                    Hint {
                        visible: root.sendKind !== "chain" && root.namesNet === "0.4"
                        color: root.ink
                        text: "On LEZ 0.4 the wallet pays only a name's public account, from your public account: the chain shows both accounts and the amount. Paying a name's private keys stays off there until real proofs have been run on LEZ 0.4, and other sends aren't in the wallet yet."
                    }

                    Part { text: "From" }
                    PickRow {
                        readonly property var row: root.sendFromRow()
                        title: row ? root.sendRowTitle(row) : "Choose an account"
                        sub: row ? (row.kind === "chain" ? "Logos Blockchain · node key " + Logic.keyShown(row.key) : row.fromPrivate ? "Private" : "Public") : ""
                        amount: row ? root.amountShown(row.balance) : ""
                        dropdown: true
                        open: root.sendFromOpen
                        onPressed: root.sendFromOpen = !root.sendFromOpen
                    }
                    ColumnLayout {
                        visible: root.sendFromOpen
                        Layout.fillWidth: true
                        Layout.leftMargin: root.px(14)
                        spacing: 4
                        Repeater {
                            model: root.walletRows.filter(function(row) { return row.balance !== null && row.balance !== "0" })
                            PickRow {
                                required property var modelData
                                compact: true
                                title: root.sendRowTitle(modelData)
                                sub: modelData.kind === "chain" ? "Logos Blockchain · node key " + Logic.keyShown(modelData.key) : modelData.fromPrivate ? "Private" : "Public"
                                amount: root.amountShown(modelData.balance)
                                current: root.sendRowIsCurrent(modelData)
                                onPressed: { root.sendFromOpen = false; root.openSend(modelData, root.sendToResolved()) }
                            }
                        }
                    }

                    Part { text: "To" }
                    Field {
                        id: sendTo
                        Layout.fillWidth: true
                        size: 19
                        hint: root.sendKind === "chain" ? "A name, or a Logos Blockchain key (64 hex characters)" : "A name, such as ada, or an account address"
                        onTextChanged: root.sendReviewing = false
                    }
                    Hint {
                        visible: root.sendKind !== "chain" && root.sendToResolved() !== sendTo.text.trim()
                        font.pixelSize: root.px(14)
                        text: "That's a LEZ account address: it's paid as the public account " + Logic.keyShown(sendTo.text.trim()) + ", and everyone can see the payment. Make sure it's their public address: money sent this way to a private account's address can't be spent by anyone."
                    }
                    Part { text: "Amount" }
                    Field {
                        id: sendAmount
                        Layout.fillWidth: true
                        size: 19
                        numbersOnly: true
                        longNumbers: true
                        hint: "1000"
                        onTextChanged: root.sendReviewing = false
                    }
                    Hint {
                        visible: root.sendKind !== "chain" && root.sendToName() && !root.sendFromPrivate
                        font.pixelSize: root.px(15)
                        text: "A name is always paid from your private account."
                    }
                    StatusLine {
                        readonly property string problem: root.sendProblem() !== "" ? root.sendProblem() : root.sendKind === "chain" ? root.chainProblem : root.sendKind === "token" ? root.tokenProblem : ""
                        visible: problem !== ""
                        kind: "problem"
                        text: problem
                    }
                    Action {
                        visible: !root.sendReviewing
                        label: root.chainBusy ? "Checking…" : "Review"
                        strong: true
                        enabledNow: sendToResolved() !== "" && sendAmount.text.trim() !== "" && root.sendProblem() === "" && root.sendingLine() === ""
                        onPressed: root.sendReview()
                    }

                    // The summary: what goes where, what it costs, and what the chain sees.
                    Card {
                        visible: root.sendReviewing && (root.sendKind !== "chain" || root.chainPlan !== null)
                        Layout.fillWidth: true
                        Text { text: "Check it, then send"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.DemiBold }
                        Fact {
                            label: "To"
                            value: root.sendKind === "chain" && root.chainPlan !== null
                                ? (root.chainPlan.name ? root.chainPlan.name + " · " : "") + Logic.keyShown(root.chainPlan.to)
                                : sendToResolved()
                        }
                        Fact { visible: root.zonesMany && root.sendKind !== "chain"; label: "Zone"; value: root.zoneName }
                        Fact { label: "Amount"; value: root.grouped(root.sendDigits()) + (root.sendKind === "chain" ? "" : " " + root.sendTitle) }
                        Fact {
                            label: "From"
                            value: root.sendKind === "chain" ? "Your node's key " + Logic.keyShown(root.chainFrom)
                                : root.sendFromPrivate || root.sendToName() ? "Your private account" : "Your public account"
                        }
                        Fact { visible: root.sendFee() !== ""; label: "Fee"; value: root.sendFee() }
                        Hint { font.pixelSize: root.px(16); text: root.sendSeen() }
                        Hint { font.pixelSize: root.px(16); text: root.sendTakes() + " It can't be undone." }
                        RowLayout {
                            spacing: 10
                            Action { label: "Send it"; strong: true; enabledNow: root.sendingLine() === ""; onPressed: root.sendConfirm() }
                            Action { label: "Change something"; onPressed: root.sendReviewing = false }
                        }
                    }
                    StatusLine { visible: root.sendingLine() !== ""; kind: "waiting"; text: root.sendingLine() }
                    Hint {
                        visible: root.sendKind === "chain" && root.chainSent !== null && root.chainSent.state === "in_block"
                        color: root.good
                        font.pixelSize: root.px(16)
                        text: root.chainSent === null ? "" : "Sent " + root.grouped(root.chainSent.amount) + ": it's in a block." + (root.chainSent.fee ? " The fee was " + root.grouped(root.chainSent.fee) + "." : "")
                    }
                    StatusLine { visible: root.notice !== "" && root.sendingLine() === ""; kind: root.noticeKind; text: root.notice }
                }

                // ── Receive ──
                ColumnLayout {
                    visible: root.walletView === "receive"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 14
                    PageHeader { title: root.receiveKind === "chain" ? "Receive on the Logos Blockchain" : "Receive LEZ and tokens"; net: root.receiveKind === "chain" ? "chain" : "0.3"; onBack: root.walletView = "home" }

                    Card {
                        visible: root.receiveKind !== "chain"
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: root.cliNames.length > 1 ? "Your names" : "Your name" }
                        Repeater {
                            model: root.cliNames
                            RowLayout {
                                required property var modelData
                                spacing: 14
                                Text { text: modelData.name; color: root.ink; font.family: root.sans; font.pixelSize: root.px(26); font.weight: Font.Bold }
                                Text { visible: modelData.name === root.mainName; text: "main"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
                                Link { text: "Copy"; onPressed: root.copy(modelData.name, "Your name") }
                            }
                        }
                        Hint {
                            text: root.cliNames.length > 0 ? "Anyone can pay any of these from Names. It arrives privately, in that name's own private account. Paid from a private account, the chain doesn't show who paid you or how much; paid from a public account, it shows both."
                                : "You don't have a name yet. With one, people pay you by name, privately."
                        }
                    }
                    Card {
                        visible: root.receiveKind !== "chain" && root.cliPublic !== null
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Your public account" }
                        Text {
                            Layout.fillWidth: true
                            text: root.cliPublic !== null ? "Public/" + root.cliPublic.account : ""
                            color: root.ink
                            font.family: root.mono
                            font.pixelSize: root.px(16)
                            wrapMode: Text.WrapAnywhere
                        }
                        Link { text: "Copy"; onPressed: root.copy("Public/" + root.cliPublic.account, "Your public account") }
                        Hint { font.pixelSize: root.px(15); text: "For apps, exchanges and the bridge. Anyone can see what it receives." }
                    }

                    Repeater {
                        model: root.receiveKind === "chain" && root.chainUsable !== null ? root.chainUsable : []
                        Card {
                            id: keyCard
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            Part { Layout.topMargin: 0; text: "Your node's key" }
                            Text {
                                Layout.fillWidth: true
                                text: keyCard.modelData.key
                                color: root.ink
                                font.family: root.mono
                                font.pixelSize: root.px(16)
                                wrapMode: Text.WrapAnywhere
                            }
                            RowLayout {
                                spacing: 14
                                Link { text: "Copy"; onPressed: root.copy(keyCard.modelData.key, "Your Logos Blockchain key") }
                                Link {
                                    visible: root.mainName !== "" && keyCard.index === 0
                                    text: "Let people pay " + root.mainName + " here"
                                    onPressed: root.keyOnName(keyCard.modelData.key)
                                }
                            }
                            Hint { font.pixelSize: root.px(15); text: "Tokens sent to this key are staked: they join the leader lottery at the next epoch." }
                        }
                    }
                    StatusLine {
                        visible: root.receiveKind === "chain" && root.chainAccounts === null
                        kind: root.chainError !== "" ? Logic.noticeKind(root.chainError, true) : "working"
                        text: root.chainError !== "" ? root.chainError : "Reading your node…"
                    }
                }

                // ── Stake: the amounts, then how it works and how to unstake ──
                ColumnLayout {
                    visible: root.walletView === "stake"
                    readonly property var table: Logic.stakeTable(root.chainStake, root.chainAccounts)
                    id: stakePage
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 14
                    PageHeader { title: "Staking"; net: "chain"; onBack: root.walletView = "home" }
                    Link { text: root.chainReading ? "Reading…" : "Refresh"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.loadChain() }
                    StatusLine { visible: root.chainError !== ""; kind: Logic.noticeKind(root.chainError, true); text: root.chainError }
                    Action {
                        visible: /isn't running|didn't answer/.test(root.chainError)
                        label: "Open the Blockchain app"
                        strong: true
                        onPressed: if (logos.request) logos.request("basecamp.apps.launch", { app: "blockchain_ui" }, function() { root.loadChain() })
                    }
                    Hint { visible: stakePage.table === null && root.chainError === ""; text: "Reading your node…" }

                    // The amounts.
                    Rectangle {
                        visible: stakePage.table !== null
                        Layout.fillWidth: true
                        implicitHeight: stakeRows.implicitHeight
                        radius: 12
                        color: root.card
                        border.width: 1
                        border.color: root.line
                        ColumnLayout {
                            id: stakeRows
                            width: parent.width
                            spacing: 0
                            Repeater {
                                model: stakePage.table === null ? [] : [
                                    { label: "On your node", value: stakePage.table.held, dot: "" },
                                    { label: "Staked now", value: stakePage.table.staked, dot: "full" },
                                    { label: "Not counted yet", value: stakePage.table.waiting, dot: "ring" },
                                ]
                                ColumnLayout {
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Rectangle { visible: index > 0; Layout.fillWidth: true; implicitHeight: 1; color: root.line }
                                    RowLayout {
                                        Layout.fillWidth: true
                                        Layout.margins: root.px(18)
                                        spacing: 12
                                        Rectangle {
                                            visible: modelData.dot !== ""
                                            implicitWidth: root.px(10)
                                            implicitHeight: root.px(10)
                                            radius: height / 2
                                            color: modelData.dot === "full" ? root.orange : "transparent"
                                            border.width: modelData.dot === "ring" ? 2 : 0
                                            border.color: root.orange
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: modelData.label
                                            color: modelData.dot === "full" ? root.orange : modelData.dot === "" ? root.dim : root.ink
                                            font.family: root.sans
                                            font.pixelSize: root.px(16)
                                            font.weight: modelData.dot === "full" ? Font.DemiBold : Font.Normal
                                        }
                                        Text { text: root.amountShown(modelData.value); color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.DemiBold }
                                    }
                                }
                            }
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: root.line }
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.margins: root.px(18)
                                spacing: 12
                                Text { Layout.fillWidth: true; text: "Rewards to claim"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16) }
                                Text {
                                    text: stakePage.table === null || stakePage.table.vouchers === 0 ? "None yet"
                                        : root.amountShown(stakePage.table.claimable) + " from " + stakePage.table.vouchers + (stakePage.table.vouchers === 1 ? " block" : " blocks")
                                    color: stakePage.table !== null && stakePage.table.vouchers > 0 ? root.ink : root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(17)
                                }
                                Action {
                                    label: root.chainBusy ? "Claiming…" : "Claim one"
                                    strong: stakePage.table !== null && stakePage.table.vouchers > 0
                                    tint: root.orange
                                    enabledNow: !root.chainBusy && stakePage.table !== null && stakePage.table.vouchers > 0
                                    onPressed: root.chainClaim()
                                }
                            }
                        }
                    }
                    Hint {
                        visible: text !== "" && stakePage.table !== null && stakePage.table.vouchers > 0
                        color: root.chainClaimPlan !== null && root.chainClaimPlan.warning ? root.ink : root.dim
                        font.pixelSize: root.px(14)
                        text: Logic.claimFeeLine(root.chainClaimPlan)
                    }
                    Hint {
                        visible: root.chainStake !== null
                        font.pixelSize: root.px(14)
                        text: root.chainStake === null ? "" : "Epoch " + root.chainStake.epoch + " · " + root.grouped(root.chainStake.notes) + (root.chainStake.notes === 1 ? " note" : " notes") + " on "
                            + (root.chainStake.keys || []).map(Logic.keyShown).join(", ")
                            + (root.chainStatus !== null ? " · node " + (root.chainStatus.mode || "?") + ", block " + root.grouped(root.chainStatus.height) : "")
                            + ". LEZ balances aren't part of this stake."
                    }

                    Card {
                        visible: root.chainStake !== null
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "How it works" }
                        Hint { font.pixelSize: root.px(16); text: "• There's no stake button: tokens on your node's key are staked by holding them there." }
                        Hint { font.pixelSize: root.px(16); text: "• New tokens start counting within two epochs of arriving." }
                        Hint { font.pixelSize: root.px(16); text: "• Staked tokens stay yours and spendable; spending them takes them out of the stake." }
                        Hint { font.pixelSize: root.px(16); text: "• Each block your node leads earns one reward. Each claim cashes one reward and pays its own fee, which can be more than the reward." }
                    }

                    Card {
                        visible: root.chainStake !== null
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Unstake" }
                        Hint { text: "Move tokens off your node's key. They leave the stake at a coming epoch boundary." }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                implicitHeight: unstakeSend.implicitHeight + root.px(32)
                                radius: 10
                                color: "transparent"
                                border.width: 1
                                border.color: root.line
                                ColumnLayout {
                                    id: unstakeSend
                                    x: root.px(16)
                                    y: root.px(16)
                                    width: parent.width - root.px(32)
                                    spacing: 10
                                    Text { text: "Send to a key outside this node"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold }
                                    Hint { font.pixelSize: root.px(14); text: "Stays on the Logos Blockchain. Sending to one of this node's own keys keeps it staked." }
                                    Action {
                                        label: "Send…"
                                        onPressed: {
                                            var rows = root.walletRows.filter(function(row) { return row.kind === "chain" })
                                            if (rows.length > 0)
                                                root.openSend(rows[0], "")
                                        }
                                    }
                                }
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                implicitHeight: unstakeBridge.implicitHeight + root.px(32)
                                radius: 10
                                color: "transparent"
                                border.width: 1
                                border.color: root.line
                                ColumnLayout {
                                    id: unstakeBridge
                                    x: root.px(16)
                                    y: root.px(16)
                                    width: parent.width - root.px(32)
                                    spacing: 10
                                    Text { text: "Bridge to LEZ"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(16); font.weight: Font.DemiBold }
                                    Hint { font.pixelSize: root.px(14); color: root.ink; text: "One way for now: LEZ can't send back to the Logos Blockchain yet. Arrives in about 12 hours." }
                                    Action { label: "Bridge…"; onPressed: root.openGetTokens() }
                                }
                            }
                        }
                    }
                    StatusLine { visible: root.chainProblem !== ""; kind: Logic.noticeKind(root.chainProblem, true); text: root.chainProblem }
                    StatusLine { visible: root.notice !== ""; kind: root.noticeKind; text: root.notice }
                }

                // ── Names: register one, and the ones you hold ──
                ColumnLayout {
                    visible: root.walletView === "names"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 14
                    PageHeader { title: "Names"; onBack: root.walletView = "home" }
                    NetSwitch {}
                    RunsOn {}
                    // Public registrations under way: when each reveal goes out, and by when it must.
                    Repeater {
                        model: root.pendingNames
                        Hint {
                            required property var modelData
                            Layout.fillWidth: true
                            color: root.waitTone
                            font.pixelSize: root.px(16)
                            text: root.revealRefused[modelData.name] ? modelData.name + ": this registration didn't land; register it again." : Logic.revealLine(modelData, Math.max(root.clockTick, Date.now()), root.clock)
                        }
                    }
                    Hint { text: "Your wallet can hold many names. Names held privately can't be linked to each other on chain; a name held publicly shows its holder. Each name is paid into its own private account unless it publishes a public one." }
                    // On another zone the names are still listed, and changed only on Home.
                    Card {
                        visible: !root.zoneHome
                        Layout.fillWidth: true
                        border.color: root.brass
                        Hint { textFormat: Text.PlainText; color: root.ink; text: "Names are kept on " + root.homeName + ", and the wallet is using " + root.zoneName + " now. They are listed here; to register, change or pay one, switch to " + root.homeName + "." }
                        Action { label: root.zoneBusy ? "Switching…" : "Switch to " + root.homeName; strong: true; enabledNow: !root.zoneBusy; onPressed: root.zoneUse("home") }
                        StatusLine { visible: root.zoneNoteBad; kind: Logic.noticeKind(root.zoneNote, true); text: root.zoneNote }
                    }

                    Card {
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Register a name" }
                        Hint { text: root.namesNet === "0.4" ? "Pick a name people can pay and message you by. On LEZ 0.4 it is registered from your public account, in the open." : "Pick a name people can pay and message you by. It's private from the start: the chain never learns which account holds it." }
                        Action { label: "Register a name"; strong: true; onPressed: root.openRegister() }
                    }

                    Card {
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "My names" }
                        Hint { visible: root.namesNote !== ""; color: root.ink; text: root.namesNote }
                        Hint { visible: root.allMine.length === 0 && root.cliPending.length === 0; text: root.cliKnown ? "None yet." : "Reading your names…" }
                        Repeater {
                            model: root.allMine
                            RowLayout {
                                id: myName
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 14
                                Text { text: myName.modelData.name; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.Bold }
                                Text { visible: myName.modelData.name === root.mainName; text: "main"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(14) }
                                Text {
                                    text: myName.modelData["private"] === false ? "Public" : "Private"
                                    color: myName.modelData["private"] === false ? root.dim : root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(14)
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: myName.modelData.state || ""
                                    color: myName.modelData.lapsed ? root.alarm : root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(14)
                                    elide: Text.ElideRight
                                }
                                Link { visible: myName.modelData["private"] !== false && root.namesNet !== "0.4"; text: "Make public…"; font.pixelSize: root.px(15); onPressed: { root.go("settings"); root.loadAccounts() } }
                                Link { visible: root.namesNet === "0.4" && !myName.modelData.account && root.publishAsk !== myName.modelData.name; text: "Let people pay it…"; font.pixelSize: root.px(15); onPressed: root.publishAsk = myName.modelData.name }
                                Link { visible: root.publishAsk === myName.modelData.name; text: "Publish " + Logic.idShort(myName.modelData.owner || ""); font.pixelSize: root.px(15); onPressed: { root.publishAsk = ""; root.publishAccount(myName.modelData.name, myName.modelData.owner || "") } }
                                Link { visible: root.publishAsk === myName.modelData.name; text: "Cancel"; font.pixelSize: root.px(15); onPressed: root.publishAsk = "" }
                                Link { visible: myName.modelData.name !== root.mainName; text: "Make main"; font.pixelSize: root.px(15); onPressed: root.setMain(myName.modelData.name) }
                                Link { text: "Open"; font.pixelSize: root.px(15); onPressed: { root.go("search"); root.show(myName.modelData.name) } }
                                Link { text: "Copy"; font.pixelSize: root.px(15); onPressed: root.copy(myName.modelData.name, "Your name") }
                                Link { text: "Transfer…"; font.pixelSize: root.px(15); onPressed: root.openTransfer(myName.modelData.name) }
                            }
                        }
                        Repeater {
                            model: root.cliPending
                            RowLayout {
                                required property var modelData
                                spacing: 14
                                Text { text: modelData.name; color: root.dim; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.Bold }
                                Text { text: modelData.failed ? "didn't register" : "registering"; color: modelData.failed ? root.alarm : root.waitTone; font.family: root.sans; font.pixelSize: root.px(14) }
                            }
                        }
                        // A name this wallet holds that isn't listed (handed to it, or registered
                        // before public registrations were listed): the CLI checks the chain first.
                        // The chain can't list a wallet's names by account, so it is typed.
                        // A name this wallet holds but doesn't list: behind an arrow, so it is never
                        // taken for Register (15 Nov: a new name typed here "didn't finish").
                        Link {
                            text: root.addNameOpen ? "Hold a name that isn't listed? ▾" : "Hold a name that isn't listed? ▸"
                            color: root.dim
                            font.pixelSize: root.px(15)
                            onPressed: root.addNameOpen = !root.addNameOpen
                        }
                        Hint { visible: root.addNameOpen; font.pixelSize: root.px(15); text: root.cliOutdated ? "The names program set for this network is too old to add one; LEZ 0.4's can." : "Type it: the wallet checks on chain that it holds it, then lists it. To get a new name, use Register a name." }
                        RowLayout {
                            visible: root.addNameOpen && !root.cliOutdated
                            Layout.fillWidth: true
                            spacing: 10
                            Field { id: addNameField; Layout.fillWidth: true; hint: "Name you hold" }
                            Action {
                                readonly property bool adding: root.pending !== null && root.pending.kind === "add_name"
                                label: adding ? "Checking…" : "Add"
                                enabledNow: root.pending === null && addNameField.text.trim() !== "" && root.zoneHome && !root.cliOutdated
                                onPressed: {
                                    var typed = addNameField.text.trim().toLowerCase().replace(/\.lez$/, "")
                                    root.addNameResult = null
                                    root.cliChange("add_name", [typed], typed, function() { return true }, "Added " + typed + ": this wallet holds it.")
                                    addNameField.text = ""
                                }
                            }
                        }
                        Hint { visible: root.addNameOpen && root.pending !== null && root.pending.kind === "add_name"; color: root.waitTone; text: "Checking on chain that this wallet holds " + (root.pending !== null ? root.pending.name : "") + "…" }
                        RowLayout {
                            visible: root.addNameOpen && root.addNameResult !== null
                            Layout.fillWidth: true
                            spacing: 12
                            Hint {
                                Layout.fillWidth: true
                                color: root.addNameResult !== null && root.addNameResult.done ? root.good : root.waitTone
                                text: root.addNameResult === null ? "" : root.addNameResult.notYours ? root.addNameResult.name + " isn't yours yet. To make it yours, register it." : root.addNameResult.line
                            }
                            Action {
                                visible: root.addNameResult !== null && root.addNameResult.notYours
                                label: "Register " + (root.addNameResult !== null ? root.addNameResult.name : "")
                                strong: true
                                onPressed: { var n = root.addNameResult.name; root.addNameResult = null; root.openRegister(); registerField.text = n; root.checkRegister(n) }
                            }
                        }
                    }

                    // Hand a name to someone else (names transfer).
                    Card {
                        visible: root.transferTarget !== ""
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Transfer " + root.transferTarget }
                        Hint {
                            font.pixelSize: root.px(15)
                            text: "Hands " + root.transferTarget + " to another LEZ account. Its receiving keys, profile entries and page are cleared; payments to it go to the new holder once they set it up. Any deposit goes with it. It can't be undone."
                        }
                        Part { text: "Their account" }
                        Field {
                            id: transferToField
                            Layout.fillWidth: true
                            size: 17
                            code: true
                            hint: "Public/… or Private/…, or a pasted account address"
                            onTextChanged: root.transferConfirm = false
                        }
                        StatusLine { visible: root.transferProblem() !== ""; kind: Logic.noticeKind(root.transferProblem(), true); text: root.transferProblem() }
                        RowLayout {
                            visible: !root.transferConfirm
                            spacing: 8
                            Action {
                                label: "Transfer…"
                                strong: true
                                enabledNow: transferToField.text.trim() !== "" && root.transferProblem() === "" && root.boardJob === null
                                onPressed: root.transferConfirm = true
                            }
                            Action { label: "Cancel"; onPressed: { root.transferTarget = ""; transferToField.text = "" } }
                        }
                        Card {
                            visible: root.transferConfirm
                            Layout.fillWidth: true
                            Hint {
                                color: root.ink
                                text: "Give " + root.transferTarget + " to " + Logic.keyShown(transferToField.text.trim().replace(/^(Public|Private)\//, "")) + "? It's proven on this computer, usually 5–12 minutes, and then it's theirs."
                            }
                            RowLayout {
                                spacing: 8
                                Action {
                                    label: "Yes, transfer it"
                                    grave: true
                                    enabledNow: root.boardJob === null
                                    onPressed: root.transferNow()
                                }
                                Action { label: "Cancel"; onPressed: root.transferConfirm = false }
                            }
                        }
                    }

                    // Take a name someone handed to you (names receive-privately). Not on LEZ 0.4,
                    // which is public only: there "Add" in My names lists a name handed to you.
                    Card {
                        visible: root.namesNet !== "0.4"
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Got a name from someone?" }
                        Hint { font.pixelSize: root.px(15); text: "Once they've transferred it to one of your accounts, set it up here: payments to it start arriving privately, at a new account of yours, and it joins My names." }
                        RowLayout {
                            spacing: 8
                            Field { id: takeNameField; Layout.preferredWidth: 260; size: 17; hint: "The name, such as ada" }
                            Action {
                                label: "Set it up"
                                enabledNow: takeNameField.text.trim() !== "" && root.boardJob === null
                                onPressed: root.takeName(takeNameField.text.trim())
                            }
                        }
                    }
                    Hint {
                        visible: root.boardJob !== null && /^cli_(transfer|receive_privately)$/.test(root.boardJob.kind)
                        color: root.waitTone
                        font.pixelSize: root.px(15)
                        text: root.boardJob === null ? "" : root.boardJob.label + ": proving it on this computer. Usually 5–12 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                    }
                    StatusLine { visible: root.notice !== "" && root.walletView === "names"; kind: root.noticeKind; text: root.notice }
                }

                // ── Claim a bridge deposit ──
                ColumnLayout {
                    id: claimScreen
                    visible: root.walletView === "claim"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    PageHeader { title: "Claim a bridge deposit"; net: "0.4"; onBack: root.leaveClaim() }

                    // The LEZ Bridge's setup, the same box as on Receive and the card (Logic.bridgeSetupState):
                    // Install, Update, the Unload step, a reopen; after it, back here.
                    BridgeSetupBox { then: "claim" }
                    RowLayout {
                        visible: root.claimStageNow === "missing" && root.bridgeSetup.state === ""
                        spacing: 12
                        StatusLine { Layout.fillWidth: false; kind: root.installedList === null || root.bridgeBundle === "" ? "working" : "waiting"; text: root.installedList === null || root.bridgeBundle === "" ? "Looking for the LEZ Bridge…" : "The LEZ Bridge doesn't answer yet." }
                        Link { text: "Look again"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.openClaim() }
                    }

                    StatusLine { visible: root.claimStageNow === "looking"; kind: "working"; text: "Looking for the LEZ Bridge…" }

                    // The claim as numbered steps: finished ones one line with a tick, the current
                    // one large with its one action, later ones grey; everything else under Details.
                    readonly property var steps: Logic.claimSteps({ first: root.claimSource.indexOf("deposit:") === 0 ? "arrived" : "file", stage: root.claimStageNow, extras: root.claimExtras, extrasDone: root.claimExtrasDone,
                        needs: root.claimNeeds !== null ? root.claimNeeds.needs : "", job: root.depositClaimJob, summary: root.claimSummaryNow })
                    readonly property var stepNow: steps.filter(function(x) { return x.state === "now" })[0] || null
                    readonly property bool stepsShown: root.claimStageNow !== "missing" && root.claimStageNow !== "looking" && root.bridgeSetupClear
                    StepHeader { visible: claimScreen.stepsShown || root.claimStageNow === "done"; steps: claimScreen.steps }

                    // Step: the file.
                    Hint { visible: root.claimStageNow === "start"; text: "Use the claim file the deposit page gave you." }
                    RowLayout {
                        visible: root.claimStageNow === "start"
                        Layout.fillWidth: true
                        spacing: 10
                        Action { label: "Choose file"; strong: true; onPressed: root.pickClaimFile() }
                        // Shown as dots: the file holds the claim's secret. It leaves the field when checked.
                        Field { id: claimPasteField; Layout.fillWidth: true; secret: true; hint: "or paste it here"; onTextChanged: root.claimPaste = text }
                        Action { visible: claimPasteField.text.trim() !== ""; label: "Check"; onPressed: { var text = claimPasteField.text; claimPasteField.text = ""; root.checkClaimFile(text) } }
                    }
                    StatusLine { visible: root.claimNote !== ""; kind: ""; text: root.claimNote }

                    // Step: the check.
                    Hint { visible: root.claimStageNow === "ready" || root.claimStageNow === "elsewhere" || root.claimStageNow === "wrong_bridge"; color: root.ink; text: root.claimSummaryNow }
                    StatusLine { visible: root.claimStageNow === "elsewhere"; kind: "problem"; text: root.claimAccountProblemNow }
                    StatusLine { visible: root.claimStageNow === "wrong_bridge"; kind: "problem"; text: root.claimBridgeNow.problem }
                    Hint { visible: root.claimStageNow === "ready" && root.claimCheck !== null && !!root.claimCheck.privacy; color: root.ink; font.pixelSize: root.px(15); text: "Public claim: links this deposit to this account. The bridge pays the fee." }
                    // Approve (next) is the real action: this only goes on to it. While a "try
                    // again" wait shows under it, it is the one button (no second primary).
                    Action { visible: root.claimStageNow === "ready"; label: root.claimAsking ? "Asking the wallet…" : "Continue"; strong: true; enabledNow: !root.claimAsking; onPressed: root.startClaim(false) }
                    Hint { visible: root.claimStageNow === "ready" && !(root.claimCheck !== null && !!root.claimCheck.privacy); font.pixelSize: root.px(15); text: "The bridge pays the fee." }

                    // The wallet's questions for this claim, answered here as the step itself.
                    Hint {
                        visible: false
                    }
                    StatusLine {
                        visible: !root.appInline && (root.claimStageNow === "asking" || root.claimStageNow === "needs" || (root.claimStageNow === "running" && claimScreen.stepNow !== null && claimScreen.stepNow.key === "approve"))
                        kind: root.claimStageNow === "asking" || root.appRequest === null ? "working" : "waiting"
                        text: root.claimStageNow === "asking" ? "Asking the wallet…" : root.appRequest !== null ? "Waiting for your answer in the approval sheet." : "Asking the wallet…"
                    }
                    ApproveBody { visible: root.appInline && root.walletView === "claim"; inline: true }
                    StatusLine { visible: root.claimStageNow === "uncertain"; kind: "waiting"; text: "No answer from the wallet yet. If an approval for this claim is open, reject it, then press Continue." }
                    Action { visible: root.claimStageNow === "uncertain"; label: "Continue"; strong: true; onPressed: root.startClaim(true) }

                    // The last step: sending, then done.
                    StatusLine {
                        visible: root.claimStageNow === "running" && claimScreen.stepNow !== null && claimScreen.stepNow.key === "done"
                        kind: "waiting"
                        text: root.depositClaimJob !== null && root.depositClaimJob.phase === "confirm" ? "Waiting for its block (about 30 s)…" : "The bridge's relay is sending it…"
                    }
                    Action { visible: root.claimStageNow === "running" && root.depositClaimJob !== null && root.depositClaimJob.can_cancel === true; label: "Cancel"; onPressed: root.cancelClaim() }
                    StatusLine {
                        visible: root.claimStageNow === "done"
                        kind: "done"
                        text: (root.depositClaimJob !== null && root.depositClaimJob.block !== undefined ? "Claimed in block " + root.depositClaimJob.block : "Claimed")
                            + (root.depositClaimJob !== null && root.depositClaimJob.result && root.depositClaimJob.result.balance_text ? " · Now in the account: " + root.depositClaimJob.result.balance_text + " (wrapped)." : ".")
                    }
                    // A refusal (the check) or a failure (the claim): one line in its kind, and its one
                    // next action (Logic.claimNext): Check again only where it can change anything.
                    StatusLine {
                        visible: (root.claimStageNow === "refused" || root.claimStageNow === "failed") && !root.claimChecking
                        kind: root.claimNextNow.kind
                        text: root.claimStageNow === "refused" ? root.claimRefusalLine
                            : root.depositClaimJob !== null && root.depositClaimJob.error ? Logic.claimErrorLine(root.depositClaimJob.error.code, root.depositClaimJob.error) : "The claim stopped."
                        actionLabel: root.claimNextNow.label
                        actionStrong: true
                        onAct: root.claimNextAction(root.claimNextNow.action)
                    }
                    // A proposal that didn't go through: its line under Continue (the one button), or,
                    // with no check to continue from, a way back to one.
                    StatusLine {
                        visible: root.claimError !== ""
                        kind: root.claimErrorKind
                        text: root.claimError
                        actionLabel: root.claimCheck === null && root.claimStageNow !== "refused" && root.claimStageNow !== "failed" ? (root.claimSource !== "" ? "Check now" : "Choose a file") : ""
                        onAct: if (root.claimSource !== "") root.checkAgain(); else { root.leaveClaim(); root.openClaim() }
                    }

                    // Not ready yet: the same file is checked again (by itself when there is a time).
                    Hint { visible: root.claimRetryAt > 0; font.pixelSize: root.px(15); text: "Checking again by itself in " + root.claimRetryLeft + "." }
                    Link { visible: !root.claimChecking && root.claimSource !== "" && root.claimSource.indexOf("deposit:") !== 0 && root.claimStageNow === "refused" && root.claimNextNow.action !== "another"; text: "Choose another file"; font.pixelSize: root.px(15); onPressed: { root.leaveClaim(); root.openClaim() } }

                    // Details, closed by default: every fact the screen used to show.
                    Link {
                        visible: claimScreen.stepsShown && root.claimStageNow !== "start" && !root.appInline
                        text: root.claimDetails ? "Details ▾" : "Details ▸"
                        color: root.dim
                        font.pixelSize: root.px(15)
                        onPressed: root.claimDetails = !root.claimDetails
                    }
                    ColumnLayout {
                        visible: root.claimDetails && claimScreen.stepsShown && root.claimStageNow !== "start" && !root.appInline
                        Layout.fillWidth: true
                        spacing: 10
                        Hint { visible: root.claimStageNow === "checking"; font.pixelSize: root.px(15); text: "Checking the claim file on Sepolia and the bridge test network. Nothing is signed or sent." }
                        Hint { visible: root.claimCheck !== null && root.claimCheck.ok === true; font.pixelSize: root.px(15); text: root.claimCheck !== null && root.claimCheck.ok ? String(root.claimCheck.network || "LEZ 0.4 · bridge test network") : "" }
                        Fact { visible: root.claimCheck !== null && root.claimCheck.ok === true; label: "Into"; code: true; value: root.claimAccount; full: root.claimAccount }
                        Fact { visible: root.claimCheck !== null && root.claimCheck.ok === true; label: "Bridge"; value: root.claimBridgeNow.label }
                        Fact { visible: root.claimCheck !== null && root.claimCheck.ok === true; label: "From"; value: root.claimCheck !== null && root.claimCheck.deposit ? "Sepolia deposit, leaf " + root.claimCheck.deposit.leaf_index + " of " + root.claimCheck.deposit.leaves + " (final)" : "" }
                        Fact { visible: root.claimCheck !== null && root.claimCheck.ok === true; label: "Signs"; value: "Your wallet signs. The bridge's relay sends it." }
                        Fact { visible: root.claimCheck !== null && root.claimCheck.ok === true && !!root.claimCheck.relay; label: "Fee"; value: root.claimCheck !== null && root.claimCheck.relay ? "Paid by the bridge's relay, at most " + root.grouped(root.claimCheck.relay.max_fee) + " LEZ" + (root.claimCheck.relay.payer ? " (" + root.accountShort(root.claimCheck.relay.payer) + ")" : "") : "" }
                        Repeater {
                            model: root.claimCheck !== null && Array.isArray(root.claimCheck.checks) ? root.claimCheck.checks : []
                            Hint {
                                required property var modelData
                                font.pixelSize: root.px(15)
                                color: modelData.ok ? root.ink : root.alarm
                                text: (modelData.ok ? "✓ " : "✗ ") + modelData.text
                            }
                        }
                        Hint { visible: root.claimCheck !== null && !!root.claimCheck.privacy; font.pixelSize: root.px(15); text: root.claimCheck !== null && root.claimCheck.privacy ? root.claimCheck.privacy + " Private claims aren't in this wallet yet." : "" }
                        Hint { visible: root.claimCheck !== null && !!root.claimCheck.approval; font.pixelSize: root.px(15); text: root.claimCheck !== null && root.claimCheck.approval ? root.claimCheck.approval : "" }
                        Hint { visible: root.claimCheck !== null && root.claimCheck.ok === false && !!root.claimCheck.error; font.pixelSize: root.px(15); text: root.claimCheck !== null && root.claimCheck.error ? Logic.claimErrorTitle(root.claimCheck.error.code) + ": " + String(root.claimCheck.error.message || "") : "" }
                        Repeater {
                            model: root.depositClaimJob !== null ? Logic.claimPhases(root.depositClaimJob) : []
                            Hint {
                                required property var modelData
                                font.pixelSize: root.px(15)
                                color: modelData.done ? root.good : modelData.now ? root.brass : root.dim
                                text: (modelData.done ? "✓ " : modelData.now ? "… " : "· ") + modelData.word + ": " + modelData.hint
                            }
                        }
                        Hint { visible: root.depositClaimJob !== null && !!root.depositClaimJob.note; font.pixelSize: root.px(15); text: root.depositClaimJob !== null && root.depositClaimJob.note ? root.depositClaimJob.note : "" }
                        Hint { visible: root.depositClaimJob !== null && !!root.depositClaimJob.error; font.pixelSize: root.px(15); text: root.depositClaimJob !== null && root.depositClaimJob.error ? Logic.claimErrorTitle(root.depositClaimJob.error.code) + ": " + String(root.depositClaimJob.error.message || "") : "" }
                        Hint { visible: root.claimUncertain !== null; font.pixelSize: root.px(15); text: root.claimUncertain !== null ? String(root.claimUncertain.message || "") : "" }
                        Hint { visible: root.claimNeeds !== null && !!root.claimNeeds.message; font.pixelSize: root.px(15); text: root.claimNeeds !== null ? String(root.claimNeeds.message || "") : "" }
                    }

                    Link {
                        visible: root.claimStageNow === "done"
                        text: "Claim another deposit"
                        font.pixelSize: root.px(15)
                        onPressed: { root.leaveClaim(); root.openClaim() }
                    }
                }

                // ── Receive from Ethereum ──
                ColumnLayout {
                    id: receiveScreen
                    visible: root.walletView === "ethreceive"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    readonly property var stepNow: root.receiveStepsNow.filter(function(x) { return x.state === "now" })[0] || null
                    readonly property var result: root.receiveResult
                    // The account the code pays: the code's own (one shown again), else the one chosen.
                    readonly property string resultAccount: result !== null && result.account ? String(result.account) : root.receiveAccount
                    readonly property var refusal: root.receiveFocus !== "" ? (root.receiveCodeRefusals[root.receiveFocus] || null) : null
                    readonly property var releaseNotice: root.bridgeConfig !== null && root.bridgeConfig.notices && root.bridgeConfig.notices.release && root.bridgeConfig.notices.release.text ? root.bridgeConfig.notices.release : null
                    readonly property var p: root.receiveFocusProgress
                    // The receive followed has no deposit seen yet: its code (when made here) is the step.
                    readonly property bool depositing: root.receiveFocus === "" || p.stage === "waiting" || p.stage === "checking"
                    readonly property bool shows: !root.receiveMissing && root.bridgeReceives && root.bridgeSetupClear
                    PageHeader { title: "Receive from Ethereum"; net: "0.4"; onBack: root.walletView = "home" }
                    // The LEZ Bridge's setup, the same box as on the claim screen and the card.
                    BridgeSetupBox { then: "receive" }
                    RowLayout {
                        visible: root.receiveMissing && root.bridgeSetup.state === ""
                        spacing: 12
                        StatusLine { Layout.fillWidth: false; kind: root.installedList === null || root.bridgeBundle === "" ? "working" : "waiting"; text: root.installedList === null || root.bridgeBundle === "" ? "Looking for the LEZ Bridge…" : "The LEZ Bridge doesn't answer yet." }
                        Link { text: "Look again"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.openEthReceive(root.receiveFocus === "" ? undefined : root.receiveFocus) }
                    }
                    // A receive opened again (or on its way when Receive opened): said first.
                    Text {
                        visible: receiveScreen.shows && root.receiveFocus !== "" && receiveScreen.result === null
                        Layout.fillWidth: true
                        text: Logic.receiveOnItsWay(root.receiveFocusRow, root.receiveStatuses[root.receiveFocus])
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(17)
                        wrapMode: Text.WordWrap
                    }

                    // The steps in a row (done ticked, the current one marked, later ones dim); under
                    // it only the current step.
                    StepHeader { visible: receiveScreen.shows; steps: root.receiveStepsNow }

                    // Step: amount and account.
                    ColumnLayout {
                        visible: receiveScreen.shows && root.receiveFocus === "" && root.receiveResult === null && root.receiveNeeds === null
                        Layout.fillWidth: true
                        spacing: 10
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: root.receiveTokenList
                                Choice { required property var modelData; label: modelData.symbol; chosen: root.receiveToken === modelData.symbol; onPressed: root.receiveToken = modelData.symbol }
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: (root.receiveTokenList.filter(function(t) { return t.symbol === root.receiveToken })[0] || { denoms: [] }).denoms
                                Choice { required property var modelData; label: modelData.text + " " + root.receiveToken; chosen: root.receiveDenom === modelData.index; onPressed: root.receiveDenom = modelData.index }
                            }
                        }
                        StatusLine { visible: root.receiveAccountList.length === 0; kind: "problem"; text: "No public account on LEZ 0.4 yet. Make one in Settings, then come back."; actionLabel: "Open Settings"; onAct: root.go("settings") }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: root.receiveAccountList
                                Choice { required property var modelData; label: "Into " + Logic.idShort(modelData); chosen: root.receiveAccountNow === modelData; onPressed: root.receiveAccount = modelData }
                            }
                        }
                        // The one thing to read first: a public account links the deposit to it.
                        Tick {
                            checked: root.receivePublicOk
                            text: "Not private: the claim links this deposit to this account for good."
                            onToggled: root.receivePublicOk = !root.receivePublicOk
                        }
                        StatusLine { visible: root.receiveHoldNow !== ""; kind: ""; text: root.receiveHoldNow }
                        Action { visible: root.receiveHoldNow === ""; label: root.receiveAsking ? "Making the code…" : "Make the deposit code"; strong: true; enabledNow: !root.receiveAsking && root.receivePublicOk && root.receiveAccountNow !== ""; onPressed: root.makeReceive() }
                        WhyLine { visible: root.receiveHoldNow === "" && text !== ""; text: Logic.receiveMakeWhy({ asking: root.receiveAsking, ticked: root.receivePublicOk, account: root.receiveAccountNow }) }
                    }
                    StatusLine { visible: root.receiveNeeds !== null && !root.appInline; kind: "waiting"; text: "Waiting for your answer in the approval sheet." }
                    // This screen's own question to the wallet, answered here (not in a sheet over it).
                    ApproveBody { visible: root.appInline && root.walletView === "ethreceive"; inline: true }

                    // Step: the code, for the deposit page (until the deposit is seen).
                    ColumnLayout {
                        visible: receiveScreen.result !== null && receiveScreen.depositing
                        Layout.fillWidth: true
                        spacing: 12
                        Hint {
                            color: root.ink
                            text: receiveScreen.result === null ? "" : receiveScreen.result.bridge_fee && receiveScreen.result.bridge_fee.text ? receiveScreen.result.bridge_fee.text
                                : (receiveScreen.result.denom_text ? String(receiveScreen.result.denom_text) + " " + String(receiveScreen.result.symbol || "") : String(receiveScreen.result.amount_text || receiveScreen.result.symbol || ""))
                                  + " into " + Logic.idShort(receiveScreen.resultAccount) + ". The deposit page shows the bridge fee."
                        }
                        // A code shown again that was made for an older vault.
                        StatusLine {
                            visible: Logic.codeOlderVault(receiveScreen.result)
                            kind: ""
                            text: "This code is for an older vault. For the new one (v4), make another code."
                        }
                        Action { label: root.receiveLinkCopied ? "Copied." : "Copy the link"; strong: true; onPressed: root.copyDepositLink(receiveScreen.result.request_url) }
                        Hint { font.pixelSize: root.px(15); text: "Paste it into your browser." }
                        Text {
                            Layout.fillWidth: true
                            text: Logic.compareLine(receiveScreen.result !== null ? receiveScreen.result.compare : null)
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(20)
                            font.weight: Font.DemiBold
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            visible: text !== ""
                            Layout.fillWidth: true
                            text: Logic.compareWords(receiveScreen.result !== null ? receiveScreen.result.compare : null)
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(30)
                            font.weight: Font.Bold
                            wrapMode: Text.WordWrap
                        }
                        Hint { font.pixelSize: root.px(15); text: "No file to save: this wallet keeps what it needs." }
                        RowLayout {
                            spacing: 14
                            Link { text: "Copy the code"; font.pixelSize: root.px(15); onPressed: root.copy(receiveScreen.result.request, "The code") }
                            Link { text: root.receiveQrOpen ? "On your phone ▾" : "On your phone ▸"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.receiveQrOpen = !root.receiveQrOpen }
                        }
                        RowLayout {
                            visible: root.receiveQrOpen
                            spacing: 18
                            QrCode { rows: receiveScreen.result !== null && receiveScreen.result.request_qr ? receiveScreen.result.request_qr.rows : []; side: root.px(180) }
                            Hint { Layout.fillWidth: true; font.pixelSize: root.px(15); text: "Scan this with your phone, or copy the link." }
                        }
                    }
                    // Where the deposit is, as it happens: waiting for it, seen, final, arrived (Claim
                    // here), claimed. The wait is the headline, in the third tone.
                    UnderwayPanel {
                        visible: receiveScreen.shows && root.receiveFocus !== "" && root.receiveHasStatus
                        StatusLine {
                            kind: Logic.toneKind(receiveScreen.p.tone)
                            text: receiveScreen.p.stage === "claimed" ? receiveScreen.p.headline.replace(/\.$/, "")
                                + (root.receiveFocusRow !== null && /now holds/.test(Logic.receiveBalanceLine(root.bridgeNetBlocks, root.receiveFocusRow.account, root.receiveFocusRow.symbol))
                                   ? " · " + Logic.receiveBalanceLine(root.bridgeNetBlocks, root.receiveFocusRow.account, root.receiveFocusRow.symbol) : ".")
                                : receiveScreen.p.headline
                        }
                        Hint { visible: receiveScreen.p.detail !== ""; font.pixelSize: root.px(15); text: receiveScreen.p.detail }
                        Hint {
                            visible: receiveScreen.p.stage === "claimable" && root.receiveFocusRow !== null
                            color: root.ink
                            text: root.receiveFocusRow === null ? "" : root.receiveFocusRow.amount + " " + root.receiveFocusRow.symbol + " is ready to claim into " + Logic.idShort(root.receiveFocusRow.account) + ". The bridge's relay pays the fee."
                        }
                        Action { visible: receiveScreen.p.claim; label: "Claim"; strong: true; onPressed: root.claimReceive(root.receiveFocus) }
                        Hint {
                            visible: receiveScreen.p.stage === "claimed" && root.receiveFocusRow !== null && !/now holds/.test(text)
                            font.pixelSize: root.px(15)
                            text: root.receiveFocusRow === null ? "" : Logic.receiveBalanceLine(root.bridgeNetBlocks, root.receiveFocusRow.account, root.receiveFocusRow.symbol)
                        }
                        Hint {
                            visible: receiveScreen.p.stage === "waiting" && receiveScreen.result === null
                            font.pixelSize: root.px(15)
                            text: receiveScreen.refusal !== null ? receiveScreen.refusal.line + " If you haven't deposited yet, make another code."
                                : Logic.bridgeCan(root.bridgeFeatures, "receive_request") && root.receiveHold ? "Getting its code…"
                                : "Its page link shows only when the code is made. If you haven't deposited yet, make another code."
                        }
                        ReceiveCheckLine { visible: receiveScreen.p.stage !== "claimed" && receiveScreen.p.stage !== "claimable" }
                    }
                    RowLayout {
                        visible: root.receiveError !== ""
                        spacing: 12
                        StatusLine {
                            Layout.fillWidth: true
                            kind: Logic.noticeKind(root.receiveError, true)
                            text: root.receiveError
                            actionLabel: root.receiveResult === null && root.receiveNeeds === null && !root.receiveAsking ? (Logic.approvalEnded(root.receiveError) ? "Start again" : "Try again") : ""
                            onAct: root.makeReceive()
                        }
                    }

                    Link {
                        visible: !root.receiveMissing && !root.appInline
                        text: root.receiveDetails ? "Details ▾" : "Details ▸"
                        color: root.dim
                        font.pixelSize: root.px(15)
                        onPressed: root.receiveDetails = !root.receiveDetails
                    }
                    ColumnLayout {
                        visible: root.receiveDetails && !root.receiveMissing && !root.appInline
                        Layout.fillWidth: true
                        spacing: 10
                        Hint { font.pixelSize: root.px(15); text: "The wallet makes this deposit's secret from your account's own key; this app never sees it. The page gets only a code: the amount, the vault and a commitment that hides which account receives it." }
                        Hint { visible: root.bridgeConfig !== null && !!root.bridgeConfig.notices && !!root.bridgeConfig.notices.not_private; font.pixelSize: root.px(15); text: root.bridgeConfig !== null && root.bridgeConfig.notices && root.bridgeConfig.notices.not_private ? String(root.bridgeConfig.notices.not_private.text || "") : "" }
                        Hint { font.pixelSize: root.px(15); text: "Private accounts can't receive on the bridge test network yet: their claim needs a proof that hasn't run there." }
                        Fact { visible: receiveScreen.result !== null; label: "Into"; code: true; value: receiveScreen.resultAccount; full: receiveScreen.resultAccount }
                        Hint { visible: receiveScreen.refusal !== null && receiveScreen.refusal.detail !== ""; font.pixelSize: root.px(15); text: receiveScreen.refusal !== null ? receiveScreen.refusal.detail : "" }
                        Hint { visible: receiveScreen.releaseNotice !== null; font.pixelSize: root.px(15); text: receiveScreen.releaseNotice !== null ? String(receiveScreen.releaseNotice.text) : "" }
                        Hint { visible: receiveScreen.releaseNotice !== null && !!receiveScreen.releaseNotice.detail; font.pixelSize: root.px(15); text: receiveScreen.releaseNotice !== null ? String(receiveScreen.releaseNotice.detail || "") : "" }
                        Hint { visible: receiveScreen.result !== null; font.pixelSize: root.px(15); text: "Text copied in Names may not reach another computer through Screen Sharing: paste the link in a browser on this computer, use the QR (On your phone), or select it below." }
                        // The link as text that can be selected by hand.
                        TextEdit {
                            visible: receiveScreen.result !== null
                            Layout.fillWidth: true
                            text: receiveScreen.result !== null ? String(receiveScreen.result.request_url || "") : ""
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.WrapAnywhere
                            color: root.ink
                            selectionColor: root.underwayLine
                            font.family: root.mono
                            font.pixelSize: root.px(14)
                        }
                        Fact { visible: receiveScreen.result !== null; label: "Code"; code: true; value: receiveScreen.result !== null ? String(receiveScreen.result.request || "") : ""; full: receiveScreen.result !== null ? String(receiveScreen.result.request || "") : "" }
                        Fact { visible: receiveScreen.result !== null && !!receiveScreen.result.commitment; label: "Commitment"; code: true; value: receiveScreen.result !== null ? String(receiveScreen.result.commitment || "") : ""; full: receiveScreen.result !== null ? String(receiveScreen.result.commitment || "") : "" }
                        Hint { visible: receiveScreen.result !== null && !!receiveScreen.result.not_private; font.pixelSize: root.px(15); text: receiveScreen.result !== null && receiveScreen.result.not_private ? String(receiveScreen.result.not_private.text || "") : "" }
                        Hint { visible: receiveScreen.result !== null && !!receiveScreen.result.bridge_fee && !!receiveScreen.result.bridge_fee.error; font.pixelSize: root.px(15); text: receiveScreen.result !== null && receiveScreen.result.bridge_fee && receiveScreen.result.bridge_fee.error ? String(receiveScreen.result.bridge_fee.error) : "" }
                        Hint { visible: receiveScreen.result !== null && !!receiveScreen.result.compare; font.pixelSize: root.px(15); text: receiveScreen.result !== null && receiveScreen.result.compare ? String(receiveScreen.result.compare.text || "") : "" }
                        Hint { visible: text !== ""; font.pixelSize: root.px(15); text: Logic.compareDigitsDetail(receiveScreen.result !== null ? receiveScreen.result.compare : null) }
                        Hint { visible: root.receiveNeeds !== null && !!root.receiveNeeds.message; font.pixelSize: root.px(15); text: root.receiveNeeds !== null ? String(root.receiveNeeds.message || "") : "" }
                    }
                    Link { visible: receiveScreen.shows && (root.receiveFocus !== "" || receiveScreen.result !== null); text: "Make another code"; font.pixelSize: root.px(15); onPressed: root.startNewReceive() }
                }

                // ── Send back to Ethereum ──
                ColumnLayout {
                    id: sendBackScreen
                    visible: root.walletView === "sendback"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    readonly property var stepNow: root.sbStepsNow.filter(function(x) { return x.state === "now" })[0] || null
                    readonly property var p: root.sbProgressNow
                    readonly property var st: root.sbFocusStatus
                    // A bridge that can't send back now (send_back_link: its vault can't release) offers
                    // no new send-back; one already made is still followed.
                    readonly property bool closed: root.sbFocus === "" && root.sbLink !== null && root.sbLink.available === false
                    readonly property bool shows: !root.receiveMissing && root.bridgeSetupClear && root.sbCan && !closed
                    readonly property bool choosing: root.sbFocus === "" && root.sbNeeds === null
                    // The burn's approval is drawn here (appInline): it is the step, not the panel.
                    readonly property bool burning: root.appInline && root.sbFocus !== "" && (p.stage === "planned" || p.stage === "approve")
                    PageHeader { title: "Bridge to Ethereum"; net: "0.4"; onBack: root.walletView = "home" }
                    BridgeSetupBox { then: "sendback" }
                    StatusLine { visible: !root.receiveMissing && root.bridgeSetupClear && root.bridgeConfig !== null && !root.sbCan; kind: ""; text: "This LEZ Bridge can't bridge to Ethereum yet. Update it (above)." }
                    RowLayout {
                        visible: root.receiveMissing && root.bridgeSetup.state === ""
                        spacing: 12
                        StatusLine { Layout.fillWidth: false; kind: "waiting"; text: "The LEZ Bridge doesn't answer yet." }
                        Link { text: "Look again"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.openSendBack(root.sbAccount, root.sbAsset, root.sbFocus === "" ? undefined : root.sbFocus) }
                    }
                    // A bridge whose vault can't release: said plainly (a fact, not a wait), with the way back.
                    StatusLine { visible: !root.receiveMissing && sendBackScreen.closed; kind: ""; text: root.sbLink !== null && root.sbLink.why ? root.sentence(String(root.sbLink.why)) : ""; actionLabel: "Back to the wallet"; onAct: root.walletView = "home" }

                    StepHeader { visible: sendBackScreen.shows; steps: root.sbStepsNow }

                    // Step 1: the amount, one denomination this account holds.
                    ColumnLayout {
                        visible: sendBackScreen.shows && sendBackScreen.choosing && root.sbDenom < 0
                        Layout.fillWidth: true
                        spacing: 10
                        // The accounts holding a token on bridge v4: short id, the token, what it holds.
                        Flow {
                            visible: root.sbSources.length > 0
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: root.sbSources
                                Choice {
                                    required property var modelData
                                    label: Logic.idShort(modelData.account) + " · " + modelData.amount + " " + modelData.symbol
                                    chosen: root.sbAsset !== null && root.sbAccount === modelData.account && root.sbAsset.symbol === modelData.symbol
                                    onPressed: root.sbPick(modelData)
                                }
                            }
                        }
                        // Then the amounts that account holds.
                        Flow {
                            visible: root.sbAsset !== null
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: root.sbAmounts
                                Choice { required property var modelData; label: modelData.text + " " + (root.sbAsset !== null ? root.sbAsset.symbol : ""); onPressed: root.sbDenom = modelData.index }
                            }
                        }
                        // Nothing to send back: said in one line, with the way to get some.
                        RowLayout {
                            visible: root.sbSources.length === 0
                            spacing: 14
                            StatusLine { Layout.fillWidth: false; kind: root.bridgeNet === null ? "working" : ""; text: root.bridgeNet === null ? "Reading your accounts on the bridge test network…" : "None of your public accounts holds tokens on bridge v4 yet." }
                            Action { visible: root.bridgeNet !== null; label: "Receive from Ethereum"; onPressed: root.openEthReceive() }
                        }
                    }

                    // Step 2: the Ethereum address, from the bridge page: copy its link, connect there, paste its code.
                    ColumnLayout {
                        visible: sendBackScreen.shows && sendBackScreen.choosing && root.sbDenom >= 0
                        Layout.fillWidth: true
                        spacing: 12
                        RowLayout {
                            spacing: 14
                            Hint { Layout.fillWidth: false; color: root.ink; text: (root.sbAmounts.filter(function(d) { return d.index === root.sbDenom })[0] || { text: "" }).text + " " + (root.sbAsset !== null ? root.sbAsset.symbol : "") + " from " + Logic.idShort(root.sbAccount) + "." }
                            Link { visible: root.sbAmounts.length > 1 && root.sbCheck === null; text: "Change"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.sbDenom = -1 }
                        }
                        ColumnLayout {
                            visible: root.sbCheck === null
                            Layout.fillWidth: true
                            spacing: 10
                            Action { label: root.sbLinkCopied ? "Copied." : "Copy the link"; strong: true; enabledNow: root.sbLink !== null && !!root.sbLink.link; onPressed: root.sbCopyLink() }
                            Hint { font.pixelSize: root.px(15); text: "Open it in your browser, connect the Ethereum wallet that should receive, and paste the code it shows here." }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 10
                                Field { id: sbCodeField; Layout.fillWidth: true; size: 15; code: true; hint: "logoswd1…"; onTextChanged: root.sbCodeInput = text; onAccepted: root.sbCheckCode() }
                                Action { label: root.sbAsking ? "Reading…" : "Read the code"; enabledNow: !root.sbAsking && Logic.sendBackCode(root.sbCodeInput) !== ""; onPressed: root.sbCheckCode() }
                            }
                            WhyLine { visible: !root.sbAsking && Logic.sendBackCode(root.sbCodeInput) === "" && root.sbCheckError === ""; text: "Paste the code from the bridge page to read it." }
                            StatusLine { visible: root.sbCheckError !== ""; kind: Logic.noticeKind(root.sbCheckError, true); text: root.sbCheckError }
                        }
                        // The address the code names, and its four words: the same as on the page.
                        ColumnLayout {
                            visible: root.sbCheck !== null
                            Layout.fillWidth: true
                            spacing: 8
                            Hint { color: root.ink; text: "To this Ethereum address:" }
                            Text { Layout.fillWidth: true; text: root.sbCheck !== null ? String(root.sbCheck.recipient || "") : ""; textFormat: Text.PlainText; color: root.ink; font.family: root.mono; font.pixelSize: root.px(16); wrapMode: Text.WrapAnywhere }
                            Text { Layout.fillWidth: true; text: "Check these four words match the bridge page. If they differ, stop."; color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.DemiBold; wrapMode: Text.WordWrap }
                            Text { Layout.fillWidth: true; text: root.sbCheck !== null ? String(root.sbCheck.words_text || "") : ""; color: root.ink; font.family: root.sans; font.pixelSize: root.px(30); font.weight: Font.Bold; wrapMode: Text.WordWrap }
                            Repeater {
                                model: root.sbCheck !== null && Array.isArray(root.sbCheck.warnings) ? root.sbCheck.warnings : []
                                Hint { required property var modelData; color: root.ink; text: "!  " + String(modelData) }
                            }
                            RowLayout {
                                spacing: 14
                                Action { label: root.sbAsking ? "Getting it ready…" : "Bridge to Ethereum"; strong: true; enabledNow: !root.sbAsking; onPressed: root.sbPlan() }
                                Link { visible: !root.sbAsking; text: "Another code"; color: root.dim; font.pixelSize: root.px(15); onPressed: { root.sbCheck = null; root.sbCodeInput = ""; sbCodeField.text = "" } }
                            }
                        }
                    }

                    StatusLine { visible: root.sbNeeds !== null && !root.appInline; kind: "waiting"; text: "Waiting for your answer in the approval sheet." }
                    // The burn's approval: where it goes first (the burn carries only a commitment, so
                    // the wallet's own sheet can't show the address), then the approval itself.
                    ColumnLayout {
                        visible: sendBackScreen.burning && root.sbFocusRow !== null
                        Layout.fillWidth: true
                        spacing: 6
                        Hint { color: root.ink; text: root.sbFocusRow !== null ? root.sbFocusRow.amount + " to this Ethereum address" + (sendBackScreen.st !== null && sendBackScreen.st.receive_text ? " (you receive " + sendBackScreen.st.receive_text + "):" : ":") : "" }
                        Text { Layout.fillWidth: true; text: root.sbFocusRow !== null ? root.sbFocusRow.recipient : ""; textFormat: Text.PlainText; color: root.ink; font.family: root.mono; font.pixelSize: root.px(16); wrapMode: Text.WrapAnywhere }
                        Text { Layout.fillWidth: true; text: root.sbFocusRow !== null ? root.sbFocusRow.words : ""; color: root.ink; font.family: root.sans; font.pixelSize: root.px(24); font.weight: Font.Bold; wrapMode: Text.WordWrap }
                    }
                    // The bridge's first-time questions and the burn itself, answered here.
                    ApproveBody { visible: root.appInline && root.walletView === "sendback"; inline: true }

                    // Step 3 and on: where the send-back is. The wait is the headline.
                    UnderwayPanel {
                        visible: sendBackScreen.shows && root.sbFocus !== "" && !sendBackScreen.burning && !(root.sbNamesOld && sendBackScreen.p.stage === "planned")
                        StatusLine {
                            kind: Logic.toneKind(sendBackScreen.p.tone)
                            text: sendBackScreen.p.headline
                        }
                        Hint { visible: sendBackScreen.p.detail !== ""; font.pixelSize: root.px(15); text: sendBackScreen.p.detail }
                        // The steps done by hand today, said once, here.
                        Hint { visible: Logic.sendBackByHand(sendBackScreen.p.stage); font.pixelSize: root.px(15); text: Logic.SEND_BACK_BY_HAND }
                        Hint {
                            visible: root.sbFocusRow !== null
                            color: root.ink
                            text: root.sbFocusRow === null ? "" : root.sbFocusRow.amount + " from " + Logic.idShort(root.sbFocusRow.account) + " to " + Logic.idShort(root.sbFocusRow.recipient)
                                + (sendBackScreen.st !== null && sendBackScreen.st.receive_text ? "; you receive " + sendBackScreen.st.receive_text + "." : ".")
                        }
                        Hint { visible: root.sbFocusRow !== null && root.sbFocusRow.words !== ""; font.pixelSize: root.px(15); text: root.sbFocusRow !== null ? "Its words: " + root.sbFocusRow.words : "" }
                        // The burn not proposed yet (or proposed again after a no).
                        Action { visible: sendBackScreen.p.action === "burn" && !root.appInline && root.sbError === ""; label: root.sbAsking ? "Asking your wallet…" : "Approve the burn"; strong: true; enabledNow: !root.sbAsking; onPressed: root.sbBurn() }
                        // Recorded: the release proof on this computer.
                        Action { visible: sendBackScreen.p.action === "prove" && root.sbWitness === null; label: root.sbAsking ? "Starting…" : "Make the proof"; strong: true; enabledNow: !root.sbAsking; onPressed: root.sbProve() }
                        ColumnLayout {
                            visible: root.sbWitness !== null && (sendBackScreen.p.stage === "recorded" || sendBackScreen.p.stage === "proving")
                            Layout.fillWidth: true
                            spacing: 8
                            Hint { color: root.ink; text: "This computer has no prover set up yet: the proof runs from one command (" + Logic.SEND_BACK_TIMES.proof + ")." }
                            Action { label: root.sbCommandCopied ? "Copied." : "Copy the command"; strong: true; onPressed: { root.sbCommandCopied = true; root.againAfter(2500, function() { root.sbCommandCopied = false }); root.copy(root.sbWitness !== null ? (Array.isArray(root.sbWitness.command) ? root.sbWitness.command.join(" ") : String(root.sbWitness.command || "")) : "", "The command") } }
                            TextEdit {
                                Layout.fillWidth: true
                                text: root.sbWitness !== null ? (Array.isArray(root.sbWitness.command) ? root.sbWitness.command.join(" ") : String(root.sbWitness.command || "")) : ""
                                readOnly: true
                                selectByMouse: true
                                wrapMode: TextEdit.WrapAnywhere
                                color: root.ink
                                selectionColor: root.underwayLine
                                font.family: root.mono
                                font.pixelSize: root.px(13)
                            }
                            Hint { font.pixelSize: root.px(15); text: root.sbWitness !== null && root.sbWitness.note ? String(root.sbWitness.note) : "" }
                        }
                        // The proof's Ethereum wrapper, by hand today: the seal the bridge team sends.
                        ColumnLayout {
                            visible: sendBackScreen.p.action === "seal"
                            Layout.fillWidth: true
                            spacing: 8
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 10
                                Field { id: sbSealField; Layout.fillWidth: true; size: 15; code: true; hint: "Paste the seal (0xe71e6620…)"; onTextChanged: root.sbSealInput = text; onAccepted: root.sbSeal() }
                                Action { label: root.sbAsking ? "Checking…" : "Add the seal"; enabledNow: !root.sbAsking && root.sbSealInput.trim() !== ""; onPressed: root.sbSeal() }
                            }
                        }
                        // Releasable: the link to collect it on the bridge page.
                        ColumnLayout {
                            visible: sendBackScreen.p.action === "release"
                            Layout.fillWidth: true
                            spacing: 8
                            Action { label: root.sbLinkCopied ? "Copied." : root.sbAsking ? "Getting the link…" : "Copy the link"; strong: true; enabledNow: !root.sbAsking; onPressed: root.sbCopyRelease() }
                            Hint { font.pixelSize: root.px(15); text: "Open it in your browser and press Release with the Ethereum wallet you connected." }
                        }
                        StatusLine { visible: sendBackScreen.st !== null && !!sendBackScreen.st.note && sendBackScreen.p.detail !== String(sendBackScreen.st.note); kind: Logic.isWaitMessage(sendBackScreen.st !== null ? sendBackScreen.st.note : "") ? "waiting" : ""; text: sendBackScreen.st !== null && sendBackScreen.st.note ? root.sentence(String(sendBackScreen.st.note)) : "" }
                        SendBackCheckLine { visible: sendBackScreen.p.stage !== "released" }
                    }
                    RowLayout {
                        visible: root.sbError !== ""
                        spacing: 12
                        // A refusal of the burn: its one next action (Try again), or, when this Names
                        // can't sign burns at all, the way back.
                        StatusLine {
                            Layout.fillWidth: true
                            kind: Logic.noticeKind(root.sbError, true)
                            text: root.sbError
                            actionLabel: root.sbNamesOld ? "Back to the wallet" : root.sbAsking ? ""
                                : root.sbFocus !== "" && sendBackScreen.p.action === "burn" ? (Logic.approvalEnded(root.sbError) ? "Start again" : "Try again")
                                : root.sbFocus === "" && Logic.approvalEnded(root.sbError) ? "Start again" : ""
                            onAct: if (root.sbNamesOld) root.walletView = "home"; else if (root.sbFocus !== "") root.sbBurn(); else root.sbPlan()
                        }
                    }

                    Link {
                        visible: sendBackScreen.shows && !root.appInline
                        text: root.sbDetails ? "Details ▾" : "Details ▸"
                        color: root.dim
                        font.pixelSize: root.px(15)
                        onPressed: root.sbDetails = !root.sbDetails
                    }
                    ColumnLayout {
                        visible: root.sbDetails && sendBackScreen.shows && !root.appInline
                        Layout.fillWidth: true
                        spacing: 10
                        Hint { font.pixelSize: root.px(15); text: "The wallet burns the wrapped tokens on the bridge test network; the bridge's relay pays the LEZ fee. Ethereum then records the burn, this computer proves it, and the vault releases the ETH to the address you connected, after its one-hour delay." }
                        Hint { font.pixelSize: root.px(15); text: "The burn carries an exit commitment, not the address: the address stays hidden until the release. Your wallet makes the exit's secret from your account's key; this app never sees it." }
                        Hint { font.pixelSize: root.px(15); text: "Text copied in Names may not reach another computer through Screen Sharing: open the link in a browser on this computer, or select it below." }
                        TextEdit {
                            visible: root.sbLink !== null && !!root.sbLink.link && root.sbFocus === ""
                            Layout.fillWidth: true
                            text: root.sbLink !== null ? String(root.sbLink.link || "") : ""
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.WrapAnywhere
                            color: root.ink
                            selectionColor: root.underwayLine
                            font.family: root.mono
                            font.pixelSize: root.px(13)
                        }
                        Hint { visible: root.sbFocusStatus !== null && !!root.sbFocusStatus.privacy; font.pixelSize: root.px(15); text: root.sbFocusStatus !== null && root.sbFocusStatus.privacy ? String(root.sbFocusStatus.privacy) : "" }
                        Fact { visible: root.sbFocusRow !== null; label: "To"; code: true; value: root.sbFocusRow !== null ? root.sbFocusRow.recipient : ""; full: root.sbFocusRow !== null ? root.sbFocusRow.recipient : "" }
                        Fact { visible: root.sbFocusStatus !== null && !!root.sbFocusStatus.exit_commitment; label: "Exit"; code: true; value: root.sbFocusStatus !== null ? String(root.sbFocusStatus.exit_commitment || "") : ""; full: root.sbFocusStatus !== null ? String(root.sbFocusStatus.exit_commitment || "") : "" }
                        Fact { visible: root.sbFocusStatus !== null && !!root.sbFocusStatus.burn_tx; label: "Burn"; code: true; value: root.sbFocusStatus !== null ? String(root.sbFocusStatus.burn_tx || "") : ""; full: root.sbFocusStatus !== null ? String(root.sbFocusStatus.burn_tx || "") : "" }
                        Fact { visible: root.sbFocusStatus !== null && !!root.sbFocusStatus.released_tx; label: "Released"; code: true; value: root.sbFocusStatus !== null ? String(root.sbFocusStatus.released_tx || "") : ""; full: root.sbFocusStatus !== null ? String(root.sbFocusStatus.released_tx || "") : "" }
                        Hint { visible: root.sbFocusStatus !== null && !!root.sbFocusStatus.text; font.pixelSize: root.px(15); text: root.sbFocusStatus !== null && root.sbFocusStatus.text ? "The bridge says: " + String(root.sbFocusStatus.text) : "" }
                        TextEdit {
                            visible: root.sbRelease !== null && root.sbRelease.id === root.sbFocus
                            Layout.fillWidth: true
                            text: root.sbRelease !== null ? String(root.sbRelease.release_link || "") : ""
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.WrapAnywhere
                            color: root.ink
                            selectionColor: root.underwayLine
                            font.family: root.mono
                            font.pixelSize: root.px(13)
                        }
                    }
                }

                // ── Add a token by its address ──
                ColumnLayout {
                    visible: root.walletView === "add"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    PageHeader { title: "Add a token"; net: "0.3"; onBack: root.walletView = "home" }
                    Hint { text: "Paste the token's address, its ID on LEZ. It stays in your wallet from now on, even before you hold any." }
                    Part { text: "Token address" }
                    Field { id: addTokenField; Layout.fillWidth: true; size: 17; code: true; hint: "32 to 44 letters and digits" }
                    ColumnLayout {
                        visible: root.tokenProgram === "" || root.tokenProgramEditing
                        Layout.fillWidth: true
                        spacing: 8
                        Part { text: "Token program" }
                        Hint { font.pixelSize: root.px(15); text: "No token program runs on the public testnet yet. On a chain of your own, enter its program ID." }
                        Field { id: addProgramField; Layout.fillWidth: true; size: 17; code: true; hint: "The token program's ID" }
                    }
                    RowLayout {
                        visible: root.tokenProgram !== "" && !root.tokenProgramEditing
                        spacing: 10
                        Text { text: "Token program"; color: root.dim; font.family: root.sans; font.pixelSize: root.px(15) }
                        Text { text: root.shorten(root.tokenProgram); color: root.ink; font.family: root.mono; font.pixelSize: root.px(15) }
                        Link { text: "Change"; font.pixelSize: root.px(15); onPressed: { addProgramField.text = root.tokenProgram; root.tokenProgramEditing = true } }
                    }
                    Action {
                        label: root.addTokenBusy ? "Checking…" : "Add token"
                        strong: true
                        enabledNow: !root.addTokenBusy && addTokenField.text.trim() !== ""
                        onPressed: { root.tokenProgramEditing = false; root.addToken(addTokenField.text.trim(), addProgramField.text.trim()) }
                    }
                    StatusLine { visible: root.addTokenProblem !== ""; kind: Logic.noticeKind(root.addTokenProblem, true); text: root.addTokenProblem }

                    Part { visible: root.watchedTokens.length > 0; text: "Added by address" }
                    Repeater {
                        model: root.watchedTokens
                        RowLayout {
                            id: watchedRow
                            required property string modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text {
                                text: root.watchedNames[watchedRow.modelData] ? Logic.tokenShown(root.watchedNames[watchedRow.modelData]) : "Token"
                                textFormat: Text.PlainText
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: root.px(17)
                                font.weight: Font.DemiBold
                            }
                            Text { text: root.shorten(watchedRow.modelData); color: root.dim; font.family: root.mono; font.pixelSize: root.px(14) }
                            Item { Layout.fillWidth: true }
                            Link { text: "Remove"; font.pixelSize: root.px(15); onPressed: root.removeToken(watchedRow.modelData) }
                        }
                    }
                }

                // One account's own page.
                ColumnLayout {
                    id: accountPage
                    visible: root.walletView === "account" && root.walletAccount !== null
                    readonly property var row: root.walletAccount
                    readonly property var send: root.sendState(row)
                    readonly property var mine: Logic.activityFor(root.activityRows, root.rowAccount(row), root.zoneId)
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 12
                    BackButton { onPressed: root.walletView = "home" }
                    RunsOn { net: accountPage.row !== null && accountPage.row.kind === "chain" ? "chain" : "0.3" }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.px(14)
                        Avatar {
                            visible: accountPage.row !== null && accountPage.row.name !== undefined && accountPage.row.name !== ""
                            Layout.preferredWidth: root.px(52)
                            Layout.preferredHeight: root.px(52)
                            pixels: visible && root.boardPeople[accountPage.row.name] ? root.boardPeople[accountPage.row.name].pixels : null
                            owner: visible ? accountPage.row.name : ""
                            letter: visible ? accountPage.row.name.charAt(0) : ""
                        }
                        Rectangle {
                            visible: accountPage.row !== null && (accountPage.row.name === undefined || accountPage.row.name === "")
                            implicitWidth: root.px(52)
                            implicitHeight: root.px(52)
                            radius: height / 2
                            color: root.raised
                            border.width: 2
                            border.color: accountPage.row === null ? root.line : accountPage.row.kind === "chain" ? root.orange : accountPage.row.fromPrivate ? root.brass : root.line
                            Text { anchors.centerIn: parent; text: accountPage.row ? accountPage.row.title.charAt(0).toUpperCase() : ""; color: root.ink; font.family: root.sans; font.pixelSize: root.px(20); font.weight: Font.Bold }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text { text: accountPage.row ? accountPage.row.title : ""; color: root.ink; font.family: root.sans; font.pixelSize: root.px(26); font.weight: Font.DemiBold }
                            Text {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                textFormat: Text.StyledText
                                text: accountPage.row ? root.subShown(accountPage.row.sub) : ""
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: root.px(14)
                            }
                        }
                        Text {
                            text: accountPage.row ? root.amountShown(accountPage.row.balance) : ""
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(28)
                            font.weight: Font.DemiBold
                        }
                    }
                    Fact {
                        // Not for an account the node takes at its next start: what reaches it before then isn't seen.
                        visible: accountPage.row !== null && accountPage.row.address !== undefined && accountPage.row.address !== "" && !accountPage.row.later
                        label: "Address"
                        code: true
                        value: accountPage.row ? Logic.keyShown(accountPage.row.address) : ""
                        full: accountPage.row ? accountPage.row.address : ""
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 8
                        Action {
                            visible: accountPage.row !== null
                            label: root.accountsBusy ? "Switching…" : "Send"
                            strong: true
                            enabledNow: accountPage.send.enabled && !root.accountsBusy
                            onPressed: root.sendFrom(accountPage.row)
                        }
                        Action { visible: accountPage.row !== null && !accountPage.row.later; label: "Receive"; onPressed: root.openReceive(accountPage.row) }
                        Action {
                            visible: accountPage.row !== null && (accountPage.row.kind === "chain" || accountPage.row.kind === "lez") && !accountPage.row.later
                            label: "Bridge"
                            onPressed: accountPage.row.kind === "chain" ? root.openGetTokens(accountPage.row.key) : root.openBridgeFromLez(root.rowAccount(accountPage.row))
                        }
                        Action {
                            visible: accountPage.row !== null && accountPage.row.kind === "lez" && !accountPage.row.canSend && !root.cliOutdated
                            label: "Make it the paying account"
                            enabledNow: !root.accountsBusy
                            onPressed: { root.useAccount(accountPage.row.account); root.walletView = "home" }
                        }
                        Action {
                            visible: accountPage.row !== null && accountPage.row.kind === "chain"
                            label: "Staking ›"
                            tint: root.orange
                            strong: true
                            onPressed: { root.walletView = "stake"; root.loadChain() }
                        }
                        Action {
                            visible: accountPage.row !== null && accountPage.row.kind === "token" && root.watchedTokens.indexOf(accountPage.row.definition) >= 0
                            label: "Remove"
                            onPressed: { root.removeToken(accountPage.row.definition); root.walletView = "home" }
                        }
                    }
                    Hint { visible: accountPage.send.reason !== ""; font.pixelSize: root.px(14); text: accountPage.send.reason }
                    Hint { visible: accountPage.row !== null && !!accountPage.row.later; font.pixelSize: root.px(14); text: Logic.chainLaterNote() }
                    Hint { visible: accountPage.send.switchFirst; font.pixelSize: root.px(14); text: "Send makes this the account payments come from, then opens the form." }
                    StatusLine { visible: root.accountsError !== ""; kind: Logic.noticeKind(root.accountsError, true); text: root.accountsError }
                    Part { text: "History" }
                    Hint { visible: accountPage.mine.length === 0; font.pixelSize: root.px(14); text: "Nothing sent or received here yet, as far as this app has seen." }
                    Repeater {
                        model: accountPage.mine
                        ActivityRow { required property var modelData; entry: modelData; account: root.rowAccount(accountPage.row) }
                    }
                }

                // Bridge: one screen for both ways, from whichever account it was opened on.
                ColumnLayout {
                    visible: root.walletView === "bridge"
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.pageWidth
                    spacing: 10
                    PageHeader { title: "Bridge"; net: "both"; onBack: root.walletView = "home" }
                    Hint { font.pixelSize: root.px(15); text: "Moves the native coin between the Logos Blockchain and LEZ. Tokens made on LEZ stay on LEZ: only the native coin crosses." }
                    ColumnLayout {
                        readonly property var bridges: root.activityRows.filter(function(e) { return e.kind === "bridge" || e.kind === "split" })
                        visible: bridges.length > 0
                        Layout.fillWidth: true
                        spacing: 0
                        Part { text: "Your bridges" }
                        Repeater {
                            model: parent.bridges.slice(0, 5)
                            ActivityRow { required property var modelData; entry: modelData; small: true }
                        }
                    }

                    Part { text: "From" }
                    PickRow {
                        title: root.bridgeFromLez !== "" ? (root.bridgeFromLez !== "LEZ" ? "LEZ · " + root.bridgeLezTitle() : "LEZ") : "Logos Blockchain"
                        sub: root.bridgeFromLez !== "" ? "Back to the Logos Blockchain: comes with a later LEZ version" : root.bridgeFrom !== "" ? "Node key · " + Logic.keyShown(root.bridgeFrom) : "No node key yet"
                        amount: root.bridgeFromLez !== "" ? (root.bridgeFromLez !== "LEZ" ? root.bridgeLezAmount() : "") : root.bridgeFromAmount()
                        dropdown: true
                        open: root.bridgeFromOpen
                        onPressed: { root.bridgeFromOpen = !root.bridgeFromOpen; root.bridgeToOpen = false }
                    }
                    ColumnLayout {
                        visible: root.bridgeFromOpen
                        Layout.fillWidth: true
                        Layout.leftMargin: root.px(14)
                        spacing: 4
                        Repeater {
                            model: root.chainUsable || []
                            PickRow {
                                required property var modelData
                                title: "Logos Blockchain"
                                sub: "Node key · " + Logic.keyShown(modelData.key)
                                amount: root.amountShown(modelData.balance)
                                enabledNow: modelData.balance !== "0"
                                compact: true
                                current: root.bridgeFromLez === "" && modelData.key === root.bridgeFrom
                                onPressed: { root.bridgeFromLez = ""; root.bridgeFromKey = modelData.key; root.bridgeFromOpen = false }
                            }
                        }
                        // LEZ as a source is one row: withdrawals aren't on yet, whichever account.
                        PickRow {
                            visible: root.cliPublic !== null
                            title: root.bridgeFromLez !== "" && root.bridgeFromLez !== "LEZ" ? "LEZ · " + root.bridgeLezTitle() : "LEZ"
                            sub: "Back to the Logos Blockchain: comes with a later LEZ version"
                            amount: root.bridgeFromLez !== "" && root.bridgeFromLez !== "LEZ" ? root.bridgeLezAmount() : ""
                            compact: true
                            current: root.bridgeFromLez !== ""
                            onPressed: { if (root.bridgeFromLez === "") root.bridgeFromLez = "LEZ"; root.bridgeFromOpen = false }
                        }
                    }
                    StatusLine {
                        visible: root.chainAccounts === null || root.chainAccounts.length === 0
                        kind: root.chainReading ? "working" : "problem"
                        text: root.chainReading ? "Reading your node…" : "Your node isn't answering. Start it in Basecamp's Blockchain app, then come back."
                    }

                    // LEZ to the Logos Blockchain: the LEZ bridge has withdrawals, switched off for now.
                    Card {
                        visible: root.bridgeFromLez !== ""
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "LEZ → Logos Blockchain: not yet" }
                        Hint {
                            font.pixelSize: root.px(15)
                            text: "LEZ's bridge already has withdrawals, but this LEZ version switches them off, so nothing can leave LEZ yet. This screen will send it once a LEZ update turns them on. To bridge into LEZ, pick a Logos Blockchain key above."
                        }
                    }

                    ColumnLayout {
                        visible: root.bridgeFromLez === ""
                        Layout.fillWidth: true
                        spacing: 10
                        Part { text: "To" }
                        PickRow {
                            title: root.bridgeToPrivate ? "LEZ · Private" : "LEZ · " + root.bridgeToTitle()
                            sub: root.bridgeToPrivate ? "Lands in your main public account, then you move it to private" : "Public · everyone can see it arrive"
                            amount: root.bridgeToPrivate ? (root.cliBalance ? root.amountShown(root.cliBalance.balance) : "") : root.bridgeToAmount()
                            dropdown: true
                            open: root.bridgeToOpen
                            onPressed: { root.bridgeToOpen = !root.bridgeToOpen; root.bridgeFromOpen = false }
                        }
                        ColumnLayout {
                            visible: root.bridgeToOpen
                            Layout.fillWidth: true
                            Layout.leftMargin: root.px(14)
                            spacing: 4
                            Repeater {
                                model: root.bridgeTargets
                                PickRow {
                                    required property var modelData
                                    required property int index
                                    title: "LEZ · " + modelData.title
                                    sub: "Public · everyone can see it arrive"
                                    amount: root.amountShown(modelData.balance)
                                    compact: true
                                    current: !root.bridgeToPrivate && (root.bridgeTo === modelData.account || (root.bridgeTo === "" && index === 0))
                                    onPressed: { root.bridgeToPrivate = false; root.bridgeTo = index === 0 ? "" : modelData.account; root.bridgeToOpen = false }
                                }
                            }
                            PickRow {
                                visible: root.cliBalance !== undefined && root.cliBalance !== null && root.cliPublic !== null
                                title: "LEZ · Private"
                                sub: "Lands in your main public account, then you move it to private"
                                amount: root.cliBalance ? root.amountShown(root.cliBalance.balance) : ""
                                compact: true
                                current: root.bridgeToPrivate
                                onPressed: { root.bridgeToPrivate = true; root.bridgeToOpen = false }
                            }
                            PickRow {
                                visible: root.cliPublic !== null && !root.cliOld
                                title: root.accountsBusy ? "Making a new account…" : "＋ New public account"
                                sub: "Made in your wallet now, then picked"
                                compact: true
                                enabledNow: !root.accountsBusy
                                onPressed: { root.bridgeToNewAccount(); root.bridgeToOpen = false }
                            }
                        }
                        StatusLine { visible: root.accountsError !== ""; kind: Logic.noticeKind(root.accountsError, true); text: root.accountsError }

                        Part { text: "Amount" }
                        Field {
                            id: bridgeAmountField
                            Layout.fillWidth: true
                            Layout.maximumWidth: 320
                            size: 17
                            numbersOnly: true
                            hint: root.bridgeToPrivate ? "Such as " + root.shieldMinimum : "Such as 5000000"
                            onAccepted: root.bridgeRun(false)
                        }
                        // Why Review stays off: "1,5" is no amount, never 15.
                        StatusLine {
                            // Once Review said it below, it isn't said twice.
                            visible: bridgeAmountField.text.trim() !== "" && root.bridgeAsked()[0] === "" && root.bridgeError !== text
                            kind: "problem"
                            text: Logic.amountProblem(bridgeAmountField.text, Logic.CHAIN_AMOUNT_MAX)
                        }
                        // The node address: kept for the module call, no longer shown (one node: Basecamp's).
                        Field { id: bridgeNodeField; visible: false }
                        Hint {
                            visible: root.bridgeToPrivate && root.bridgeAmountShort()
                            color: root.ink
                            font.pixelSize: root.px(14)
                            text: "Moving it to private costs a private fee of about 5,100,000 taken from what moves, so " + root.grouped(root.bridgeAsked()[0]) + " would leave next to nothing. Bridge at least " + root.grouped(root.shieldMinimum) + "."
                        }

                        RowLayout {
                            Layout.topMargin: 2
                            spacing: 8
                            Action {
                                visible: !root.bridgeConfirm
                                label: root.bridgeBusy ? "Checking…" : "Review"
                                strong: true
                                enabledNow: !root.bridgeBusy && root.cliPublic !== null && root.bridgeFrom !== "" && /^[0-9]+$/.test(root.bridgeAsked()[0])
                                onPressed: { root.bridgeReview = true; root.bridgeRun(false) }
                            }
                        }
                        Hint {
                            visible: root.bridgePlan !== null
                            font.pixelSize: root.px(15)
                            color: root.bridgePlanFor === root.bridgeAsked().join("|") ? root.ink : root.faint
                            text: root.bridgePlanFor === root.bridgeAsked().join("|") ? root.bridgePlanText() : "You changed something. Check again before you bridge."
                        }
                        Action {
                            visible: root.bridgePlan !== null && root.bridgePlan.needs_split && root.bridgePlanFor === root.bridgeAsked().join("|") && root.bridgeSplitHash === ""
                            label: root.bridgeBusy ? "Making it…" : "Make a note of exactly " + root.grouped(root.bridgeAsked()[0])
                            strong: true
                            tint: root.orange
                            enabledNow: !root.bridgeBusy
                            onPressed: root.bridgeSplit()
                        }
                        Hint {
                            visible: root.bridgeSplitHash !== ""
                            color: root.waitTone
                            font.pixelSize: root.px(15)
                            text: "Making the note: waiting for your node's next blocks. The bridge is checked again as soon as it lands."
                        }
                        Card {
                            visible: root.bridgeConfirm && root.bridgePlan !== null
                            Layout.fillWidth: true
                            Hint {
                                color: root.ink
                                text: root.bridgePlan === null ? "" : "This moves exactly " + root.grouped(root.bridgeAsked()[0]) + " from your Logos Blockchain key into " + (root.zonesMany && root.bridgePlan.zone_name ? "the zone " + root.bridgePlan.zone_name : "LEZ")
                                    + (root.bridgeToPrivate ? " (your main public account, then you move it to private)" : "")
                                    + ", through its deposit channel " + Logic.keyShown(root.bridgePlan.channel || "") + " (your node knows it). It can't be undone. It arrives in about 12 hours."
                            }
                            RowLayout {
                                spacing: 8
                                Action {
                                    label: root.bridgeBusy ? "Sending…" : "Yes, bridge it"
                                    strong: true
                                    enabledNow: !root.bridgeBusy
                                    onPressed: root.bridgeRun(true)
                                }
                                Action {
                                    label: "Cancel"
                                    enabledNow: !root.bridgeBusy
                                    onPressed: root.bridgeConfirm = false
                                }
                            }
                        }
                        StatusLine { visible: root.bridgeError !== ""; kind: Logic.noticeKind(root.bridgeError, true); text: root.bridgeError }
                        Card {
                            visible: root.bridgeSent
                            Layout.fillWidth: true
                            Hint { color: root.good; font.weight: Font.DemiBold; text: "Sent. It arrives in your public account in about 12 hours." + (root.bridgeToPrivate ? " Then move it to private below." : "") }
                            Fact { visible: root.bridgeHash !== ""; label: "Deposit"; code: true; value: root.bridgeHash; full: root.bridgeHash }
                            Text {
                                visible: root.bridgeHash === ""
                                Layout.fillWidth: true
                                textFormat: Text.PlainText
                                text: root.bridgeSentLines.join("\n")
                                color: root.dim
                                font.family: root.mono
                                font.pixelSize: root.px(13)
                                wrapMode: Text.WrapAnywhere
                            }
                        }
                    }

                    // No wallet yet: where bridged tokens land has to exist first.
                    Card {
                        visible: root.cliPublic === null
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Your LEZ wallet" }
                        Hint {
                            text: root.cliPath === "" ? "Install the names program first (see Welcome)."
                                : root.cliBalance === undefined ? "Checking your accounts…"
                                : root.hasCliWallet ? "Couldn't read your accounts just now."
                                : "You don't have a wallet yet. Create one first: it's where bridged tokens land."
                            color: root.cliBalance === undefined ? root.dim : root.ink
                        }
                        Action {
                            visible: root.cliPath !== "" && root.cliBalance !== undefined && !root.hasCliWallet
                            label: root.walletBusy ? "Creating…" : "Create my wallet"
                            strong: true
                            enabledNow: !root.walletBusy
                            onPressed: root.createWallet()
                        }
                        Link {
                            visible: root.cliPath !== "" && root.cliBalance !== undefined && root.hasCliWallet
                            text: "Check again"
                            font.pixelSize: root.px(15)
                            onPressed: root.loadCliBalance()
                        }
                        StatusLine { visible: root.walletError !== ""; kind: Logic.noticeKind(root.walletError, true); text: root.walletError }
                    }

                    // To private: exactly the amount typed, from the main public account.
                    Card {
                        visible: root.bridgeFromLez === "" && root.cliPublic !== null && (root.bridgeToPrivate || root.shieldJob !== null || root.shieldMoved)
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Move to private" }
                        Hint {
                            font.pixelSize: root.px(14)
                            text: "From your main public account, which holds " + root.amountShown(root.cliPublic !== null ? root.cliPublic.balance : "0") + ". Only what you type moves."
                        }
                        RowLayout {
                            spacing: 8
                            Field {
                                id: privateMoveField
                                Layout.preferredWidth: 220
                                size: 17
                                numbersOnly: true
                                hint: root.bridgeAsked()[0] !== "" ? root.bridgeAsked()[0] : root.shieldMinimum
                            }
                            Action {
                                label: root.shieldJob !== null ? "Moving…" : "Move to private"
                                enabledNow: root.shieldJob === null && /^[0-9]+$/.test(root.privateMoveAmount()) && Logic.atLeast(root.cliPublic !== null ? root.cliPublic.balance : "0", root.privateMoveAmount())
                                onPressed: root.moveToPrivate(root.privateMoveAmount())
                            }
                        }
                        StatusLine {
                            visible: privateMoveField.text.trim() !== "" && root.privateMoveAmount() === ""
                            kind: "problem"
                            text: Logic.amountProblem(privateMoveField.text)
                        }
                        Hint { visible: root.shieldJob !== null; color: root.waitTone; font.pixelSize: root.px(15); text: root.shieldLine() }
                        Hint { visible: root.shieldMoved && root.shieldJob === null; color: root.good; font.pixelSize: root.px(15); text: "Moved. Your private account is ready for names." }
                        StatusLine { visible: root.shieldProblem !== "" && root.shieldJob === null; kind: Logic.noticeKind(root.shieldProblem, true); text: root.shieldProblem }
                    }
                    // Gas and minimums, at the very bottom.
                    Rectangle {
                        visible: root.bridgeFromLez === ""
                        Layout.topMargin: root.px(6)
                        Layout.fillWidth: true
                        implicitHeight: feeNotes.implicitHeight + root.px(20)
                        radius: 8
                        color: "transparent"
                        border.width: 1
                        border.color: root.line
                        ColumnLayout {
                            id: feeNotes
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.leftMargin: root.px(14)
                            anchors.rightMargin: root.px(14)
                            spacing: 4
                            Text { text: "Gas and minimums"; color: root.ink; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: root.px(15) }
                            Hint { font.pixelSize: root.px(14); text: "• Logos Blockchain fee: at most 100,000, paid from the key's other notes, not from the amount." }
                            Hint { font.pixelSize: root.px(14); text: "• The amount goes as one whole note. If no note holds exactly that, a split makes one first: a send from the key to itself, a fee of about 2,000, one block." }
                            Hint { font.pixelSize: root.px(13); text: "• It arrives once the Logos Blockchain block is final: expected about 12 hours on the testnet (the first deposits are still arriving)." }
                            Hint {
                                visible: root.bridgeToPrivate
                                font.pixelSize: root.px(14)
                                text: root.cliOutdated ? "• Then moving it to private is proven on this computer, usually 5–12 minutes."
                                    : "• Then moving it to private is a private transaction: a fee of about 5,100,000, taken from what moves, and 5–12 minutes of proving. To register a name privately you need " + root.grouped(root.shieldMinimum) + " there."
                            }
                        }
                    }
                }
            }
        }

        // Posts: the board.
        Flickable {
            visible: root.section === "posts" && !root.needsWelcome
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: home.implicitHeight + 72
            clip: true

            ColumnLayout {
                id: home
                x: 56
                y: 40
                width: Math.min(parent.width - 112, 760)
                spacing: 16

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    BackButton {
                        visible: root.boardView === "thread"
                        label: "Posts"
                        onPressed: root.boardShow("all")
                    }
                    Tab { visible: root.boardView !== "thread"; label: "Recent"; current: root.boardView === "all"; onPressed: root.boardShow("all") }
                    Tab { visible: root.boardView !== "thread"; label: "Following" + (root.boardFollows.length > 0 ? "  " + root.boardFollows.length : ""); current: root.boardView === "following"; onPressed: root.boardShow("following") }
                    Text { visible: root.boardView === "thread"; text: "Thread"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    Link { text: root.boardLoading ? "Loading…" : "Refresh"; color: root.faint; font.pixelSize: root.px(15); onPressed: root.boardLoad() }
                }

                // The post box.
                Card {
                    Layout.fillWidth: true
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Text {
                            text: root.boardAs !== "" ? "Posting as " + root.boardAs : "No name to post as yet"
                            color: root.anonOn && root.anonReady ? root.brass : root.dim
                            font.family: root.sans
                            font.pixelSize: root.px(15)
                            font.weight: Font.DemiBold
                        }
                        Item { Layout.fillWidth: true }
                        Chip {
                            label: root.anonOn ? "Anon: on" : "Anon: off"
                            link: true
                            onPressed: root.anonOn = !root.anonOn
                        }
                    }
                    Rectangle {
                        visible: root.boardAsMoving
                        Layout.fillWidth: true
                        implicitHeight: movingNote.implicitHeight + 20
                        radius: 8
                        color: root.noticeFill
                        Text {
                            id: movingNote
                            x: 14
                            y: 10
                            width: parent.width - 28
                            text: root.boardAs + " is being made public: it moves to its own public account, then gets its profile and page back. You can post as soon as that's done, about "
                                + Math.max(1, 10 - Math.floor((Math.max(root.clockTick, Date.now()) - (root.boardJob ? root.boardJob.started : Date.now())) / 60000)) + " min."
                            color: root.waitTone
                            font.family: root.sans
                            font.pixelSize: root.px(15)
                            wrapMode: Text.Wrap
                        }
                    }
                    // How posting as this name works, in one line, before anyone presses Post.
                    Text {
                        visible: root.boardAs !== "" && !(root.anonOn && !root.anonReady) && !root.boardAsMoving
                        Layout.fillWidth: true
                        text: root.heldPublicly(root.boardAs) ? "Public · about 30 seconds per post, a little gas"
                            : "Private · usually 5–12 minutes per post, free; the chain never learns your account"
                        color: root.heldPublicly(root.boardAs) ? root.faint : root.dim
                        font.family: root.sans
                        font.pixelSize: root.px(14)
                        wrapMode: Text.Wrap
                    }
                    // The anon name, before it can post.
                    ColumnLayout {
                        visible: root.anonOn && !root.anonReady
                        Layout.fillWidth: true
                        spacing: 8
                        Hint {
                            font.pixelSize: root.px(15)
                            text: root.anonName !== "" && root.cliPendingFor(root.anonName) !== null
                                ? root.anonName + " is being set up: " + root.pendingLine(root.cliPendingFor(root.anonName)) + "."
                                : "Anon posts come from a separate name, like anon-quiet-otter, held by its own private account. Nothing on chain links it to "
                                  + (root.mainName !== "" ? root.mainName : "your name") + "; only your wallet knows both. Setting it up takes about 25 minutes, once, and a 1,000,000 deposit you get back if you release it."
                        }
                        RowLayout {
                            visible: root.anonName === "" || root.cliPendingFor(root.anonName) === null
                            spacing: 6
                            // LEZ 0.4 is public only: no private anon there (the module refuses it too).
                            Tab { visible: root.namesNet !== "0.4"; label: "Private anon (4 min posts)"; current: !root.anonPublic; onPressed: root.anonPublic = false }
                            Tab { label: "Public anon (30 s posts)"; current: root.anonPublic || root.namesNet === "0.4"; onPressed: root.anonPublic = true }
                        }
                        Action {
                            visible: root.anonName === "" || root.cliPendingFor(root.anonName) === null
                            label: "Set up my anon name"
                            strong: true
                            enabledNow: root.boardJob === null && root.mainName !== ""
                            onPressed: root.makeAnon()
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: root.boardReplyTo !== ""
                        Text { Layout.fillWidth: true; text: "Replying to " + root.boardPreviewOf(root.boardReplyTo); color: root.ink; font.family: root.sans; font.pixelSize: root.px(15); elide: Text.ElideRight }
                        Link { text: "Cancel"; color: root.faint; font.pixelSize: root.px(14); onPressed: root.boardReplyTo = "" }
                    }
                    Area {
                        id: boardComposer
                        Layout.fillWidth: true
                        implicitHeight: 96
                        code: false
                        onTextChanged: spellTimer.restart()
                        hint: root.boardAs !== "" ? "What's happening, " + root.boardAs + "?" : "Register a name to post"
                    }
                    Timer { id: spellTimer; interval: 600; onTriggered: root.boardSpellCheck() }
                    Flow {
                        visible: root.boardSpelling.length > 0
                        Layout.fillWidth: true
                        spacing: 8
                        Text { text: "Spelling:"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14); height: 34; verticalAlignment: Text.AlignVCenter }
                        Repeater {
                            model: root.boardSpelling
                            Row {
                                required property var modelData
                                spacing: 4
                                Text {
                                    text: modelData.word + (modelData.suggestions.length > 0 ? " →" : " (no suggestion)")
                                    color: root.ink
                                    font.family: root.sans
                                    font.pixelSize: root.px(14)
                                    height: 34
                                    verticalAlignment: Text.AlignVCenter
                                }
                                Repeater {
                                    model: modelData.suggestions
                                    Chip {
                                        required property string modelData
                                        label: modelData
                                        link: true
                                        onPressed: root.boardFixWord(parent.modelData.word, modelData)
                                    }
                                }
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Link {
                            visible: !root.boardAttach && boardCid.text === ""
                            text: "Attach a file"
                            color: root.faint
                            font.pixelSize: root.px(14)
                            onPressed: root.boardAttach = true
                        }
                        Field {
                            id: boardCid
                            visible: root.boardAttach || text !== ""
                            Layout.fillWidth: true
                            size: 15
                            code: true
                            hint: "Its Logos Storage CID"
                        }
                        Item { visible: !boardCid.visible; Layout.fillWidth: true }
                        Text {
                            text: boardComposer.text.length + " / 1024"
                            color: boardComposer.text.length > 1024 ? root.alarm : root.faint
                            font.family: root.sans
                            font.pixelSize: root.px(13)
                        }
                        Action {
                            label: root.boardReplyTo !== "" ? "Reply" : "Post"
                            strong: true
                            enabledNow: root.boardJob === null && !root.boardAsMoving && root.boardAs !== "" && !(root.anonOn && !root.anonReady) && boardComposer.text.length <= 1024 && (boardComposer.text.trim() !== "" || boardCid.text.trim() !== "")
                            onPressed: root.boardPost()
                        }
                    }
                }

                Hint {
                    visible: root.boardJob !== null
                    color: root.waitTone
                    font.pixelSize: root.px(16)
                    text: root.boardJob === null ? "" : root.boardJobLine(root.boardJob)
                }
                StatusLine {
                    visible: root.boardJob === null && root.notice !== "" && root.section === "posts"
                    kind: root.noticeKind
                    text: root.notice
                }
                Hint {
                    visible: !root.boardLoading && root.boardPosts.length === 0
                    font.pixelSize: root.px(17)
                    text: root.cliPath === "" ? "The board needs the names CLI. Install it, then open Advanced to point at it."
                        : root.boardError !== "" ? "Waiting for the board: " + root.boardError
                        : root.boardView === "following" && root.boardFollows.length === 0 ? "You don't follow anyone yet. Press Follow on a post."
                        : "No posts in the last few hours."
                }

                Repeater {
                    model: root.boardTree
                    Card {
                        id: postCard
                        required property var modelData
                        readonly property var p: modelData.post
                        Layout.leftMargin: modelData.depth * 40
                        readonly property var who: root.boardPeople[modelData.post.name] || ({ display: "", color: "", pixels: null })
                        readonly property bool isMine: root.cliNames.some(function(entry) { return entry.name === postCard.p.name })
                        visible: !modelData.post.hidden
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 14
                            Avatar {
                                Layout.preferredWidth: 44
                                Layout.preferredHeight: 44
                                Layout.alignment: Qt.AlignTop
                                pixels: postCard.who.pixels
                                owner: postCard.p.name
                                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.show(postCard.p.name) }
                                letter: postCard.p.name.charAt(0)
                                ring: postCard.who.color !== "" ? postCard.who.color : root.brass
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Text {
                                        visible: postCard.who.display !== ""
                                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.show(postCard.p.name) }
                                        text: postCard.who.display
                                        textFormat: Text.PlainText
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: root.px(17)
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        text: postCard.p.name
                                        color: postCard.who.display !== "" ? root.faint : root.ink
                                        font.family: root.sans
                                        font.pixelSize: postCard.who.display !== "" ? 15 : 17
                                        font.weight: postCard.who.display !== "" ? Font.Normal : Font.DemiBold
                                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.show(postCard.p.name) }
                                    }
                                    Text { text: "· " + root.boardAgo(postCard.p.time); color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                                    Item { Layout.fillWidth: true }
                                    Link {
                                        visible: !postCard.isMine
                                        text: root.boardFollowing(postCard.p.name) ? "Following" : "Follow"
                                        color: root.boardFollowing(postCard.p.name) ? root.faint : root.brass
                                        font.pixelSize: root.px(14)
                                        onPressed: root.boardFollow(postCard.p.name, root.boardFollowing(postCard.p.name))
                                    }
                                }
                                Text {
                                    visible: !!postCard.p.parent && postCard.modelData.depth === 0
                                    text: "replying to " + (postCard.p.parent ? String(postCard.p.parent).split("/")[0] : "")
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: root.px(13)
                                }
                                Text {
                                    Layout.fillWidth: true
                                    visible: postCard.p.text !== ""
                                    text: postCard.p.text
                                    textFormat: Text.PlainText
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(17)
                                    lineHeight: 1.2
                                    wrapMode: Text.Wrap
                                }
                                Flow {
                                    Layout.fillWidth: true
                                    visible: postCard.p.cids.length > 0
                                    spacing: 8
                                    Repeater {
                                        model: postCard.p.cids
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
                                    Link { text: "Reply"; color: root.faint; font.pixelSize: root.px(14); onPressed: root.boardReply(postCard.p.id) }
                                    Link {
                                        readonly property bool liked: root.boardLiked.indexOf(postCard.p.id) >= 0
                                        text: (liked ? "♥ " : "♡ ") + Math.max(liked ? 1 : 0, postCard.p.likes)
                                        color: liked ? root.brass : root.faint
                                        font.pixelSize: root.px(14)
                                        font.underline: false
                                        onPressed: root.boardToggleLike(postCard.p.id)
                                    }
                                    Link {
                                        visible: root.boardView !== "thread"
                                        text: "Thread"
                                        color: root.faint
                                        font.pixelSize: root.px(14)
                                        onPressed: root.boardShow("thread", root.boardRootOf(postCard.p.id))
                                    }
                                    Link {
                                        visible: postCard.isMine
                                        text: "Take down"
                                        color: root.faint
                                        font.pixelSize: root.px(14)
                                        onPressed: root.boardStart("board_hide", [postCard.p.id], "Taking it down", null, postCard.p.name)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // The plate: search, profile and settings.
        Flickable {
            visible: (root.section === "search" || root.section === "profile" || root.section === "settings") && !root.welcomeCovers
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: plate.implicitHeight + 72
            clip: true

            ColumnLayout {
                id: plate
                x: 56
                y: 40
                width: Math.min(parent.width - 112, root.tool === "settings" ? 1040 : 820)
                spacing: 22

                // Which network names are looked up on: always reachable on Search, a result shown
                // or not (switching clears the result).
                NetSwitch { visible: root.section === "search" }
                RunsOn { visible: root.section === "search" || root.section === "profile"; prefix: "LNS names and profiles: looked up on" }

                // Search: look up any name.
                ColumnLayout {
                    visible: root.section === "search"
                    Layout.fillWidth: true
                    spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Field {
                        id: searchField
                        Layout.fillWidth: true
                        hint: "Look up a name, such as jef"
                        onAccepted: root.find(searchField.text)
                    }
                    IconButton {
                        kind: "search"
                        boxed: true
                        onPressed: root.find(searchField.text)
                    }
                }
                }

                // Settings: your names, registering another, and Advanced.
                ColumnLayout {
                    visible: root.section === "settings" && !root.registering && !root.gettingTokens
                    Layout.fillWidth: true
                    spacing: 14
                    Text { text: "Settings"; color: root.ink; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: root.px(28) }
                    // What this build brings, three lines, after an update (until "Got it").
                    ColumnLayout {
                        visible: root.whatsNewShown
                        Layout.fillWidth: true
                        spacing: 6
                        Part { text: "What's new in Names 0.17.0" + (root.buildCommit !== "" ? " (" + root.buildCommit + ")" : "") }
                        Repeater {
                            model: Logic.WHATS_NEW
                            Hint { required property var modelData; color: root.ink; text: "· " + modelData }
                        }
                        Link { text: "Got it"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.whatsNewDone() }
                    }
                    Part { text: "Main name" }
                    Hint { text: "Payments, messages and your profile are made as this name." }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        Repeater {
                            model: root.boardNames.filter(function(entry) { return entry.name !== root.anonName })
                            Tab {
                                required property var modelData
                                label: modelData.name
                                current: root.mainName === modelData.name
                                onPressed: root.setMain(modelData.name)
                            }
                        }
                    }
                    Hint { visible: root.anonName !== ""; text: "Anon name: " + root.anonName + (root.anonReady ? "" : " (being set up)") + ". Only your wallet links it to your main name." }
                    Part { text: "Your names" }
                    Hint { font.pixelSize: root.px(14); text: "Private: the holder stays hidden, and each change is proved on this computer (usually 5–12 min). Public: about 30 s and a little gas, the holder visible. Payments to the name stay private either way." }
                    Repeater {
                        model: root.cliPending
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { Layout.preferredWidth: 200; text: modelData.name; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); elide: Text.ElideRight }
                            Text { Layout.fillWidth: true; text: root.pendingLine(modelData); color: modelData.failed ? root.alarm : root.waitTone; font.family: root.sans; font.pixelSize: root.px(14); elide: Text.ElideRight }
                        }
                    }
                    Repeater {
                        model: root.allMine
                        ColumnLayout {
                            id: nameRow
                            required property var modelData
                            readonly property bool privateCli: modelData.signer === "cli" && modelData["private"] !== false
                            readonly property bool isAnon: modelData.name === root.anonName
                            Layout.fillWidth: true
                            spacing: 4
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 44
                                spacing: 12
                                Text {
                                    Layout.preferredWidth: 200
                                    text: nameRow.modelData.name
                                    color: root.ink
                                    font.family: root.sans
                                    font.pixelSize: root.px(17)
                                    elide: Text.ElideRight
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.show(nameRow.modelData.name) }
                                }
                                Text {
                                    text: (root.boardJob !== null && root.boardJob.kind === "make_public" && root.boardJob.name === nameRow.modelData.name ? "Becoming public…"
                                        : nameRow.privateCli ? "Private" : "Public") + (nameRow.isAnon ? " · anon" : nameRow.modelData.name === root.mainName ? " · main" : "")
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: root.px(15)
                                }
                                Item { Layout.fillWidth: true }
                                Action {
                                    visible: nameRow.privateCli && root.publicAsk !== nameRow.modelData.name
                                    label: "Make public"
                                    enabledNow: root.boardJob === null
                                    onPressed: root.publicAsk = nameRow.modelData.name
                                }
                                Action {
                                    visible: root.publicAsk === nameRow.modelData.name
                                    label: "Make it public"
                                    strong: true
                                    onPressed: {
                                        var name = nameRow.modelData.name
                                        root.publicAsk = ""
                                        root.boardStart("make_public", [name], "Making " + name + " public", function() { root.loadCliMine() }, name)
                                    }
                                }
                                Action {
                                    visible: root.publicAsk === nameRow.modelData.name
                                    label: "Cancel"
                                    onPressed: root.publicAsk = ""
                                }
                            }
                            Hint {
                                visible: root.publicAsk === nameRow.modelData.name
                                color: root.ink
                                font.pixelSize: root.px(14)
                                text: "About 9 minutes. It moves the name to a new public account and funds that with 10,000,000 for gas from your private account, so the chain can't see where the money came from."
                                    + (nameRow.isAnon ? " Keep the anon private in practice: never send between its account and your other public accounts, and don't make it public right after your main name." : "")
                            }
                        }
                    }
                    Hint {
                        visible: root.boardJob !== null && root.section === "settings"
                        color: root.waitTone
                        text: root.boardJob === null ? "" : root.boardJob.label + ": about 9 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                    }
                    Action {
                        Layout.topMargin: 6
                        label: "+  Register another name"
                        strong: true
                        onPressed: root.openRegister()
                    }
                    Part { text: "Accounts" }
                    // Which network this list is: never one network's accounts read as the other's.
                    Hint { color: root.ink; font.pixelSize: root.px(15); text: "On " + (root.namesNet === "0.4" ? "LEZ 0.4 (the bridge test network)" : "LEZ 0.3 (the public Logos testnet)") + ": the accounts of that network's wallet. Switch networks at the top of the Wallet." }
                    Hint { visible: !root.cliOld || root.cliAccounts !== null; text: "Your wallet's accounts. The one marked Pays is where payments and new names come from, one private and one public. Names you hold stay with the accounts holding them, and payments to a name still go where the name points." }
                    Hint { visible: root.cliPath === ""; text: "Install Names on this computer first (Welcome) to see your accounts." }
                    Hint { visible: root.cliPath !== "" && root.cliAccounts === null && root.accountsError === ""; text: "Reading your accounts…" }
                    Repeater {
                        model: (root.cliAccounts || []).filter(function(entry) { return root.walletShowEmpty || entry.balance !== "0" || entry["default"] || (entry.npk && root.namesByNpk[entry.npk]) })
                        ColumnLayout {
                            id: accountRow
                            required property var modelData
                            readonly property bool chosen: accountRow.modelData["default"] === true
                            readonly property bool isPrivate: accountRow.modelData.kind === "private"
                            readonly property bool empty: accountRow.modelData.balance === "0"
                            Layout.fillWidth: true
                            spacing: 4
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 40
                                spacing: 12
                                Text { Layout.preferredWidth: 170; text: accountRow.modelData.npk && root.namesByNpk[accountRow.modelData.npk] ? root.namesByNpk[accountRow.modelData.npk].join(", ") : Logic.accountTitle(accountRow.modelData); color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); elide: Text.ElideRight }
                                Text {
                                    text: root.accountShort(accountRow.modelData.account)
                                    color: root.dim
                                    font.family: root.mono
                                    font.pixelSize: root.px(14)
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.copy(accountRow.modelData.account, "Account") }
                                }
                                Text {
                                    text: accountRow.modelData.balance === null ? "?" : root.grouped(accountRow.modelData.balance)
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: root.px(15)
                                }
                                Text { visible: accountRow.chosen; text: "Pays"; color: root.brass; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: root.px(15) }
                                Item { Layout.fillWidth: true }
                                // The full ID, as `Public/<id>` (shown shortened beside it).
                                Link { text: "Copy"; color: root.dim; font.pixelSize: root.px(14); onPressed: root.copy(accountRow.modelData.account, "The full account ID") }
                                Action {
                                    // Choosing and naming accounts need the names program's 0.4 update.
                                    visible: !accountRow.chosen && !root.cliOutdated
                                    label: "Pay from this"
                                    enabledNow: !root.accountsBusy
                                    onPressed: root.useAccount(accountRow.modelData.account)
                                }
                                // Accounts carry registered names only: no nicknames to set.
                                Action {
                                    visible: false
                                    label: "Rename"
                                    enabledNow: !root.accountsBusy
                                    onPressed: {
                                        root.accountRenaming = accountRow.modelData.account
                                        renameField.text = accountRow.modelData.label || ""
                                    }
                                }
                            }
                            RowLayout {
                                visible: root.accountRenaming === accountRow.modelData.account
                                Layout.fillWidth: true
                                spacing: 8
                                Field { id: renameField; Layout.fillWidth: true; size: 16; hint: "A nickname, such as Savings (empty: none)"; onAccepted: root.nameAccount(accountRow.modelData.account, renameField.text) }
                                Action { label: "Save"; strong: true; enabledNow: !root.accountsBusy; onPressed: root.nameAccount(accountRow.modelData.account, renameField.text) }
                                Action { label: "Cancel"; onPressed: root.accountRenaming = "" }
                            }
                            // LEZ 0.4 credits a public account the chain hasn't seen only to a sender who has its key.
                            RowLayout {
                                visible: !accountRow.isPrivate && accountRow.empty && !!accountRow.modelData.public_key
                                Layout.fillWidth: true
                                spacing: 8
                                Hint { Layout.fillWidth: true; font.pixelSize: root.px(14); text: "Not used on the chain yet: to pay it, a sender needs its key as well as its address." }
                                Link { text: "Copy key"; font.pixelSize: root.px(14); onPressed: root.copy(accountRow.modelData.public_key || "", "Key") }
                            }
                            Hint {
                                visible: accountRow.chosen && accountRow.empty
                                color: root.ink
                                font.pixelSize: root.px(14)
                                text: accountRow.isPrivate ? "This account is empty, so payments and new names can't come from it until tokens are moved in. Pay into it from another account, or choose a funded one."
                                    : "This account is empty, so it can't pay fees. Get tokens into it, or choose a funded one."
                            }
                        }
                    }
                    Link {
                        readonly property int empties: (root.cliAccounts || []).filter(function(entry) { return entry.balance === "0" && !entry["default"] && !(entry.npk && root.namesByNpk[entry.npk]) }).length
                        visible: empties > 0
                        text: root.walletShowEmpty ? "Hide empty accounts" : "Show " + empties + (empties === 1 ? " empty account" : " empty accounts")
                        color: root.dim
                        font.pixelSize: root.px(15)
                        onPressed: root.walletShowEmpty = !root.walletShowEmpty
                    }
                    StatusLine { visible: root.accountsError !== "" && !root.cliOld; kind: Logic.noticeKind(root.accountsError, true); text: root.accountsError }
                    Hint { visible: root.accountsNote !== "" && root.accountsError === ""; color: root.ink; text: root.accountsNote }
                    // An older names program: the two accounts it pays from, and why there's no more.
                    Fact {
                        visible: root.cliOld && root.cliAccounts === null && root.cliBalance !== undefined && root.cliBalance !== null
                        label: "Private"
                        code: true
                        value: root.cliBalance ? root.shorten(root.cliBalance.account || "") + "   " + root.grouped(root.cliBalance.balance) : ""
                        full: root.cliBalance && root.cliBalance.account ? "Private/" + root.cliBalance.account : ""
                    }
                    Fact {
                        visible: root.cliOld && root.cliAccounts === null && root.cliPublic !== null
                        label: "Public"
                        code: true
                        value: root.cliPublic !== null ? root.shorten(root.cliPublic.account) + "   " + root.grouped(root.cliPublic.balance) : ""
                        full: root.cliPublic !== null ? "Public/" + root.cliPublic.account : ""
                    }
                    Hint {
                        visible: root.cliOld
                        font.pixelSize: root.px(15)
                        text: root.cliAccounts === null ? "These are the two accounts the names tool for " + (root.namesNet === "0.4" ? "LEZ 0.4" : "LEZ 0.3") + " pays from. That tool is too old to show every account, or to make or name more."
                            : "The names tool for " + (root.namesNet === "0.4" ? "LEZ 0.4" : "LEZ 0.3") + " is too old to make, name or choose accounts."
                    }
                    RowLayout {
                        visible: root.accountNewKind === "" && root.cliPath !== "" && !root.cliOld
                        spacing: 10
                        Action { label: "+  New private account"; enabledNow: !root.accountsBusy; onPressed: { root.accountNewKind = "private"; newAccountField.text = "" } }
                        Action { label: "+  New public account"; enabledNow: !root.accountsBusy; onPressed: { root.accountNewKind = "public"; newAccountField.text = "" } }
                    }
                    ColumnLayout {
                        visible: root.accountNewKind !== ""
                        Layout.fillWidth: true
                        spacing: 6
                        Hint {
                            font.pixelSize: root.px(15)
                            text: root.accountNewKind === "private"
                                ? "A new private account: only your wallet sees it or what it holds. It starts empty. Nothing is sent and nothing is paid."
                                : "A new public account: everyone can see what it holds and sends. The chain learns of it when it's first paid or used. Nothing is sent and nothing is paid."
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Field { id: newAccountField; Layout.fillWidth: true; size: 16; hint: "Nickname (optional)"; onAccepted: root.newAccount(root.accountNewKind, newAccountField.text) }
                            Action { label: "Make it"; strong: true; enabledNow: !root.accountsBusy; onPressed: root.newAccount(root.accountNewKind, newAccountField.text) }
                            Action { label: "Cancel"; onPressed: root.accountNewKind = "" }
                        }
                    }
                    Part { text: "Zones" }
                    Hint { text: "A zone is a LEZ chain with its own sequencer. Each has its own wallet here, and one is in use at a time. Your names are kept on " + root.homeName + "." }
                    Repeater {
                        model: root.zones
                        ColumnLayout {
                            id: zoneRow
                            required property var modelData
                            readonly property bool open: root.zoneOpen === modelData.id
                            Layout.fillWidth: true
                            spacing: 8
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12
                                Rectangle { Layout.preferredWidth: 10; Layout.preferredHeight: 10; radius: 5; color: zoneRow.modelData.current ? root.good : "transparent"; border.width: 1; border.color: zoneRow.modelData.current ? root.good : root.line }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Text {
                                        Layout.fillWidth: true
                                        textFormat: Text.PlainText
                                        text: zoneRow.modelData.name + (zoneRow.modelData.current ? "  ·  in use" : "") + (zoneRow.modelData.home ? "  ·  names are kept here" : "")
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: root.px(17)
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        textFormat: Text.PlainText
                                        text: zoneRow.modelData.sequencer + (zoneRow.modelData.channel ? "  ·  channel " + Logic.keyShown(zoneRow.modelData.channel) : "")
                                        color: root.dim
                                        font.family: root.mono
                                        font.pixelSize: root.px(13)
                                        elide: Text.ElideMiddle
                                    }
                                }
                                Action { visible: !zoneRow.modelData.current; label: "Use"; enabledNow: !root.zoneBusy; onPressed: root.zoneUse(zoneRow.modelData.id) }
                                Link {
                                    text: zoneRow.open ? "Close" : "Change"
                                    color: root.dim
                                    font.pixelSize: root.px(15)
                                    onPressed: { root.zoneOpen = zoneRow.open ? "" : zoneRow.modelData.id; root.zoneRemoving = false; root.zoneOtherChain = null; root.zoneNote = "" }
                                }
                            }
                            Card {
                                visible: zoneRow.open
                                Layout.fillWidth: true
                                Hint { font.pixelSize: root.px(15); text: "Name" }
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Field { id: zoneNameField; Layout.fillWidth: true; size: 16; text: zoneRow.modelData.name; hint: "What you call this zone"; onAccepted: root.zoneRename(zoneRow.modelData.id, zoneNameField.text) }
                                    Action { label: "Rename"; enabledNow: !root.zoneBusy && zoneNameField.text.trim() !== "" && zoneNameField.text.trim() !== zoneRow.modelData.name; onPressed: root.zoneRename(zoneRow.modelData.id, zoneNameField.text) }
                                }
                                Hint { font.pixelSize: root.px(15); text: "Sequencer. Another one serving the same chain can be used at any time: the wallet, its accounts and its history stay as they are." }
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Field { id: zoneSequencerField; Layout.fillWidth: true; size: 16; text: zoneRow.modelData.sequencer; hint: "https://…" }
                                    Action {
                                        label: root.zoneBusy ? "Asking it…" : "Change sequencer"
                                        enabledNow: !root.zoneBusy && zoneSequencerField.text.trim() !== "" && zoneSequencerField.text.trim() !== zoneRow.modelData.sequencer
                                        onPressed: root.zoneSequencer(zoneRow.modelData.id, zoneSequencerField.text, false)
                                    }
                                }
                                // Home's sequencer names another chain: said first, done only on a yes.
                                ColumnLayout {
                                    visible: zoneRow.modelData.home && root.zoneOtherChain !== null
                                    Layout.fillWidth: true
                                    spacing: 8
                                    StatusLine {
                                        kind: "problem"
                                        text: root.zoneOtherChain === null ? "" : "That sequencer serves another chain (channel " + Logic.keyShown(root.zoneOtherChain.channel) + "). "
                                            + zoneRow.modelData.name + " would become that chain: your names and balances are read there from then on, and what you hold on the present chain stays behind on it. To keep both, add it as a zone instead."
                                    }
                                    RowLayout {
                                        spacing: 8
                                        Action { label: "Add it as a zone"; strong: true; enabledNow: !root.zoneBusy; onPressed: root.zoneAdd("", root.zoneOtherChain.url) }
                                        Action { label: "Make " + zoneRow.modelData.name + " that chain"; grave: true; enabledNow: !root.zoneBusy; onPressed: root.zoneSequencer("home", root.zoneOtherChain.url, true) }
                                        Action { label: "Cancel"; onPressed: { root.zoneOtherChain = null; zoneSequencerField.text = zoneRow.modelData.sequencer } }
                                    }
                                }
                                Fact { label: "Wallet folder"; code: true; value: zoneRow.modelData.wallet_dir ? zoneRow.modelData.wallet_dir + (zoneRow.modelData.has_wallet ? "" : "  (no wallet in it yet)") : "The names program's own"; full: zoneRow.modelData.wallet_dir || "" }
                                Fact { visible: !!zoneRow.modelData.channel; label: "Channel"; code: true; value: zoneRow.modelData.channel || ""; full: zoneRow.modelData.channel || "" }
                                // Removing forgets the zone here; nothing on its chain changes and its wallet folder stays.
                                Hint {
                                    visible: !zoneRow.modelData.home
                                    font.pixelSize: root.px(15)
                                    text: zoneRow.modelData.current ? "This is the zone in use. To remove it, use another zone first."
                                        : "Removing a zone forgets it here. Nothing changes on its chain, and its wallet folder is kept."
                                }
                                RowLayout {
                                    visible: !zoneRow.modelData.home && !zoneRow.modelData.current
                                    spacing: 8
                                    Action { visible: !root.zoneRemoving; label: "Remove this zone"; grave: true; enabledNow: !root.zoneBusy; onPressed: root.zoneRemoving = true }
                                    Action { visible: root.zoneRemoving; label: "Yes, remove " + zoneRow.modelData.name; grave: true; strong: true; enabledNow: !root.zoneBusy; onPressed: root.zoneRemove(zoneRow.modelData.id) }
                                    Action { visible: root.zoneRemoving; label: "Cancel"; onPressed: root.zoneRemoving = false }
                                }
                            }
                        }
                    }
                    StatusLine { visible: root.zoneNote !== ""; kind: root.zoneNoteBad ? "problem" : "done"; text: root.zoneNote }
                    Action {
                        visible: !root.zoneAdding
                        label: "+  Add a zone"
                        enabledNow: !root.zoneBusy && root.zones.length > 0 && root.zones.length < root.zoneMax
                        onPressed: { root.zoneAdding = true; root.zoneNote = ""; newZoneName.text = ""; newZoneSequencer.text = "" }
                    }
                    Hint { visible: root.zones.length >= root.zoneMax; font.pixelSize: root.px(15); text: "At most " + root.zoneMax + " zones are kept. Remove one to add another." }
                    Card {
                        visible: root.zoneAdding
                        Layout.fillWidth: true
                        Hint { color: root.ink; text: "Give the address of the zone's sequencer. It is asked which chain it serves, and that chain becomes a zone here with an empty wallet folder of its own. Nothing is sent." }
                        Field { id: newZoneSequencer; Layout.fillWidth: true; size: 16; hint: "Sequencer: https://… (http:// only on this computer)" }
                        Field { id: newZoneName; Layout.fillWidth: true; size: 16; hint: "Name (optional)"; onAccepted: root.zoneAdd(newZoneName.text, newZoneSequencer.text) }
                        RowLayout {
                            spacing: 8
                            Action { label: root.zoneBusy ? "Asking it…" : "Add zone"; strong: true; enabledNow: !root.zoneBusy && newZoneSequencer.text.trim() !== ""; onPressed: root.zoneAdd(newZoneName.text, newZoneSequencer.text) }
                            Action { label: "Cancel"; enabledNow: !root.zoneBusy; onPressed: root.zoneAdding = false }
                        }
                    }
                    Part { text: "Backup" }
                    Hint { color: root.ink; text: Logic.backupLastLine(root.backupStatus !== null ? root.backupStatus.last : null, root.backupWhen) }
                    Hint { visible: Logic.backupUncoveredLine(root.backupStatus) !== ""; color: root.ink; text: Logic.backupUncoveredLine(root.backupStatus) }
                    RowLayout {
                        visible: root.backupMode === ""
                        spacing: 14
                        Action { label: "Back up now"; strong: true; onPressed: root.openBackup("make") }
                        Link { text: "Restore from a backup file"; font.pixelSize: root.px(15); onPressed: root.openBackup("restore") }
                    }

                    // Back up: what, a password, where.
                    ColumnLayout {
                        visible: root.backupMode === "make" && root.backupMade === null
                        Layout.fillWidth: true
                        spacing: 10
                        Hint { color: root.ink; text: "Step 1 of 3 · What to back up" }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Action { label: "Both wallets"; strong: root.backupWhich === "both"; onPressed: root.backupWhich = "both" }
                            Action { label: "This network's wallet (" + (root.namesNet === "0.4" ? "LEZ 0.4" : "LEZ 0.3") + ")"; strong: root.backupWhich === "this"; onPressed: root.backupWhich = "this" }
                        }
                        Hint { color: root.ink; text: "Step 2 of 3 · A password for the file" }
                        Field { id: backupPw1; Layout.fillWidth: true; secret: true; hint: "Password" }
                        Field { id: backupPw2; Layout.fillWidth: true; secret: true; hint: "The same password again" }
                        readonly property var pwHint: Logic.backupPasswordHint(backupPw1.text, backupPw2.text, root.backupStatus !== null ? root.backupStatus.min_password : 10)
                        Hint { font.pixelSize: root.px(15); color: parent.pwHint.ok || backupPw1.text === "" ? root.dim : root.ink; text: parent.pwHint.text }
                        Hint { color: root.ink; text: "Step 3 of 3 · Where it goes" }
                        Hint {
                            font.pixelSize: root.px(15)
                            text: root.backupWhere !== "" ? root.backupWhere : (root.backupStatus !== null && root.backupStatus.folder ? root.backupStatus.folder + "/" : "~/Names Wallet Backups/")
                        }
                        RowLayout {
                            spacing: 14
                            Link { text: "Choose where"; font.pixelSize: root.px(15); onPressed: root.chooseBackupWhere() }
                            Link { text: "Copy"; font.pixelSize: root.px(15); onPressed: root.copy(root.backupWhere !== "" ? root.backupWhere : String(root.backupStatus !== null && root.backupStatus.folder ? root.backupStatus.folder : ""), "The folder") }
                        }
                        RowLayout {
                            spacing: 14
                            Action {
                                label: root.backupBusy ? "Backing up…" : "Back up now"
                                strong: true
                                enabledNow: !root.backupBusy && parent.parent.pwHint.ok
                                onPressed: { var pw = backupPw1.text; backupPw1.text = ""; backupPw2.text = ""; root.makeBackup(pw) }
                            }
                            Link { text: "Cancel"; font.pixelSize: root.px(15); onPressed: { backupPw1.text = ""; backupPw2.text = ""; root.backupMode = "" } }
                        }
                    }
                    // Made, and checked at once.
                    ColumnLayout {
                        visible: root.backupMode === "make" && root.backupMade !== null
                        Layout.fillWidth: true
                        spacing: 8
                        readonly property var checked: Logic.backupCheckedLine(root.backupMade !== null ? root.backupMade.check : null)
                        Hint { color: parent.checked.ok ? root.good : root.ink; text: (parent.checked.ok ? "✓ " : "") + parent.checked.text }
                        Hint { font.pixelSize: root.px(15); text: root.backupMade !== null ? Logic.backupFoldersLine(root.backupMade.folders) : "" }
                        RowLayout {
                            spacing: 14
                            Hint { Layout.fillWidth: false; font.pixelSize: root.px(15); text: root.backupMade !== null ? root.backupMade.file : "" }
                            Link { text: "Copy"; font.pixelSize: root.px(15); onPressed: root.copy(root.backupMade.file, "The path") }
                        }
                        Hint { font.pixelSize: root.px(15); text: "Keep it somewhere else too (a USB stick, another computer). Without the password it can't be opened." }
                        Link { text: root.backupDetails ? "Details ▾" : "Details ▸"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.backupDetails = !root.backupDetails }
                        Repeater {
                            model: root.backupDetails && root.backupMade !== null && root.backupMade.check ? root.backupMade.check.networks : []
                            Hint {
                                required property var modelData
                                font.pixelSize: root.px(14)
                                text: modelData.id + ": " + (modelData.accounts || []).map(function(a) { return (a.account || "an imported private account") + (a.imported ? " (imported)" : "") }).join(", ")
                            }
                        }
                        Hint { visible: root.backupDetails; font.pixelSize: root.px(14); text: "Encrypted with AES-256-GCM under a key made from your password with Argon2id (64 MiB, 3 passes). It holds each wallet folder's store and settings files; not program files or logs." }
                        Link { text: "Done"; font.pixelSize: root.px(15); onPressed: root.backupMode = "" }
                    }

                    // Restore: the file, its password, what is inside, then one confirmation.
                    ColumnLayout {
                        visible: root.backupMode === "restore" && root.restoreResult === null
                        Layout.fillWidth: true
                        spacing: 10
                        Hint { color: root.ink; text: "Step 1 of 3 · The backup file" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Action { label: "Choose file"; strong: root.restoreFile === ""; onPressed: root.chooseRestoreFile() }
                            Field { id: restorePathField; Layout.fillWidth: true; hint: "or paste its full path"; text: root.restoreFile; onTextChanged: if (text !== root.restoreFile) { root.restoreFile = text; root.restoreInside = null } }
                        }
                        Hint { color: root.ink; text: "Step 2 of 3 · Its password" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Field { id: restorePw; Layout.fillWidth: true; secret: true; hint: "The backup's password" }
                            Action { label: root.backupBusy && root.restoreInside === null ? "Opening…" : "Open"; enabledNow: !root.backupBusy && root.restoreFile !== "" && restorePw.text !== ""; onPressed: root.openRestoreFile(restorePw.text) }
                        }
                        Hint { color: root.ink; visible: root.restoreInside !== null; text: "Step 3 of 3 · What it holds" }
                        Hint { visible: root.restoreInside !== null; font.pixelSize: root.px(15); text: root.restoreInside !== null ? "Made " + root.backupWhen(root.restoreInside.created) + "." : "" }
                        Repeater {
                            model: root.restoreInside !== null ? root.restoreInside.networks : []
                            ColumnLayout {
                                id: restoreNet
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 6
                                Hint {
                                    color: root.ink
                                    text: (restoreNet.modelData.shown ? restoreNet.modelData.shown.version : restoreNet.modelData.id) + " wallet · " + (restoreNet.modelData.accounts || []).length + " accounts: "
                                        + (restoreNet.modelData.accounts || []).map(function(a) { return a.account ? Logic.idShort(a.account) : "an imported private account" }).join(", ")
                                }
                                Hint {
                                    font.pixelSize: root.px(15)
                                    color: restoreNet.modelData.can_restore ? root.dim : root.ink
                                    text: restoreNet.modelData.can_restore ? "Goes to " + restoreNet.modelData.folder + "." : "A wallet is there already, so it can't be restored over it."
                                }
                                Hint {
                                    visible: restoreNet.modelData.settings === true
                                    font.pixelSize: root.px(15)
                                    // The wallet's own wallet_config.json comes back, but which network Names uses
                                    // is Names' setting (every call passes the network: the CLI's --rpc), not that file's.
                                    text: "Its wallet settings come back too. The network Names uses stays as set here."
                                }
                                Action {
                                    visible: restoreNet.modelData.can_restore
                                    label: root.backupBusy ? "Restoring…" : "Restore"
                                    strong: true
                                    enabledNow: !root.backupBusy && restorePw.text !== ""
                                    onPressed: { var pw = restorePw.text; restorePw.text = ""; root.restoreFromBackup(pw, restoreNet.modelData.id) }
                                }
                            }
                        }
                        Link { text: "Cancel"; font.pixelSize: root.px(15); onPressed: { restorePw.text = ""; root.backupMode = ""; root.restoreInside = null } }
                    }
                    Hint {
                        visible: root.backupMode === "restore" && root.restoreResult !== null
                        color: root.good
                        text: root.restoreResult !== null ? "✓ Restored: " + (root.restoreResult.accounts || []).length + " accounts into " + root.restoreResult.folder + "." : ""
                    }
                    StatusLine { visible: root.backupError !== ""; kind: Logic.noticeKind(root.backupError, true); text: root.backupError }

                    Part { text: "Bridge" }
                    Hint { text: "Moving tokens from your Logos Blockchain node into LEZ is in the Wallet. One way for now; about 12 hours to arrive." }
                    Action {
                        label: "Open the bridge"
                        enabledNow: root.cliPath !== ""
                        onPressed: root.openGetTokens()
                    }
                    Part { text: "Connected apps" }
                    Hint { text: root.appGrants.length === 0 ? "Other apps can't see your accounts unless you share them here: when one asks, you choose which accounts it sees, and approve each payment it asks for. (An app installed under the Names app's own package name could, so install apps only from sources you trust.)" : "These apps can see the accounts listed and ask you to approve payments. They can't send anything without you." }
                    Repeater {
                        model: root.appGrants
                        RowLayout {
                            id: grantRow
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { Layout.preferredWidth: 200; text: grantRow.modelData.app; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); elide: Text.ElideRight }
                            Text {
                                Layout.fillWidth: true
                                text: (grantRow.modelData.accounts || []).map(function(account) { return root.shorten(account) }).join(", ")
                                color: root.dim
                                font.family: root.mono
                                font.pixelSize: root.px(14)
                                elide: Text.ElideRight
                            }
                            Action { label: "Remove access"; onPressed: root.revokeApp(grantRow.modelData.app) }
                        }
                    }
                    Link {
                        Layout.topMargin: 10
                        text: root.settingsOpen ? "Hide advanced" : "Advanced"
                        color: root.faint
                        font.pixelSize: root.px(15)
                        onPressed: root.settingsOpen = !root.settingsOpen
                    }
                }


                // Advanced.
                ColumnLayout {
                    visible: root.settingsOpen && root.section === "settings"
                    Layout.fillWidth: true
                    spacing: 10
                    Text {
                        text: "Advanced"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: root.px(28)
                    }
                    Part { text: "Registry" }
                    Hint { text: "Everyone using the same registry sees the same names." }
                    Field {
                        id: registryField
                        Layout.fillWidth: true
                        size: 17
                        text: root.registry
                        hint: "Program ID of the registry"
                    }
                    Hint { font.pixelSize: root.px(15); text: "Sequencers are set per zone: Settings, Zones." }
                    // Saved only on purpose: a slip here points every read and change elsewhere (M-7).
                    RowLayout {
                        visible: !root.networkConfirm
                        spacing: 8
                        Action {
                            label: "Save"
                            enabledNow: registryField.text.trim() !== root.registry
                            onPressed: root.networkConfirm = true
                        }
                        Link {
                            visible: registryField.text.trim() !== root.registry
                            text: "Undo"
                            font.pixelSize: root.px(16)
                            onPressed: registryField.text = root.registry
                        }
                    }
                    Card {
                        visible: root.networkConfirm
                        Layout.fillWidth: true
                        Hint {
                            color: root.ink
                            text: "Switch to the registry " + Logic.keyShown(registryField.text.trim()) + "? Names and every change you make to them then go there. Only switch to one you trust."
                        }
                        RowLayout {
                            spacing: 8
                            Action {
                                label: "Yes, switch"
                                strong: true
                                onPressed: { root.networkConfirm = false; root.saveConfig(registryField.text.trim()) }
                            }
                            Action {
                                label: "Cancel"
                                onPressed: { root.networkConfirm = false; registryField.text = root.registry }
                            }
                        }
                    }
                    Part { text: "Wallet files" }
                    Hint { text: "Not used on this chain: your names program's own wallet signs for your names." }
                    Field {
                        id: walletConfigField
                        Layout.fillWidth: true
                        size: 17
                        text: root.walletConfig
                        hint: "Path of wallet_config.json"
                    }
                    Field {
                        id: walletStorageField
                        Layout.fillWidth: true
                        size: 17
                        text: root.walletStorage
                        hint: "Path of storage.json"
                    }
                    Action {
                        label: "Use this wallet"
                        onPressed: root.saveWalletFiles(walletConfigField.text.trim(), walletStorageField.text.trim())
                    }
                    Part { text: "names CLI" }
                    Hint { text: "Names your names CLI holds privately are edited through it. Each change is proven on this machine and takes a few minutes." }
                    StatusLine { visible: root.proofWarning !== ""; kind: Logic.noticeKind(root.proofWarning, true); text: root.proofWarning }
                    Field {
                        id: cliPathField
                        Layout.fillWidth: true
                        size: 17
                        text: root.cliPath
                        hint: "Path of the names program (~/.local/bin/names)"
                    }
                    Field {
                        id: cliWalletField
                        Layout.fillWidth: true
                        size: 17
                        text: root.cliWallet
                        hint: "Its wallet folder (~/.lee/wallet)"
                    }
                    Action {
                        label: "Use this CLI"
                        onPressed: root.saveCliSettings(cliPathField.text.trim(), cliWalletField.text.trim())
                    }
                    // LEZ 0.4 without a working tool: the one-click install, as on Welcome.
                    RowLayout {
                        visible: root.namesNet === "0.4" && (root.cliPath === "" || root.cliOutdated)
                        spacing: 12
                        Action { label: root.installJob !== null ? "Installing…" : "Install the LEZ 0.4 names tool"; strong: true; enabledNow: root.installJob === null; onPressed: root.installCli() }
                        Link { visible: Logic.FULL_TOOL_DOWNLOAD && root.installJob === null; text: "Install the full tool, with proving"; font.pixelSize: root.px(15); onPressed: root.installFullTool() }
                        Hint { visible: root.installJob !== null; Layout.fillWidth: true; color: root.waitTone; font.pixelSize: root.px(15); text: root.installJob === null ? "" : root.installJob.line }
                    }
                    Link {
                        visible: root.lastReply !== ""
                        Layout.topMargin: 4
                        text: root.showLastReply ? "Hide last module reply" : "Show last module reply"
                        font.pixelSize: root.px(15)
                        onPressed: root.showLastReply = !root.showLastReply
                    }
                    Text {
                        visible: root.lastReply !== "" && root.showLastReply
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: root.lastReply
                        color: root.dim
                        font.family: root.mono
                        font.pixelSize: root.px(10)
                        wrapMode: Text.WrapAnywhere
                    }
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: 8; height: 1; color: root.line }
                }

                StatusLine {
                    visible: root.notice !== ""
                    kind: root.noticeKind
                    text: root.notice
                }

                // Nothing chosen yet.
                ColumnLayout {
                    visible: root.section === "search" && root.record === null && root.missing === "" && !root.registering
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 10
                    Text {
                        text: root.moduleOutdated !== "" ? "Update the names module" : !root.configKnown ? "Starting" : root.registry === "" ? "Choose a registry to begin" : "Find someone"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: root.px(36)
                    }
                    Hint {
                        font.pixelSize: root.px(18)
                        color: root.ink
                        text: root.moduleOutdated !== "" ? "This app needs names module " + root.neededModule + " or newer; Basecamp loaded " + root.moduleOutdated + ". Open Applications, click Names and press Install." : !root.configKnown ? "Waiting for the names module to answer." : root.registry === "" ? "Open Advanced and enter the registry's program ID." : "Type a name in the box on the left to see its profile and page."
                    }
                    Action {
                        visible: root.configKnown && root.registry !== ""
                        Layout.topMargin: 10
                        label: "Register a name"
                        strong: true
                        onPressed: root.openRegister()
                    }
                }

                // Register a name: type one, check it.
                ColumnLayout {
                    visible: root.registering && root.record === null && root.missing === ""
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 14
                    Text {
                        text: "Register a name"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: root.px(36)
                    }
                    NetSwitch {}
                    RunsOn {}
                    Hint {
                        font.pixelSize: root.px(18)
                        text: root.namesNet === "0.4"
                            ? "Pick a name people can use to find your profile and page. On LEZ 0.4 it is registered from your public account, in the open, with a deposit you get back when you release it."
                            : "Pick a name people can use to find your profile and page. Held privately, with a deposit you get back when you release it."
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.maximumWidth: 560
                        spacing: 8
                        Field {
                            id: registerField
                            Layout.fillWidth: true
                            size: 22
                            hint: "yourname"
                            onAccepted: root.checkRegister(registerField.text)
                        }
                        Action {
                            label: root.registerChecking ? "Checking…" : "Check"
                            strong: true
                            enabledNow: registerField.text.trim() !== "" && !root.registerChecking
                            onPressed: root.checkRegister(registerField.text)
                        }
                    }
                    Hint { visible: root.registerWait !== ""; color: root.waitTone; text: root.registerWait }
                    RowLayout {
                        visible: root.registerTaken !== ""
                        spacing: 12
                        Rectangle { width: 10; height: 10; radius: 5; color: root.dim }
                        Text {
                            text: root.registerTaken + " is taken. Try another."
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(18)
                        }
                        Link {
                            text: "See it"
                            font.pixelSize: root.px(16)
                            onPressed: root.show(root.registerTaken)
                        }
                    }
                    Hint {
                        font.pixelSize: root.px(15)
                        text: root.lockedUpTo > 0 ? (root.lockedUpTo + 1) + " to 32 letters, digits or hyphens. Shorter names, and a reserved list, are placed by the registry's admin." : "1 to 32 letters, digits or hyphens. Some short names are reserved."
                    }
                }

                // A name nobody holds.
                ColumnLayout {
                    visible: root.missing !== ""
                    Layout.fillWidth: true
                    Layout.topMargin: 24
                    spacing: 14
                    NameLabel {
                        Layout.fillWidth: true
                        name: root.missing
                        size: 52
                        weight: Font.DemiBold
                    }
                    RowLayout {
                        spacing: 10
                        readonly property bool reserved: root.deposit !== null && root.deposit.tier === "locked"
                        readonly property bool underway: root.registerStepOf(root.missing) > 0
                        Rectangle { width: 10; height: 10; radius: 5; color: parent.reserved ? root.dim : parent.underway ? root.waitTone : root.good }
                        Text {
                            text: parent.underway ? "Registering privately for you" : parent.reserved ? "Reserved: names of one to four characters and the reserved list are placed by the registry's admin" : "Available"
                            color: parent.underway ? root.waitTone : root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(19)
                        }
                    }
                    // A private registration through the names CLI, under way or stopped.
                    StatusLine {
                        visible: root.cliPendingFor(root.missing) !== null && root.cliPendingFor(root.missing).failed
                        kind: "problem"
                        text: root.cliPendingFor(root.missing) === null ? "" : "It stopped: " + root.cliPendingFor(root.missing).state + "."
                    }
                    Card {
                        visible: root.registerStepOf(root.missing) > 0
                        Layout.fillWidth: true
                        Text {
                            text: "Registering " + root.missing + " privately"
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(22)
                            font.weight: Font.DemiBold
                        }
                        Steps {
                            Layout.fillWidth: true
                            active: root.registerStepOf(root.missing)
                            note: {
                                var step = root.registerStepOf(root.missing)
                                var under = root.cliPendingFor(root.missing)
                                if (step === 1 && root.pending !== null)
                                    return "usually 5–12 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.pending.started) / 60000)) + " so far"
                                if (step === 2 && under !== null) {
                                    var minutes = /about (\d+) minutes/.exec(under.state)
                                    return minutes ? minutes[1] + " min to go" : "about a minute to go"
                                }
                                return ""
                            }
                        }
                        Hint { font.pixelSize: root.px(15); text: "Keep this computer on and awake. You can close this screen and keep using the app; the name shows under My names when it's done." }
                    }
                    ColumnLayout {
                        visible: root.registerStepOf(root.missing) === 0
                        Layout.fillWidth: true
                        spacing: 22
                    ColumnLayout {
                        visible: root.cliPath === ""
                        Layout.fillWidth: true
                        spacing: 10
                        Part { text: "Deposit" }
                        Hint {
                            font.pixelSize: root.px(20)
                            color: root.ink
                            text: root.deposit === null ? "Checking the deposit." : root.deposit.tier === "locked" ? "One- and two-character names are reserved." : root.deposit.deposit === null || root.deposit.deposit === undefined ? (root.deposit.fee ? root.grouped(root.deposit.fee) + " to register" : "The registry is not set up for registering yet.") : root.grouped(root.deposit.deposit)
                        }
                        Hint {
                            visible: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode === "deposit"
                            text: "Refunded in full when you release the name. Testnet: names don't expire here, but to keep one on mainnet you claim it within 30 days of launch. Network fee extra."
                        }
                        StatusLine {
                            visible: root.deposit !== null && root.deposit.claims_until !== null && root.deposit.claims_until !== undefined
                            kind: ""
                            text: "Until " + (root.deposit !== null && root.deposit.claims_until ? root.dated(root.deposit.claims_until) : "") + ", whoever held this name on the earlier chain can take it back. If they do, you lose the name, and your deposit goes to them when they release it."
                        }
                        Link {
                            visible: root.claimsUntil !== null && root.claimsUntil !== undefined && !root.claimBusy
                            text: "This was my name on the earlier chain: take it back for free"
                            font.pixelSize: root.px(16)
                            onPressed: root.claimName(root.missing)
                        }
                        Hint { visible: root.claimLine !== ""; text: root.claimLine }
                    }
                    Hint {
                        visible: root.namesNet === "0.4"
                        color: root.ink
                        text: "Registering privately stays off on LEZ 0.4 until real proofs have been run there. Register from a public account below, or switch to LEZ 0.3."
                    }
                    Card {
                        visible: root.namesNet === "0.3" && root.cliPath !== "" && !(root.deposit !== null && root.deposit.tier === "locked")
                        Layout.fillWidth: true
                        Text {
                            text: "Register privately"
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: root.px(22)
                            font.weight: Font.DemiBold
                        }
                        Hint { text: "Held by your private account, which never appears on chain. Nobody can find who holds it." }
                        GridLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                            columns: 2
                            columnSpacing: 24
                            Figure {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                label: "Deposit"
                                value: root.deposit === null ? "…" : root.deposit.deposit !== null && root.deposit.deposit !== undefined ? root.grouped(root.deposit.deposit) : root.deposit.fee ? root.grouped(root.deposit.fee) : "Not set up"
                                note: "Refunded in full when you release it"
                            }
                            Figure {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                label: "Time"
                                value: "About 20 min"
                                note: "Keep this computer on and awake until it's done."
                            }
                        }
                        Link {
                            text: root.registerHow ? "Hide how it works" : "How it works: three steps"
                            color: root.faint
                            font.pixelSize: root.px(14)
                            onPressed: root.registerHow = !root.registerHow
                        }
                        Steps {
                            visible: root.registerHow
                            Layout.fillWidth: true
                            Layout.topMargin: 4
                        }
                        Text {
                            Layout.fillWidth: true
                            text: root.cliBalance === undefined ? "Checking your private account…"
                                : root.cliBalance === null || !root.cliBalance.account ? "No private account found. Run names setup in a terminal first."
                                : "From your private account (balance " + root.grouped(root.cliBalance.balance) + ")"
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: root.px(15)
                            wrapMode: Text.Wrap
                        }
                        StatusLine {
                            visible: root.cliBalance !== undefined && root.cliBalance !== null && !root.cliCanPay()
                            kind: "problem"
                            text: "That's less than the deposit. Add funds to this private account first."
                        }
                        Link {
                            visible: root.cliBalance !== undefined && root.cliBalance !== null && !root.cliCanPay()
                            text: "Get tokens"
                            font.pixelSize: root.px(15)
                            onPressed: root.openGetTokens()
                        }
                        Text { Layout.topMargin: 6; text: "Then"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
                        RowLayout {
                            spacing: 6
                            Tab { label: "Keep it private"; current: !root.registerPublic; onPressed: root.registerPublic = false }
                            Tab { label: "Make it public, unlinked"; current: root.registerPublic; onPressed: root.registerPublic = true }
                        }
                        Hint {
                            font.pixelSize: root.px(14)
                            text: root.registerPublic
                                ? "Registered privately first, so nobody sees who registered it, then moved to a new public account funded privately with 10,000,000 for gas. Posts take about 30 seconds; nothing links that account to you. About 10 more minutes."
                                : "Posts take 5–12 minutes each, free, and nobody can find which account holds it. You can make it public later in Settings."
                        }
                        Action {
                            Layout.topMargin: 6
                            label: "Register " + root.missing
                            strong: true
                            changes: true
                            enabledNow: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode !== "unconfigured" && root.cliBalance !== null && root.cliBalance !== undefined && root.cliCanPay()
                            onPressed: root.registerPrivately()
                        }
                        Text {
                            text: "Testnet name: to keep it on mainnet, claim it within 30 days of launch."
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: root.faint
                            font.family: root.sans
                            font.pixelSize: root.px(14)
                        }
                    }
                    Link {
                        visible: root.namesNet === "0.3" && root.cliPath !== "" && root.wallet.length > 0 && !(root.deposit !== null && root.deposit.tier === "locked")
                        text: root.publicRegisterOpen ? "Hide the public option" : "Register from a public account instead"
                        color: root.faint
                        font.pixelSize: root.px(16)
                        onPressed: root.publicRegisterOpen = !root.publicRegisterOpen
                    }
                    Panel {
                        visible: root.wallet.length > 0 && (root.cliPath === "" || root.publicRegisterOpen || root.namesNet === "0.4")
                        title: root.cliPath !== "" ? "From a public account" : "Register it to"
                        Accounts {}
                        Hint {
                            color: root.cliPath !== "" ? root.dim : root.ink
                            text: "Anyone who knows the name can find this account, and so its balance and history." + (root.namesNet === "0.4" ? "" : (root.cliPath !== "" ? "" : " To hide it, install the names CLI and register privately.") + " Payments to the name still go to your private account.")
                        }
                        RowLayout {
                            visible: root.namesNet === "0.4"
                            Layout.fillWidth: true
                            spacing: 10
                            Rectangle {
                                Layout.preferredWidth: 18
                                Layout.preferredHeight: 18
                                Layout.alignment: Qt.AlignTop
                                radius: 4
                                color: root.publishOnRegister ? root.brass : "transparent"
                                border.width: 2
                                border.color: root.brass
                                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.publishOnRegister = !root.publishOnRegister }
                            }
                            Hint {
                                color: root.ink
                                text: "People can pay " + (root.missing || "the name") + " at your account " + Logic.idShort(root.wallet.length > root.fromIndex ? root.wallet[root.fromIndex].account : "") + "."
                                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.publishOnRegister = !root.publishOnRegister }
                            }
                        }
                        Hint { visible: text !== ""; color: root.ink; font.pixelSize: root.px(15); text: Logic.uncheckedLine(root.toolNow, ["commit", "register"]) }
                        Action {
                            label: "Register " + root.missing + " publicly"
                            strong: root.cliPath === ""
                            changes: true
                            enabledNow: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode !== "unconfigured"
                            onPressed: root.registerName()
                        }
                    }
                    }
                    ColumnLayout {
                        visible: root.wallet.length === 0 && root.cliPath === ""
                        Layout.fillWidth: true
                        spacing: 12
                        Hint {
                            font.pixelSize: root.px(17)
                            text: root.walletProblem
                        }
                        Action {
                            label: "Check the wallet again"
                            onPressed: root.connectWallet()
                        }
                    }
                }

                // A registered name.
                ColumnLayout {
                    visible: root.record !== null
                    Layout.fillWidth: true
                    spacing: 22

                    RowLayout {
                        visible: root.tool !== "settings"
                        Layout.fillWidth: true
                        spacing: 22
                        Avatar {
                            Layout.preferredWidth: 104
                            Layout.preferredHeight: 104
                            Layout.alignment: Qt.AlignTop
                            pixels: root.profileAvatar
                            owner: root.record !== null ? root.record.owner_tag : ""
                            letter: root.record !== null ? root.record.name.charAt(0) : ""
                            ring: root.accent
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            spacing: 4
                            Text {
                                visible: root.profileDisplay !== ""
                                Layout.fillWidth: true
                                text: root.profileDisplay
                                textFormat: Text.PlainText
                                color: root.ink
                                font.family: root.headingFont
                                font.weight: Font.DemiBold
                                font.pixelSize: root.px(44)
                                elide: Text.ElideRight
                            }
                            NameLabel {
                                visible: root.profileDisplay === ""
                                Layout.fillWidth: true
                                name: root.record !== null ? root.record.name : ""
                                size: 52
                                weight: Font.DemiBold
                            }
                            Text {
                                visible: root.forwardedFrom !== "" && root.record !== null
                                text: root.record === null ? "" : root.forwardedFrom + " → " + root.record.name + "  (" + root.forwardedFrom + " forwards here)"
                                textFormat: Text.PlainText
                                color: root.ink
                                font.family: root.sans
                                font.weight: Font.DemiBold
                                font.pixelSize: root.px(16)
                            }
                            Text {
                                visible: root.profileDisplay !== ""
                                text: root.record !== null ? root.shown(root.record.name) : ""
                                textFormat: Text.PlainText
                                color: root.accent
                                font.family: root.sans
                                font.weight: Font.DemiBold
                                font.pixelSize: root.px(20)
                            }
                            Text {
                                visible: root.profileBio !== ""
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                text: root.profileBio
                                textFormat: Text.PlainText
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: root.px(18)
                                lineHeight: 1.25
                                wrapMode: Text.Wrap
                                maximumLineCount: 8
                                elide: Text.ElideRight
                            }
                        }
                        Action {
                            visible: root.mine
                            Layout.alignment: Qt.AlignTop
                            label: "Edit"
                            strong: true
                            onPressed: root.openSettings()
                        }
                    }

                    Flow {
                        visible: root.tool !== "settings"
                        Layout.fillWidth: true
                        spacing: 10
                        Chip { label: root.holderChip() }
                        Chip { label: root.paymentsChip() }
                        Chip {
                            visible: root.record !== null && Logic.someAmount(root.record.deposit)
                            label: root.record !== null && Logic.someAmount(root.record.deposit) ? "Deposit " + root.grouped(root.record.deposit) : ""
                        }
                        Chip {
                            visible: root.record !== null && !root.record.never
                            label: root.record === null || root.record.never ? "" : (root.record.lapsed ? "Lapsed " : "Held until ") + root.dated(root.record.expires)
                        }
                        Chip {
                            label: "Copy link"
                            link: true
                            onPressed: root.copy(root.linkFor(root.record.name), "Link")
                        }
                    }
                    RowLayout {
                        visible: root.tool !== "settings" && root.record !== null && !root.mine
                        spacing: 8
                        Action {
                            label: "Message"
                            strong: true
                            onPressed: { var name = root.record.name; root.go("messages"); root.chatLoad(); root.chatStart(name) }
                        }
                        Action {
                            label: "Pay"
                            onPressed: { root.payTo(root.record.name); root.walletRefresh() }
                        }
                        Action {
                            label: root.record !== null && root.boardFollowing(root.record.name) ? "Following" : "Follow"
                            onPressed: root.boardFollow(root.record.name, root.boardFollowing(root.record.name))
                        }
                    }

                    // The holder's settings for this name.
                    ColumnLayout {
                        visible: root.tool === "settings"
                        Layout.fillWidth: true
                        spacing: 16
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 20
                            BackButton {
                                visible: root.section !== "profile"
                                label: root.record !== null ? root.record.name : "Back"
                                onPressed: root.tool = ""
                            }
                            Text {
                                text: "Edit"
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: root.px(24)
                                font.weight: Font.DemiBold
                            }
                            NameLabel {
                                Layout.leftMargin: -12
                                name: root.record !== null ? root.record.name : ""
                                size: 24
                                weight: Font.DemiBold
                            }
                            Item { Layout.preferredWidth: 8 }
                            Tab { label: "Profile"; current: root.editTab === "profile"; onPressed: root.editTab = "profile" }
                            Tab { label: "Page"; current: root.editTab === "page"; onPressed: root.editTab = "page" }
                            Tab { label: "Give it up"; current: root.editTab === "giveup"; onPressed: root.editTab = "giveup" }
                            Item { Layout.fillWidth: true }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: root.line }

                        StatusLine {
                            visible: root.cliHeld && root.busy && root.notice !== ""
                            kind: root.noticeKind
                            text: root.notice
                        }
                        Hint {
                            visible: root.cliHeld && !root.busy
                            color: root.ink
                            text: root.record !== null && root.heldPublicly(root.record.name) ? "Saving takes about 30 seconds." : "Saving takes 5–12 minutes (private name); keep Basecamp open until it's done."
                        }
                        RowLayout {
                        visible: root.editTab === "profile"
                        Layout.fillWidth: true
                        spacing: 40
                        ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        spacing: 12

                        Repeater {
                            model: root.profileFields
                            RowLayout {
                                id: profileRow
                                required property var modelData
                                visible: profileRow.modelData.key !== "cid" || root.entriesOpen
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    Layout.preferredWidth: 110
                                    Layout.alignment: profileRow.modelData.lines ? Qt.AlignTop : Qt.AlignVCenter
                                    Layout.topMargin: profileRow.modelData.lines ? 12 : 0
                                    text: profileRow.modelData.label
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(17)
                                    elide: Text.ElideRight
                                }
                                Area {
                                    visible: profileRow.modelData.lines === true
                                    Layout.fillWidth: true
                                    implicitHeight: 110
                                    code: false
                                    hint: profileRow.modelData.hint
                                    text: root.record !== null && root.record.texts && root.record.texts[profileRow.modelData.key] !== undefined ? root.record.texts[profileRow.modelData.key] : ""
                                    onTextChanged: if (profileRow.modelData.lines === true) {
                                        var draft = Object.assign({}, root.profileDraft)
                                        draft[profileRow.modelData.key] = text
                                        root.profileDraft = draft
                                    }
                                }
                                Flow {
                                    visible: profileRow.modelData.choices !== undefined
                                    Layout.fillWidth: true
                                    spacing: 6
                                    Repeater {
                                        model: profileRow.modelData.choices || []
                                        Tab {
                                            required property var modelData
                                            label: modelData[1]
                                            current: root.draftText(profileRow.modelData.key).toLowerCase() === modelData[0]
                                            onPressed: {
                                                var draft = Object.assign({}, root.profileDraft)
                                                draft[profileRow.modelData.key] = modelData[0]
                                                root.profileDraft = draft
                                            }
                                        }
                                    }
                                }
                                Row {
                                    visible: profileRow.modelData.swatches !== undefined
                                    spacing: 8
                                    Repeater {
                                        model: profileRow.modelData.swatches || []
                                        Rectangle {
                                            required property string modelData
                                            width: 34
                                            height: 34
                                            radius: 17
                                            color: modelData
                                            border.width: root.draftText("color").toLowerCase() === modelData ? 3 : 0
                                            border.color: root.ink
                                            MouseArea {
                                                anchors.fill: parent
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: profileField.text = parent.modelData
                                            }
                                        }
                                    }
                                }
                                RowLayout {
                                    // A name the CLI holds shows its picture this way too, not as data: text (M-6).
                                    readonly property bool shows: profileRow.modelData.key === "avatar" && /^data:/.test(root.draftText("avatar"))
                                    visible: shows
                                    Layout.fillWidth: true
                                    spacing: 12
                                    Text { text: "Picture set"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17) }
                                    Link {
                                        text: root.cliHeld ? "Change or remove" : "Remove"
                                        font.pixelSize: root.px(16)
                                        onPressed: profileField.text = ""
                                    }
                                    Item { Layout.fillWidth: true }
                                }
                                RowLayout {
                                    visible: profileRow.modelData.key === "avatar" && root.draftText("avatar") === "" && root.profileText("avatar") !== ""
                                    Layout.fillWidth: true
                                    spacing: 12
                                    Text { text: root.cliHeld ? "Picture removed when you save, unless you give a PNG file below" : "Picture removed when you save"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(17); wrapMode: Text.WordWrap; Layout.fillWidth: true }
                                    Link {
                                        text: "Undo"
                                        font.pixelSize: root.px(16)
                                        onPressed: profileField.text = root.profileText("avatar")
                                    }
                                    Item { Layout.fillWidth: true }
                                }
                                Field {
                                    id: profileField
                                    visible: profileRow.modelData.lines !== true && profileRow.modelData.choices === undefined
                                        && !(profileRow.modelData.key === "avatar" && !root.cliHeld && (/^data:/.test(root.draftText("avatar")) || root.profileText("avatar") !== ""))
                                        && !(profileRow.modelData.key === "avatar" && root.cliHeld && /^data:/.test(root.draftText("avatar")))
                                    Layout.fillWidth: true
                                    Layout.maximumWidth: profileRow.modelData.swatches !== undefined ? 130 : 100000
                                    size: 19
                                    hint: root.cliHeld && profileRow.modelData.key === "avatar" ? "Full path of a PNG file, such as /Users/you/me.png" : profileRow.modelData.hint
                                    text: root.record !== null && root.record.texts && root.record.texts[profileRow.modelData.key] !== undefined ? root.record.texts[profileRow.modelData.key] : ""
                                    onTextChanged: if (profileRow.modelData.lines !== true) {
                                        var draft = Object.assign({}, root.profileDraft)
                                        draft[profileRow.modelData.key] = text
                                        root.profileDraft = draft
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Hint { visible: text !== ""; color: root.ink; font.pixelSize: root.px(15); text: Logic.uncheckedLine(root.toolNow, ["change"]) }
                            Action {
                                label: "Save profile"
                                strong: true
                                changes: true
                                enabledNow: Object.keys(root.profileChanges()).length > 0
                                onPressed: root.saveProfile()
                            }
                            Item { Layout.fillWidth: true }
                        }

                        ColumnLayout {
                        visible: !root.cliHeld
                        Layout.fillWidth: true
                        spacing: 10
                        Part { text: "Where it is paid" }
                        Hint {
                            text: root.record === null ? "" : root.record.npk !== null && root.record.account === null ? "Private payments only. They reveal nothing about your funds."
                                : root.record.npk !== null ? "Private payments, and a public account anyone can see."
                                : root.record.account !== null ? "A public account only. Anyone can see its balance and history."
                                : "Not set up. Nobody can pay this name yet."
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Action {
                                // LEZ 0.4 is public only (the module refuses it there too).
                                visible: root.namesNet !== "0.4"
                                label: root.record !== null && root.record.npk !== null && root.record.account !== null ? "Make it private only" : "Receive privately"
                                strong: true
                                changes: true
                                enabledNow: root.record !== null && !(root.record.npk !== null && root.record.account === null)
                                onPressed: root.receivePrivately()
                            }
                            Action {
                                label: "Stop taking payments"
                                changes: true
                                enabledNow: root.record !== null && (root.record.account !== null || root.record.npk !== null)
                                onPressed: root.clearAddresses()
                            }
                            Item { Layout.fillWidth: true }
                        }
                        }

                        Link {
                            text: root.entriesOpen ? "Less" : "More: a file, entries for other apps"
                            onPressed: root.entriesOpen = !root.entriesOpen
                        }
                        ColumnLayout {
                            visible: root.entriesOpen
                            Layout.fillWidth: true
                            spacing: 10
                            Part { text: "Entries" }
                            Repeater {
                                model: root.textKeys(["cid"])
                                RowLayout {
                                    id: entry
                                    required property string modelData
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Text {
                                        Layout.preferredWidth: 96
                                        text: entry.modelData
                                        color: root.dim
                                        font.family: root.sans
                                        font.pixelSize: root.px(17)
                                        elide: Text.ElideRight
                                    }
                                    Field {
                                        id: entryValue
                                        Layout.fillWidth: true
                                        size: 19
                                        text: root.record !== null && root.record.texts[entry.modelData] !== undefined ? root.record.texts[entry.modelData] : ""
                                    }
                                    Action {
                                        label: "Save"
                                        changes: true
                                        enabledNow: entryValue.text !== "" && root.record !== null && entryValue.text !== root.record.texts[entry.modelData]
                                        onPressed: root.saveText(entry.modelData, entryValue.text)
                                    }
                                    Action {
                                        label: "Remove"
                                        changes: true
                                        onPressed: root.saveText(entry.modelData, "")
                                    }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Field {
                                    id: newKeyField
                                    Layout.preferredWidth: 96
                                    size: 17
                                    hint: "chat"
                                }
                                Field {
                                    id: newValueField
                                    Layout.fillWidth: true
                                    size: 17
                                    hint: "Value"
                                }
                                Action {
                                    label: "Add"
                                    changes: true
                                    enabledNow: newKeyField.text.trim() !== "" && newValueField.text !== ""
                                    onPressed: root.saveText(newKeyField.text, newValueField.text)
                                }
                            }
                            Hint { text: "Read by other apps: chat (Logos Chat address), inbox, agent." }
                        }

                        }
                        ColumnLayout {
                            Layout.preferredWidth: 340
                            Layout.maximumWidth: 340
                            Layout.alignment: Qt.AlignTop
                            spacing: 10
                            Text { text: "How others see it"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
                            Card {
                                Layout.fillWidth: true
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 14
                                    Avatar {
                                        Layout.preferredWidth: 64
                                        Layout.preferredHeight: 64
                                        pixels: root.profileAvatar
                                        owner: root.record !== null ? root.record.owner_tag : ""
                                        letter: root.record !== null ? root.record.name.charAt(0) : ""
                                        ring: root.draftAccent
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2
                                        Text {
                                            Layout.fillWidth: true
                                            text: root.draftText("display") !== "" ? root.draftText("display") : (root.record !== null ? root.shown(root.record.name) : "")
                                            textFormat: Text.PlainText
                                            color: root.ink
                                            font.family: root.draftFont
                                            font.pixelSize: root.px(24)
                                            font.weight: Font.DemiBold
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            visible: root.draftText("display") !== ""
                                            text: root.record !== null ? root.shown(root.record.name) : ""
                                            color: root.draftAccent
                                            font.family: root.sans
                                            font.pixelSize: root.px(15)
                                        }
                                    }
                                }
                                Text {
                                    visible: root.draftText("bio") !== ""
                                    Layout.fillWidth: true
                                    text: root.draftText("bio")
                                    textFormat: Text.PlainText
                                    color: root.dim
                                    font.family: root.draftFont
                                    font.pixelSize: root.px(16)
                                    lineHeight: 1.2
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 8
                                    elide: Text.ElideRight
                                }
                            }
                            Hint { font.pixelSize: root.px(14); text: "Anyone who knows the name can read the profile." }
                        }
                        }

                        ColumnLayout {
                        visible: root.editTab === "page"
                        Layout.fillWidth: true
                        spacing: 10
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 24
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                spacing: 8
                                Text { text: "Paste your page's HTML"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
                                Area {
                                    id: pageArea
                                    Layout.fillWidth: true
                                    implicitHeight: 420
                                    hint: "<h1>Hi</h1>\n<p>About me…</p>"
                                    onTextChanged: previewTimer.restart()
                                }
                                Text { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "This app leaves out scripts, styles and images. In a browser, the page runs as written."; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                spacing: 8
                                Text { text: "Preview · how this app shows it"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
                                Rectangle {
                                    Layout.fillWidth: true
                                    implicitHeight: 420
                                    radius: 6
                                    color: root.card
                                    border.width: 1
                                    border.color: root.line
                                    clip: true
                                    Flickable {
                                        anchors.fill: parent
                                        anchors.margins: 20
                                        contentHeight: previewText.implicitHeight
                                        clip: true
                                        Text {
                                            id: previewText
                                            width: parent.width
                                            text: pageArea.text === "" ? "Paste HTML on the left to see it here." : root.pagePreview !== null ? root.viewBody(root.pagePreview) : "…"
                                            textFormat: pageArea.text !== "" && root.pagePreview !== null && root.pagePreview.kind === "html" ? Text.RichText : Text.PlainText
                                            color: pageArea.text === "" ? root.faint : root.ink
                                            linkColor: root.accent
                                            font.family: root.pagePreview !== null && root.pagePreview.kind === "html" ? root.sans : root.mono
                                            font.pixelSize: root.pagePreview !== null && root.pagePreview.kind === "html" ? 17 : 15
                                            wrapMode: Text.Wrap
                                        }
                                    }
                                }
                                Text { text: "Public: anyone who knows the name can read it."; color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                            }
                        }
                        Timer {
                            id: previewTimer
                            interval: 400
                            onTriggered: root.previewPage()
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                            spacing: 8
                            Action {
                                label: "Publish page"
                                strong: true
                                changes: true
                                enabledNow: pageArea.text !== "" && (root.page === null || pageArea.text !== root.page.text)
                                onPressed: root.savePage(pageArea.text)
                            }
                            Action {
                                label: "Remove page"
                                changes: true
                                enabledNow: root.page !== null
                                onPressed: root.savePage("")
                            }
                        }
                        }

                        Hint {
                            visible: root.editTab === "giveup" && root.cliHeld
                            text: "To hand this name to someone, use Names → Transfer… beside it. To give it up for good, use the names CLI: names release."
                        }
                        RowLayout {
                            visible: root.editTab === "giveup" && !root.cliHeld
                            Layout.fillWidth: true
                            spacing: 24
                            Card {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                Text { text: "Transfer"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.DemiBold }
                                Hint { text: root.record !== null ? "Give " + root.shown(root.record.name) + " to someone else. Its profile and page are cleared for them." : "" }
                                Text { text: "Their account"; color: root.faint; font.family: root.sans; font.pixelSize: root.px(15) }
                                Field {
                                    id: newOwnerField
                                    Layout.fillWidth: true
                                    code: true
                                    hint: "Account ID"
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    implicitHeight: depositWarning.implicitHeight + 24
                                    radius: 8
                                    color: root.noticeFill
                                    Text {
                                        id: depositWarning
                                        x: 14
                                        y: 12
                                        width: parent.width - 28
                                        text: root.record !== null && Logic.someAmount(root.record.deposit) ? "Your deposit of " + root.grouped(root.record.deposit) + " goes with the name. They get it back when they release it, not you." : "Any deposit goes with the name."
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: root.px(15)
                                        wrapMode: Text.Wrap
                                    }
                                }
                                Action {
                                    label: root.record !== null ? "Transfer " + root.shown(root.record.name) : ""
                                    changes: true
                                    enabledNow: newOwnerField.text.trim() !== ""
                                    onPressed: root.transferName()
                                }
                            }
                            Card {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                Text { text: "Release"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.DemiBold }
                                Hint { text: "Give the name up. Its profile and page are deleted and anyone can register it." }
                                Figure {
                                    visible: root.record !== null && Logic.someAmount(root.record.deposit)
                                    label: "You get back"
                                    value: root.record !== null && Logic.someAmount(root.record.deposit) ? root.grouped(root.record.deposit) : ""
                                    note: "To the account that holds it"
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Action {
                                        visible: !root.confirmingRelease
                                        label: root.record !== null ? "Release " + root.shown(root.record.name) + "…" : ""
                                        grave: true
                                        onPressed: root.confirmingRelease = true
                                    }
                                    Text {
                                        visible: root.confirmingRelease
                                        text: "Release it?"
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: root.px(18)
                                        font.weight: Font.DemiBold
                                    }
                                    Action {
                                        visible: root.confirmingRelease
                                        label: "Yes, release it"
                                        strong: true
                                        grave: true
                                        changes: true
                                        onPressed: root.releaseName()
                                    }
                                    Action {
                                        visible: root.confirmingRelease
                                        label: "Keep it"
                                        onPressed: root.confirmingRelease = false
                                    }
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        visible: root.tool !== "settings" && root.textKeys().length > 0
                        Layout.fillWidth: true
                        spacing: 10
                        Repeater {
                            model: root.textKeys()
                            Fact {
                                required property string modelData
                                label: root.entryLabel(modelData)
                                code: modelData !== "agent"
                                value: root.record !== null && root.record.texts[modelData] !== undefined ? Logic.entryShown(root.record.texts[modelData]) : ""
                                full: root.record !== null && root.record.texts[modelData] !== undefined ? root.record.texts[modelData] : ""
                            }
                        }

                    }

                    // Their posts: gone with the board (this list goes with its code).
                    ColumnLayout {
                        visible: false
                        Layout.fillWidth: true
                        spacing: 10
                        Text { text: "Posts" + (root.authorTotal > 0 ? "  " + root.authorTotal : ""); color: root.ink; font.family: root.sans; font.pixelSize: root.px(18); font.weight: Font.Bold }
                        Hint {
                            visible: root.authorPosts.length === 0
                            font.pixelSize: root.px(15)
                            text: root.authorLoading ? "Loading posts…" : "No posts yet."
                        }
                        Repeater {
                            model: root.authorPosts.filter(function(post) { return !post.hidden })
                            Card {
                                id: authorCard
                                required property var modelData
                                Layout.fillWidth: true
                                Text {
                                    visible: !!authorCard.modelData.parent_id
                                    text: "Reply"
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: root.px(13)
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: authorCard.modelData.text
                                    textFormat: Text.PlainText
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: root.px(17)
                                    wrapMode: Text.Wrap
                                }
                                RowLayout {
                                    spacing: 18
                                    Text { text: root.boardAgo(authorCard.modelData.time); color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
                                    Link {
                                        text: "Reply"
                                        color: root.faint
                                        font.pixelSize: root.px(14)
                                        onPressed: { root.boardShow("thread", authorCard.modelData.parent_id ? authorCard.modelData.parent_id : authorCard.modelData.id); root.boardReply(authorCard.modelData.id) }
                                    }
                                    Link {
                                        readonly property bool liked: root.boardLiked.indexOf(authorCard.modelData.id) >= 0
                                        text: (liked ? "♥ " : "♡ ") + Math.max(liked ? 1 : 0, authorCard.modelData.likes)
                                        color: liked ? root.brass : root.faint
                                        font.pixelSize: root.px(14)
                                        font.underline: false
                                        onPressed: root.boardToggleLike(authorCard.modelData.id)
                                    }
                                    Link {
                                        text: "Thread"
                                        color: root.faint
                                        font.pixelSize: root.px(14)
                                        onPressed: root.boardShow("thread", authorCard.modelData.parent_id ? authorCard.modelData.parent_id : authorCard.modelData.id)
                                    }
                                }
                            }
                        }
                        Action {
                            visible: root.authorPosts.length > 0 && root.authorPosts[root.authorPosts.length - 1].n > 0
                            label: root.authorLoading ? "Loading…" : "View more"
                            enabledNow: !root.authorLoading
                            onPressed: root.loadAuthorPosts(root.authorOf, true)
                        }
                    }

                    // The page, drawn from the markup the module sanitized
                    // (page_view): allowlisted tags only, links only to http(s)
                    // addresses, no scripts, styles, images or attributes. A
                    // plain-text page is drawn as plain text. A link is never
                    // followed by a press: it shows its address, to copy or to
                    // open on a second, explicit press.
                    ColumnLayout {
                        visible: root.page !== null && root.tool !== "settings"
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 10
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Text {
                                text: "Page"
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: root.px(18)
                                font.weight: Font.Bold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: root.pageHeading()
                                textFormat: Text.PlainText
                                color: root.dim
                                font.family: root.sans
                                font.weight: Font.Medium
                                font.pixelSize: root.px(17)
                                elide: Text.ElideRight
                            }
                            Link {
                                visible: root.page !== null && root.page.text !== null && root.page.text !== undefined
                                text: root.pageSourceOpen ? "Hide source" : "Show source"
                                font.pixelSize: root.px(16)
                                onPressed: root.pageSourceOpen = !root.pageSourceOpen
                            }
                        }
                        Rectangle {
                            visible: root.pageBody() !== ""
                            Layout.fillWidth: true
                            implicitHeight: pageClip.height + 36
                            radius: 8
                            color: root.card
                            border.width: 1
                            border.color: root.line
                            clip: true
                            Rectangle {
                                width: parent.width
                                height: 3
                                radius: 8
                                color: root.accent
                            }
                            Item {
                                id: pageClip
                                x: 20
                                y: 18
                                width: parent.width - 40
                                height: root.pageAll || !root.pageIsLong ? pageText.implicitHeight : root.pageFold
                                clip: true
                                Text {
                                    id: pageText
                                    width: parent.width
                                    text: root.pageBody()
                                    textFormat: root.page !== null && root.page.kind === "html" ? Text.RichText : Text.PlainText
                                    color: root.ink
                                    linkColor: root.accent
                                    font.family: root.page !== null && root.page.kind === "html" ? root.sans : root.mono
                                    font.pixelSize: root.page !== null && root.page.kind === "html" ? 17 : 15
                                    wrapMode: Text.Wrap
                                    onLinkActivated: (link) => root.pageLink = link
                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.NoButton
                                        cursorShape: pageText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    }
                                }
                                FontMetrics {
                                    id: pageMetrics
                                    font: pageText.font
                                }
                            }
                            // A fade over the fold, so a cut page reads as cut.
                            Rectangle {
                                visible: root.pageIsLong && !root.pageAll
                                x: 1
                                width: parent.width - 2
                                height: 48
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 1
                                gradient: Gradient {
                                    GradientStop { position: 0; color: "transparent" }
                                    GradientStop { position: 1; color: root.card }
                                }
                            }
                        }
                        Link {
                            visible: root.pageIsLong || (root.page !== null && root.page.truncated === true)
                            text: root.pageIsLong ? (root.pageAll ? "Show less" : "Show all") : "Only the first 64 KB is shown"
                            font.pixelSize: root.px(16)
                            onPressed: root.pageAll = !root.pageAll
                        }

                        // The link last pressed in the page: its address in full,
                        // as plain text, to copy or open.
                        Rectangle {
                            visible: root.pageLink !== ""
                            Layout.fillWidth: true
                            implicitHeight: linkRow.implicitHeight + 20
                            radius: 6
                            color: root.raised
                            RowLayout {
                                id: linkRow
                                x: 14
                                y: 10
                                width: parent.width - 28
                                spacing: 14
                                Text {
                                    text: "Link"
                                    color: root.dim
                                    font.family: root.sans
                                    font.weight: Font.Medium
                                    font.pixelSize: root.px(17)
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.pageLink
                                    textFormat: Text.PlainText
                                    color: root.ink
                                    font.family: root.mono
                                    font.pixelSize: root.px(15)
                                    elide: Text.ElideMiddle
                                }
                                Link {
                                    text: "Copy"
                                    font.pixelSize: root.px(16)
                                    onPressed: root.copy(root.pageLink, "Link")
                                }
                                Link {
                                    text: "Open in browser"
                                    font.pixelSize: root.px(16)
                                    onPressed: root.openPageLink()
                                }
                                Link {
                                    text: "Close"
                                    font.pixelSize: root.px(16)
                                    onPressed: root.pageLink = ""
                                }
                            }
                        }

                        Rectangle {
                            visible: root.page !== null && root.pageSourceOpen
                            Layout.fillWidth: true
                            implicitHeight: sourceText.implicitHeight + 24
                            radius: 6
                            color: root.slate
                            border.width: 1
                            border.color: root.line
                            clip: true
                            Text {
                                id: sourceText
                                x: 12
                                y: 12
                                width: parent.width - 24
                                text: root.pageSource()
                                textFormat: Text.PlainText
                                color: root.dim
                                font.family: root.mono
                                font.pixelSize: root.px(14)
                                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                                maximumLineCount: 14
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }
    }

    // The app's version, bottom right of the main screen, under any sheet.
    Text {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: root.px(16)
        anchors.bottomMargin: root.px(12)
        text: "Names " + root.appVersion + (root.buildCommit !== "" ? " (" + root.buildCommit + ")" : "")
        color: root.dim
        font.family: root.sans
        font.pixelSize: root.px(15)
    }

    // Another app's request, over everything but a recovery phrase, until the
    // person answers it: who asks, the account to use, then what it will do.
    Rectangle {
        id: appSheet
        // Not while this app's own Claim or Receive screen shows it as its step (never twice).
        visible: root.appRequest !== null && !root.appInline
        anchors.fill: parent
        color: Qt.rgba(root.slate.r, root.slate.g, root.slate.b, 0.94)
        readonly property var request: root.appRequest
        readonly property string kind: request !== null ? request.kind : ""
        // Nothing behind it can be pressed while it shows.
        MouseArea { anchors.fill: parent; hoverEnabled: true; onWheel: function(wheel) { wheel.accepted = true } }
        Flickable {
            anchors.fill: parent
            contentWidth: width
            contentHeight: Math.max(height, requestCard.implicitHeight + root.px(64))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Card {
                id: requestCard
                width: Math.min(appSheet.width - root.px(64), root.px(760))
                x: (appSheet.width - width) / 2
                y: Math.max(root.px(32), (appSheet.height - implicitHeight) / 2)
                ApproveBody {}
            }
        }
    }

    // An app's request to inscribe a testimonial on the Logos Blockchain, shown
    // once no other request waits: who asks, the words, the chain account that
    // pays, and the fee the node asks for it.
    Rectangle {
        id: inscribeSheet
        visible: root.appRequest === null && root.inscribeRequest !== null
        anchors.fill: parent
        color: Qt.rgba(root.slate.r, root.slate.g, root.slate.b, 0.94)
        readonly property var request: root.inscribeRequest
        readonly property var choices: request !== null ? (request.choices || []) : []
        readonly property string nodeProblem: request !== null && request.node ? root.sentence(request.node) : ""
        readonly property bool risky: request !== null && root.appPublisherFailed(request)
        readonly property var fee: Logic.inscribeFee(root.inscribeFrom,
            request !== null && root.inscribePlanFor === request.id + " " + root.inscribeFrom ? root.inscribePlan : null,
            root.inscribeProblem)
        // Nothing behind it can be pressed while it shows.
        MouseArea { anchors.fill: parent; hoverEnabled: true; onWheel: function(wheel) { wheel.accepted = true } }
        Flickable {
            anchors.fill: parent
            contentWidth: width
            contentHeight: Math.max(height, inscribeCard.implicitHeight + root.px(64))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Card {
                id: inscribeCard
                width: Math.min(inscribeSheet.width - root.px(64), root.px(760))
                x: (inscribeSheet.width - width) / 2
                y: Math.max(root.px(32), (inscribeSheet.height - implicitHeight) / 2)
                Text {
                    Layout.fillWidth: true
                    text: Logic.inscribeTitle(inscribeSheet.request)
                    textFormat: Text.PlainText
                    color: root.ink
                    font.family: root.sans
                    font.pixelSize: root.px(24)
                    font.weight: Font.DemiBold
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                }
                Hint {
                    font.pixelSize: root.px(15)
                    text: inscribeSheet.request === null ? "" : "Asked by the app “" + inscribeSheet.request.app + "”, as Basecamp names it. "
                        + root.appExpiresIn(inscribeSheet.request) + (root.inscribeRequests.length > 1 ? " " + (root.inscribeRequests.length - 1) + " more waiting." : "")
                }
                // Who publishes the app, checked against its LNS name by the Program Registry.
                Text {
                    readonly property var line: root.publisherShown(inscribeSheet.request)
                    visible: line !== null
                    Layout.fillWidth: true
                    text: line === null ? "" : line.text
                    textFormat: Text.PlainText
                    color: line === null ? root.dim : line.tone === "good" ? root.good : line.tone === "warn" ? root.ink : root.dim
                    font.family: root.sans
                    font.pixelSize: root.px(16)
                    font.weight: line !== null && line.tone !== "plain" ? Font.DemiBold : Font.Normal
                    wrapMode: Text.WordWrap
                }

                // The words, whole, as they will be written.
                Part { text: "What it writes" }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: inscribeWords.implicitHeight + 24
                    radius: 6
                    color: root.raised
                    border.width: 1
                    border.color: root.line
                    Text {
                        id: inscribeWords
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 12
                        text: inscribeSheet.request !== null ? inscribeSheet.request.text : ""
                        textFormat: Text.PlainText
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: root.px(18)
                        lineHeight: 1.25
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    }
                }
                Hint {
                    text: "An inscription is public and stays on the Logos Blockchain for good: anyone can read these words and see the account that paid for them. It can't be changed or removed."
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Fact { label: "Chain"; value: "Logos Blockchain (not the LEZ zone)" }
                    Fact { label: "Filed under"; code: true; value: inscribeSheet.request !== null ? inscribeSheet.request.submission : "" }
                    Fact { label: "Moves"; value: "Nothing but the fee" }
                }

                // The chain account that inscribes it and pays.
                Part { text: "Inscribe and pay the fee from" }
                Repeater {
                    model: inscribeSheet.choices
                    Rectangle {
                        id: inscribeRow
                        required property var modelData
                        readonly property bool ticked: root.inscribeFrom === modelData.key
                        readonly property var shown: Logic.inscribeChoice(modelData)
                        Layout.fillWidth: true
                        implicitHeight: root.px(44)
                        radius: 6
                        color: ticked ? root.raised : "transparent"
                        border.width: 1
                        border.color: ticked ? root.brass : root.line
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 10
                            Rectangle {
                                Layout.preferredWidth: 18
                                Layout.preferredHeight: 18
                                radius: 9
                                color: inscribeRow.ticked ? root.brass : "transparent"
                                border.width: 2
                                border.color: inscribeRow.ticked ? root.brass : root.dim
                            }
                            Text { text: inscribeRow.shown.title; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: root.px(16); elide: Text.ElideRight; Layout.maximumWidth: root.px(260) }
                            Text { text: inscribeRow.shown.sub; color: root.dim; font.family: root.mono; font.pixelSize: root.px(15); elide: Text.ElideRight; Layout.fillWidth: true }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.chooseInscribeAccount(inscribeRow.modelData.key)
                        }
                    }
                }
                StatusLine {
                    visible: inscribeSheet.nodeProblem !== ""
                    kind: "problem"
                    text: inscribeSheet.nodeProblem + " You can still reject this."
                }
                StatusLine {
                    visible: inscribeSheet.request !== null && inscribeSheet.nodeProblem === "" && inscribeSheet.choices.length === 0
                    kind: "problem"
                    text: "Your node's wallet has no Logos Blockchain account yet. Add one on the Blockchain page; an inscription can't be paid from a LEZ account."
                }
                Fact {
                    visible: inscribeSheet.nodeProblem === "" && inscribeSheet.choices.length > 0 && inscribeSheet.fee.tone !== "bad"
                    label: "Network fee"
                    value: inscribeSheet.fee.text
                }
                StatusLine {
                    visible: inscribeSheet.nodeProblem === "" && inscribeSheet.choices.length > 0 && inscribeSheet.fee.tone === "bad"
                    kind: "problem"
                    text: inscribeSheet.fee.text
                }

                // An app that may be a fake needs a second, deliberate tick.
                Tick {
                    visible: inscribeSheet.risky
                    checked: root.inscribeUnderstood
                    text: "I've read the warning above and still want these words inscribed from my account."
                    onToggled: root.inscribeUnderstood = !root.inscribeUnderstood
                }

                StatusLine { visible: root.inscribeError !== ""; kind: Logic.noticeKind(root.inscribeError, true); text: root.inscribeError }
                StatusLine { visible: root.inscribeBusy; kind: "waiting"; text: "Signing it and sending it to your node. This can take a minute or two…" }
                RowLayout {
                    Layout.topMargin: 8
                    spacing: 10
                    Action {
                        label: root.publisherChecking(root.inscribeRequest) ? "Waiting for the check…" : "Approve and inscribe"
                        strong: true
                        enabledNow: root.inscribeCanApprove()
                        onPressed: root.answerInscribeRequest(true)
                    }
                    Action {
                        label: "Reject"
                        enabledNow: !root.inscribeBusy
                        onPressed: root.answerInscribeRequest(false)
                    }
                }
                // A disabled Approve says why (STYLE.md).
                WhyLine {
                    visible: !root.inscribeBusy && !root.inscribeCanApprove() && text !== ""
                    text: root.publisherChecking(root.inscribeRequest) ? "Waiting for the Program Registry's answer."
                        : root.inscribeFrom === "" ? "Choose the account that pays above."
                        : root.inscribeRequest !== null && root.appPublisherFailed(root.inscribeRequest) && !root.inscribeUnderstood ? "Tick the box above to inscribe anyway."
                        : root.inscribeProblem !== "" ? "See the problem above." : ""
                }
            }
        }
    }

    // A new wallet's recovery phrase, over everything, until it's written down.
    Rectangle {
        visible: root.recoveryPhrase !== ""
        anchors.fill: parent
        color: Qt.rgba(root.slate.r, root.slate.g, root.slate.b, 0.94)
        // Nothing behind it can be pressed while it shows.
        MouseArea { anchors.fill: parent; hoverEnabled: true; onWheel: function(wheel) { wheel.accepted = true } }
        Card {
            anchors.centerIn: parent
            width: Math.min(parent.width - root.px(64), root.px(720))
            Text {
                Layout.fillWidth: true
                text: "Write down your recovery phrase"
                color: root.ink
                font.family: root.sans
                font.pixelSize: root.px(26)
                font.weight: Font.DemiBold
                wrapMode: Text.Wrap
            }
            Hint {
                color: root.ink
                text: "Write this down on paper. It's the only way to get your wallet back if this computer is lost or breaks."
            }
            Hint {
                font.pixelSize: root.px(16)
                text: "Anyone who sees it can take your tokens and names, so don't type it anywhere or share a photo of it. It won't be shown again."
            }
            GridLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                columns: 4
                columnSpacing: root.px(10)
                rowSpacing: root.px(10)
                Repeater {
                    model: root.recoveryPhrase === "" ? [] : root.recoveryPhrase.split(" ")
                    delegate: Rectangle {
                        required property int index
                        required property string modelData
                        Layout.fillWidth: true
                        implicitHeight: wordText.implicitHeight + root.px(16)
                        radius: root.px(6)
                        color: root.slate
                        border.width: 1
                        border.color: root.line
                        Text {
                            id: wordText
                            anchors.verticalCenter: parent.verticalCenter
                            x: root.px(10)
                            text: (index + 1) + "  " + modelData
                            textFormat: Text.PlainText
                            color: root.ink
                            font.family: root.mono
                            font.pixelSize: root.px(16)
                        }
                    }
                }
            }
            Action {
                Layout.topMargin: 8
                label: "I've written it down"
                strong: true
                onPressed: root.recoveryPhrase = ""
            }
        }
    }
}
