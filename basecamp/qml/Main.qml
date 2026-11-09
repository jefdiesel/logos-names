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
    property var record: null          // the name on the plate
    property string forwardedFrom: ""  // the name asked for, when the plate shows where it forwards
    property var page: null
    property string editTab: "profile"   // the Edit screen's tab: profile, page or giveup
    property bool publicRegisterOpen: false
    property bool registering: false       // the "Register a name" screen is open
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
    readonly property string neededModule: "0.15.3"
    // This app's own version, as in basecamp/metadata.json (a module test keeps them equal).
    readonly property string appVersion: "0.15.9"
    property string moduleOutdated: ""   // the loaded module's version when it is too old
    property string lastReply: ""
    property bool showLastReply: false // for finding out why something failed

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property bool busy: pending !== null
    readonly property string holder: record !== null ? holderOf(record.name) : ""
    // Held by the CLI's wallet and not by the open LEZ wallet: changed through the CLI.
    readonly property bool cliHeld: record !== null && holder === "" && cliHolds(record.name)
    readonly property bool mine: holder !== "" || cliHeld
    // "My names": the LEZ wallet's, then the CLI's, each with the signer that applies.
    readonly property var allMine: {
        var all = myNames.map(function(entry) { return Object.assign({ signer: "wallet" }, entry) })
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
    readonly property color noticeFill: Qt.rgba(brass.r, brass.g, brass.b, 0.14)
    readonly property color ownBubble: Qt.rgba(brass.r, brass.g, brass.b, 0.24)

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
        if (what === "names.cli_create_wallet")
            return what + " → (not shown: it holds your recovery phrase)"
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
        var reply = call("names", method, args)
        if (!reply || reply.success === undefined)
            throw new Error("The names module is missing or out of date. In Basecamp, open Applications, click Names and press Install.")
        if (!reply.success)
            throw new Error(reply.error)
        return reply.value
    }

    readonly property string moduleBusy: "The names module is busy and didn't answer in time; it's asked again shortly."

    /// Asks the names module without holding the screen still, for anything
    /// that waits on the wallet. Exactly one of `done(value)` and
    /// `failed(message)` runs, later.
    function askLater(method, args, done, failed, patience) {
        function settle(reply) {
            lastReply = shownReply("names." + method, reply)
            // No reply at all is a call that waited past its time: the module answers one
            // call at a time, so a slow one (a wallet catching up) can hold the rest.
            if (reply === undefined || reply === null || String(reply) === "") {
                failed(moduleBusy)
                return
            }
            reply = parsed(reply)
            if (!reply || reply.success === undefined)
                failed("The names module is missing or out of date. In Basecamp, open Applications, click Names and press Install.")
            else if (!reply.success)
                failed(reply.error)
            else
                done(reply.value, reply)
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

    function say(message, isError) {
        notice = sentence(message)
        noticeIsError = !!isError
    }

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
            var loaded = config.module_version ? String(config.module_version) : ""
            if (!versionAtLeast(loaded, neededModule)) {
                moduleOutdated = loaded === "" ? "older than 0.11" : loaded
                say("The names module is out of date (" + moduleOutdated + "; this app needs " + neededModule + " or newer). In Basecamp, open Applications, click Names and press Install.", true)
                return true
            }
            moduleOutdated = ""
            registry = config.registry ? config.registry : ""
            sequencer = config.sequencer ? config.sequencer : ""
            walletConfig = config.wallet_config ? config.wallet_config : ""
            walletStorage = config.wallet_storage ? config.wallet_storage : ""
            cliPath = config.cli ? config.cli : ""
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
            return true
        }
        if (bridgeReady && logos.callModuleAsync) {
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

    function saveConfig(newSequencer, newRegistry) {
        try {
            if (newSequencer !== sequencer && !call("names", "set_sequencer", [newSequencer]))
                say("That is not an http or https address.", true)
            if (newRegistry !== registry && !call("names", "set_registry", [newRegistry]))
                say("That is not a program ID.", true)
        } catch (error) {
            say(error.message, true)
        }
        loadConfig()
    }

    /// Reads the deposit a name would lock. Until it is known, registering waits.
    function loadDeposit(name) {
        deposit = null
        askLater("deposit", [name], function(value) {
            if (missing === name)
                deposit = value
        }, function(message) {
            console.log("names_ui: deposit:", message)
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
        askLater("cli_mine", [], function(value, reply) {
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
        }, function(message) {
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
        cliBalance = undefined
        askLater("cli_balance", [], function(value, reply) {
            cliBalance = value
            // Balances the sequencer didn't give show as unknown, and this says which.
            balanceNote = Logic.readNote(reply)
            loadActivity()
        }, function(message) {
            cliBalance = null
            balanceNote = sentence(message)
            console.log("names_ui: CLI balance:", message)
        }, 120000)
    }

    property string balanceNote: ""   // what the last balance read said about itself: partial, or why it failed
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
        askLater("cli_accounts", [], function(value, reply) {
            cliAccounts = Logic.accountsShown(value)
            accountsNote = Logic.readNote(reply)
            accountsError = ""
        }, function(message) {
            accountsError = sentence(message)
            console.log("names_ui: accounts:", message)
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
                if (chainFrom === "" && keys.length > 0)
                    chainFrom = keys[0].key
                askLater("chain_stake", [], function(stake) {
                    chainStake = stake
                    chainReading = false
                }, function(message) {
                    chainReading = false
                    console.log("names_ui: stake:", message)
                }, 30000)
            }, function(message) {
                chainReading = false
                if (message === moduleBusy) {
                    chainRetry.start()
                    return
                }
                chainError = sentence(message)
            }, 60000)
        }, function(message) {
            chainReading = false
            if (message === moduleBusy) {
                chainRetry.start()
                return
            }
            chainStatus = null
            chainError = sentence(message)
        }, 20000)
    }

    function chainAsked() {
        return [chainFrom, sendToResolved(), sendAmount.text.trim()]
    }

    /// Checks the send and shows its plan; nothing is sent.
    function chainCheck() {
        var asked = chainAsked()
        var problem = asked[0] === "" ? "Choose a key to send from." : Logic.chainToProblem(asked[1])
        // Commas only between groups of three, as the module reads it: "1,5" is refused, not 15.
        if (problem === "" && !/^([0-9]+|[0-9]{1,3}(,[0-9]{3})+)$/.test(asked[2]))
            problem = "Enter a whole amount, such as 1000 or 1,000."
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
                                                           mainName, mainName)

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
            return bad !== "" ? bad : /^([0-9]+|[0-9]{1,3}(,[0-9]{3})+)$/.test(amount) ? "" : "Enter a whole amount, such as 1000 or 1,000."
        }
        if (/^Private\//.test(to))
            return "A private account can't be paid by its address: nobody but its holder knows its keys. Pay its name, or ask for a public account."
        if (sendKind === "token")
            return tokenSendProblem()
        var bad = payIsAccount(to) ? payAccountProblem(to) : /^[0-9]+$/.test(amount) ? "" : "Enter a whole amount, such as 1000."
        return bad !== "" ? bad : sendBalanceProblem(amount)
    }

    /// Why the account Send pays from can't pay `amount` (digits) and its fee, or "" when it
    /// can or its balance isn't known yet (send-failure-text-raw-and-no-balance-check).
    function sendBalanceProblem(amount) {
        var digits = String(amount).replace(/,/g, "")
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
        payFromPublic = !sendFromPrivate && !sendToName()
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
            return "Paid from the key's notes; shown once it's sent (about 2,000 on the testnet)."
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
        bridgeFromKey = fromKey ? fromKey : bridgeKey !== "" ? bridgeKey : chainAccounts !== null && chainAccounts.length > 0 ? chainAccounts[0].key : ""
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
        var typed = privateMoveField.text.trim().replace(/,/g, "")
        return typed !== "" ? typed : bridgeAsked()[0]
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
        var keys = (chainAccounts || []).map(function(entry) { return entry.key })
        return keys.indexOf(bridgeFromKey) >= 0 ? bridgeFromKey : keys.length > 0 ? keys[0] : ""
    }

    function bridgeAsked() {
        return [bridgeAmountField.text.trim().replace(/,/g, ""), bridgeFrom, bridgeNodeField.text.trim(), bridgeToPrivate ? "" : bridgeTo]
    }

    /// Checks the fields before the module sees them; "" if they look right.
    function bridgeInputProblem(asked) {
        if (!/^[0-9]+$/.test(asked[0]) || /^0+$/.test(asked[0]))
            return "Enter a whole amount, such as 1000."
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
                + grouped(asked) + " off into a note of its own: a send from the key to itself (a fee of about 2,000, in a block within a minute). "
                + "The rest comes back as change. The notes it spends leave the stake, and the new ones join it within two epochs."
        if (bridgePlan.fee_short === true)
            return "That key holds a note of exactly " + grouped(asked) + ", but the deposit fee (at most " + grouped(bridgePlan.fee || "100000")
                + ") comes from its other notes, which hold " + grouped(bridgePlan.others || "0") + ". Mine or receive a little more first."
        return "Ready: exactly " + grouped(asked) + " from one note of that key, into " + (bridgeTo !== "" ? Logic.keyShown(bridgeTo.substr(7)) : "your LEZ public account")
            + ". Network fee at most " + grouped(bridgePlan.fee || "100000") + ", paid from the key's other notes. Nothing has been sent yet."
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
    function loadCliStatus() {
        askLater("cli_status", [], function(value) {
            cliStatus = value
            if (value && value.installing && installJob === null)
                installJob = { id: value.installing.id, started: value.installing.started || Date.now(), line: installLine(value.installing.log_tail) }
            if (value && value.busy && value.busy.kind === "shield" && shieldJob === null)
                shieldJob = { id: value.busy.id, started: value.busy.started || Date.now() }
        }, function(message) {
            console.log("names_ui: CLI status:", message)
        })
    }

    /// Installs the names program and its prover into the module's folder.
    function installCli() {
        if (installJob !== null)
            return
        installProblem = ""
        installWarning = ""
        installJob = { id: "", started: Date.now(), line: "Starting…" }
        askLater("cli_install", [], function(value) {
            if (value.installed) {
                installJob = null
                installWarning = value.prover ? "" : "The prover isn't installed, so private changes can't be made yet."
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
                installProblem = value.problem ? value.problem : "The install stopped: " + lastLogLine(value.log_tail)
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
            restoreDone = "Your wallet is back. The old files were kept beside it."
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
        }, function(message) {
            console.log("names_ui: settings:", message)
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
            claimLine = value.state === "done" ? job.name + " is yours again." : sentence(lastLogLine(value.log_tail))
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
                receiveProblem = value.problem ? value.problem : "Claiming didn't finish: " + lastLogLine(value.log_tail)
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
                shieldProblem = value.problem ? value.problem : "Moving to private didn't go through: " + lastLogLine(value.log_tail)
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
        if (registry === "" || wallet.length === 0) {
            myNames = []
            pendingNames = []
            return
        }
        askLater("mine", [], function(value) {
            myNames = value.names
            pendingNames = value.pending ? value.pending : []
            if (pendingNames.length > 0)
                revealTimer.start()
        }, function(message) {
            console.log("names_ui: my names:", message)
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
        if (revealing || bridgeReady === false)
            return
        for (var i = 0; i < pendingNames.length; i++) {
            var item = pendingNames[i]
            if (item.ready_at !== null && item.ready_at !== undefined && Date.now() < item.ready_at)
                continue
            revealing = true
            askLater("register_name", [item.name, item.owner, "auto"], function(value) {
                revealing = false
                if (value && value.stage === "registered") {
                    say(item.name + " is yours.", false)
                    loadMine()
                    if (record !== null && record.name === item.name)
                        read(item.name)
                } else {
                    loadMine()
                }
            }, function(message) {
                revealing = false
                console.log("names_ui: reveal:", message)
            })
            return
        }
    }

    Timer {
        id: revealTimer
        interval: 60000
        repeat: true
        running: root.pendingNames.length > 0
        onTriggered: root.revealDue()
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.pending !== null
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
        askLater("wallet", [], function(value) {
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
        }, function(message) {
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
                    say("That account has no name.", false)
                    return
                }
                text = known.name
            }
            show(text)
        } catch (error) {
            say(error.message, true)
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
        registering = true
        registerField.text = ""
        registerField.forceActiveFocus()
    }

    /// Checks the name typed on the "Register a name" screen: a free one opens
    /// its registration, a held one is said to be taken.
    function checkRegister(typed) {
        var text = String(typed).trim()
        if (text === "")
            return
        say("", false)
        try {
            var name = String(call("names", "normalize", [text]))
            if (name === "") {
                say("A name is 1 to 32 characters of a-z, 0-9 or a hyphen, such as jef.", true)
                return
            }
            if (ask("resolve", [name]) !== null) {
                registerTaken = name
                return
            }
            show(name)
        } catch (error) {
            say(error.message, true)
        }
    }

    function show(name) {
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
                missing = String(call("names", "normalize", [name]))
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
            say(error.message, true)
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
        say("Asking the wallet to sign.", false)
        askLater(method, args, function(value) {
            job.sent = true
            job.result = value
            say("Sent. Waiting for it to be confirmed.", false)
        }, function(message) {
            pending = null
            say(message, true)
        })
    }

    /// The same change for a name the names CLI holds: the module starts the
    /// CLI, which proves the change on this machine (minutes), and this polls
    /// the job until it ends, then reads the name back.
    readonly property var cliMethods: ({ set_text: "cli_set_text", set_profile: "cli_profile", set_page: "cli_set_page", register: "cli_register", transfer: "cli_transfer" })

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
        say("Starting the names CLI.", false)
        askLater(cliMethod, args, function(value) {
            job.id = value.id
            job.started = value.started || Date.now()
            var skipped = value.skipped && value.skipped.length > 0 ? " (The avatar was left as it is: give the full path of a PNG file to change it.)" : ""
            say(savingText(job) + skipped, false)
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
        return "Proving the change on this computer, so the chain never learns your account. Usually 5–12 min" + sofar + "."
    }

    /// The three steps of a private registration, for the register card and its progress.
    readonly property var registerSteps: [
        { title: "Prove the commitment", time: "5–12 min",
          why: "Your computer builds a zero-knowledge proof. The chain checks it without learning which account is yours." },
        { title: "Wait", time: "until 15 min after step 1",
          why: "The commitment hides which name you picked. Waiting means nobody who sees it can rush to take the name first." },
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
                say(savingText(job), false)
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
            } else {
                say("The names CLI could not save it: " + lastLogLine(value.log_tail), true)
            }
        }, function(message) {
            job.asking = false
            console.log("names_ui: job:", message)
        })
    }

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
            // A failed read is not an answer; the next tick asks again.
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
        say("Asking the wallet to sign.", false)
        askLater("register_name", [name, owner, "auto"], function(value) {
            pending = null
            say("Committed. The name finishes registering in about 15 to 20 minutes; keep Basecamp open.", false)
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
        }, function() {
            if (record !== null && record.name === name)
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
        if (cliHeld)
            return "Yours · held privately"
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
    /// First run: no names CLI yet, or no name to post as.
    readonly property bool needsWelcome: configKnown && registry !== "" && (cliPath === "" || (cliKnown && mainName === "" && cliPending.length === 0))
    // The Welcome screen stands in for every section but Settings (where
    // Register is) and Search (where a name checked there opens to register).
    readonly property bool welcomeCovers: needsWelcome && section !== "settings" && section !== "search" && !(section === "wallet" && walletView === "bridge")

    /// Opens a section of the sidebar; a notice from another section doesn't follow.
    function go(which) {
        say("", false)
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
        boardStart("make_anon", [anonPublic], "Setting up your anon name", function() {
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
        var amount = sendAmount.text.trim()
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
        try { bare = String(call("names", "normalize", [to])) } catch (error) {}
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
        if (target !== "" && payForwardSeen !== to + "→" + target) {
            payForwardSeen = to + "→" + target
            say(to + " forwards to " + target + ", so the payment goes to " + target + ". Press Pay again to send it.", false)
            return
        }
        payForwardSeen = ""
        boardStart("cli_pay", [to, amount], "Your payment to " + (target !== "" ? to + " → " + target : to), function() {
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
                // The module's own words when it has them, never the CLI's "Error:" prefix
                // (send-failure-text-raw-and-no-balance-check).
                var why = value.problem ? String(value.problem) : "It didn't go through: " + lastLogLine(value.log_tail)
                say(sentence(why.replace(/(^|: )Error: /, "$1")), true)
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
            name = String(call("names", "normalize", [typed]))
        } catch (error) {}
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
        if (tokenProgram === "")
            return
        askLater("cli_token_holdings", [""], function(value) {
            tokenHoldings = value.holdings === null || value.holdings === undefined ? null : value.holdings
            tokenUpdating = !!value.updating
            tokenError = value.error ? sentence(value.error) : ""
        }, function(message) {
            tokenUpdating = false
            tokenError = sentence(message)
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
        var amount = Logic.tokenAmountProblem(sendAmount.text)
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
        var amount = sendAmount.text.trim().replace(/[,_]/g, "")
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
                tokenProblem = value.problem ? sentence(value.problem) : "It didn't go through: " + lastLogLine(value.log_tail)
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
    property string appChosenFor: ""      // which request those ticks are for
    property bool appUnderstood: false    // an unverified program's warning was read
    property bool appBusy: false          // an answer is on its way to the module
    property string appError: ""
    property var appGrants: []            // [{app, accounts, since}]
    readonly property var appRequest: appRequests.length > 0 ? appRequests[0] : null

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
            if (appChosenFor !== request.id) {
                appChosenFor = request.id
                appUnderstood = false
                appError = ""
                var choices = request.choices || []
                var given = choices.filter(function(choice) { return choice.granted })
                // Nothing is ticked for an app that may be a fake: choosing is deliberate (M-2).
                appChosen = appPublisherFailed(request) ? [] : given.length > 0 ? [given[0].account] : choices.length > 0 ? [choices[0].account] : []
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
        if (appRequest === null || appBusy || appChosen.length === 0)
            return false
        if (appRequestRisky(appRequest) && !appUnderstood)
            return false
        return true
    }

    /// A program call, or a token transfer, with warnings: its program's
    /// source isn't verified, it goes to the Names registry, the token has no name.
    function appRequestRisky(request) {
        // An app whose publisher check failed may be a fake, whatever it asks (M-2).
        if (appPublisherFailed(request))
            return true
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
                    + (Logic.tokenSendProves(/^Private\//.test(chosen[0] || ""), request.destination) ? "It's proven on this computer first: 7-12 minutes." : "One block, about 30 seconds."), false)
            else if (approve && request.kind === "transfer")
                say("Sending " + grouped(request.amount) + " " + request.asset + " to " + request.destination + " for " + request.app + ". It takes a few minutes.", false)
            else if (approve && request.kind === "call")
                say("Sent for " + request.app + ".", false)
            else if (approve)
                say(request.app + " can now see the accounts you chose.", false)
            loadAppRequests()
            loadAppGrants()
        }, function(message) {
            appBusy = false
            appError = sentence(message)
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

    Timer {
        interval: 3000
        repeat: true
        running: root.bridgeReady && root.configKnown && !root.appBusy
        triggeredOnStart: true
        onTriggered: { root.clockTick = Date.now(); root.loadAppRequests() }
    }

    TextEdit { id: clipboard; visible: false }

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
            color: act.line.sign === "+" ? root.good : root.dim
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
                color: act.line.sign === "+" ? root.good : root.ink
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
                color: act.line.tone === "good" ? root.good : act.line.tone === "bad" ? root.alarm : act.line.tone === "warn" ? root.brass : root.dim
                font.family: root.sans
                font.pixelSize: root.px(12)
            }
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
                        color: stepRow.now ? root.brass : root.ink
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
                        color: root.brass
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
                            text: root.mainName !== "" ? root.mainName : "Welcome"
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
                    Rectangle { width: 8; height: 8; radius: 4; color: root.good }
                    Text { text: root.networkName(); color: root.faint; font.family: root.sans; font.pixelSize: root.px(14) }
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
                Hint { font.pixelSize: root.px(18); text: "Get a name, then post, pay and be paid as it. Four steps, all here." }

                // 1. Install the names program.
                Card {
                    Layout.fillWidth: true
                    StepHead { number: 1; done: root.cliPath !== ""; title: "Install" }
                    Hint {
                        visible: root.cliPath === ""
                        text: "Names needs a small program that proves your changes on this computer, so the chain never learns your account. About 90 MB, checked against the release signature before it's used."
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
                            color: root.brass
                            font.pixelSize: root.px(16)
                            text: root.installJob === null ? "" : root.installJob.line
                        }
                    }
                    Hint { visible: root.installProblem !== ""; color: root.alarm; text: root.installProblem }
                    Hint {
                        visible: root.cliPath !== ""
                        text: "Installed." + (root.cliStatus !== null && root.cliStatus.cli ? " " + root.cliStatus.cli : "")
                        font.pixelSize: root.px(15)
                        wrapMode: Text.WrapAnywhere
                    }
                    Hint { visible: root.installWarning !== ""; color: root.alarm; font.pixelSize: root.px(15); text: root.installWarning }
                    Link {
                        visible: root.cliPath !== "" && root.installWarning !== ""
                        text: "Install the prover"
                        font.pixelSize: root.px(15)
                        onPressed: root.installCli()
                    }
                    // For people who already have the CLI, or want it in Terminal too.
                    Link {
                        visible: root.cliPath === "" && !root.terminalNoteOpen
                        Layout.topMargin: 4
                        text: "Already use the command line?"
                        color: root.dim
                        font.pixelSize: root.px(14)
                        onPressed: root.terminalNoteOpen = true
                    }
                    ColumnLayout {
                        visible: root.cliPath === "" && root.terminalNoteOpen
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
                    Hint {
                        visible: root.cliOutdated
                        color: root.alarm
                        text: "This names program is from before the chain's reset. Install the new one to go on."
                    }
                    Hint {
                        visible: root.oldWallet
                        color: root.alarm
                        text: "Your wallet is from before the chain's reset and no longer opens. Enter its recovery phrase to bring its accounts back. The old files are kept. Tokens don't carry over."
                    }
                    Field {
                        id: restorePhraseField
                        visible: root.oldWallet
                        Layout.fillWidth: true
                        hint: "Your recovery phrase"
                    }
                    Action {
                        visible: root.oldWallet
                        label: root.restoreBusy ? "Restoring…" : "Bring my wallet back"
                        strong: true
                        enabledNow: !root.restoreBusy
                        onPressed: { root.restoreWallet(restorePhraseField.text); restorePhraseField.text = "" }
                    }
                    Hint { visible: root.restoreError !== ""; color: root.alarm; text: root.restoreError }
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
                    Hint { visible: root.walletError !== ""; color: root.alarm; text: root.walletError }
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
                        visible: root.hasCliWallet && !root.canShield && !root.hasPrivateFunds
                        font.pixelSize: root.px(16)
                        text: "Get " + root.grouped(root.faucetAmount) + " free test tokens to start: your name's deposit, and the fees to hold it privately."
                    }
                    Action {
                        visible: root.hasCliWallet && !root.canShield && !root.hasPrivateFunds && !root.faucetSent
                        label: root.faucetBusy ? "Asking…" : "Get free tokens"
                        strong: true
                        enabledNow: !root.faucetBusy
                        onPressed: root.getFreeTokens()
                    }
                    Hint {
                        visible: root.faucetSent && !root.canShield
                        font.pixelSize: root.px(16)
                        color: root.brass
                        text: "Sent. They arrive in a minute or two; this checks by itself."
                    }
                    Hint { visible: root.faucetError !== ""; color: root.alarm; font.pixelSize: root.px(16); text: root.faucetError }
                    RowLayout {
                        visible: root.hasCliWallet
                        spacing: 18
                        Link { text: "Or bridge from my Logos node"; font.pixelSize: root.px(16); onPressed: root.openGetTokens() }
                        Link { text: "Check again"; color: root.dim; font.pixelSize: root.px(16); onPressed: root.loadCliBalance() }
                    }
                }

                // 3. Move tokens to the private account.
                Card {
                    Layout.fillWidth: true
                    opacity: root.hasCliWallet ? 1 : 0.5
                    StepHead { number: 3; done: root.hasPrivateFunds && root.shieldJob === null; title: "Move to private" }
                    Hint {
                        text: "Moves " + (root.canShield ? root.grouped(root.shieldAmount) : "your tokens") + " from your public account to your private one, so your names and payments stay private. Your computer proves it, usually 5–12 minutes."
                    }
                    Hint {
                        visible: root.hasCliWallet && root.cliPublic !== null && !root.canShield && root.shieldJob === null && !root.hasPrivateFunds
                        font.pixelSize: root.px(16)
                        color: root.brass
                        text: root.cliPublic === null ? "" : "Your public account holds " + root.grouped(root.cliPublic.balance) + ". It needs at least " + root.grouped(root.shieldMinimum) + " first: "
                            + (root.cliOutdated ? "a few names' deposits." : "a name's 1,000,000 deposit plus two private transactions' fees, about 5,100,000 each.")
                    }
                    Action {
                        label: root.shieldJob !== null ? "Moving…" : "Move to private"
                        strong: root.canShield
                        enabledNow: root.hasCliWallet && root.canShield && root.shieldJob === null
                        onPressed: root.moveToPrivate()
                    }
                    Hint { visible: root.shieldJob !== null; color: root.brass; font.pixelSize: root.px(16); text: root.shieldLine() }
                    Hint { visible: root.shieldProblem !== "" && root.shieldJob === null; color: root.alarm; text: root.shieldProblem }
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
                    StepHead { number: 4; done: false; title: "Pick your name" }
                    Hint {
                        text: "Held privately: nobody can find which account holds it. It takes a deposit (1,000,000 for most names, more for 3–4 characters), returned when you release it; if you transfer it, the deposit goes with it, and about 20 minutes to register."
                    }
                    Action {
                        label: "Choose my name"
                        strong: root.hasPrivateFunds
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
                        Text { Layout.fillWidth: true; text: "Messaging isn't running yet. Open the Chat app once, then come back."; color: root.brass; font.family: root.sans; font.pixelSize: root.px(15); wrapMode: Text.Wrap }
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
                    color: root.brass
                    text: "Messaging is starting up (network: " + root.chatState + "). Once it's connected you can turn on messages for " + (root.mainName !== "" ? root.mainName : "your name") + "."
                }
                Hint {
                    visible: root.chatError === "" && root.mainChat !== "" && root.mainChat === root.chatMine
                    font.pixelSize: root.px(14)
                    text: "Messages are on: anyone who looks up " + root.mainName + " can message you here."
                }
                Hint {
                    visible: root.notice !== "" && root.section === "messages"
                    color: root.noticeIsError ? root.alarm : root.dim
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
                    Layout.maximumWidth: 680
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
                    Hint { visible: root.sendingLine() !== ""; color: root.brass; font.pixelSize: root.px(16); text: root.sendingLine() }
                    Hint { visible: root.notice !== "" && root.sendingLine() === ""; color: root.noticeIsError ? root.alarm : root.dim; font.pixelSize: root.px(16); text: root.notice }
                    // Payments to your names' private keys wait until you claim them (SC-1).
                    Card {
                        visible: root.waitingPayments !== null || root.receiveJob !== null || root.receiveProblem !== ""
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
                        Hint { visible: root.receiveJob !== null; color: root.brass; font.pixelSize: root.px(15); text: "Proving the claim on this computer. Several minutes for each payment." }
                        Hint { visible: root.receiveProblem !== "" && root.receiveJob === null; color: root.alarm; font.pixelSize: root.px(15); text: root.receiveProblem }
                    }
                    // A read the sequencer only partly answered, or why it failed: never "no balances".
                    Hint { visible: root.balanceNote !== "" && root.cliBalance !== undefined; color: root.brass; font.pixelSize: root.px(15); text: root.balanceNote }
                    Hint {
                        visible: root.walletGroups.length === 0 && (root.balanceNote === "" || root.cliBalance === undefined)
                        text: root.cliBalance === undefined ? "Reading your balances…" : "No balances yet. Make a wallet in Settings, or start your node in Basecamp's Blockchain app."
                    }

                    // One card per chain: its total, then (open) one row per address.
                    Repeater {
                        model: root.walletGroups
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
                            }
                        }
                    }

                    // Tokens sent to your name's private keys, waiting to be taken in.
                    Rectangle {
                        visible: root.tokenProgram !== "" && root.tokenPending.length > 0
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
                        Layout.topMargin: 6
                        spacing: 10
                        Action { visible: !root.cliOutdated; label: "Add a token"; onPressed: { root.addTokenProblem = ""; root.walletView = "add"; addTokenField.forceActiveFocus() } }
                        Item { Layout.fillWidth: true }
                    }
                    Hint {
                        font.pixelSize: root.px(14)
                        text: "Private accounts are seen only by you; public ones and the Logos Blockchain by anyone. LEZ and the Logos Blockchain are separate chains with their own addresses; the bridge moves tokens into LEZ."
                    }
                }

                // ── Send ──
                ColumnLayout {
                    visible: root.walletView === "send"
                    Layout.fillWidth: true
                    Layout.maximumWidth: 680
                    spacing: 12
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
                    Text { text: root.sendKind === "chain" ? "Send on the Logos Blockchain" : "Send " + root.sendTitle; textFormat: Text.PlainText; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }

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
                    Hint {
                        readonly property string problem: root.sendProblem() !== "" ? root.sendProblem() : root.sendKind === "chain" ? root.chainProblem : root.sendKind === "token" ? root.tokenProblem : ""
                        visible: problem !== ""
                        color: root.alarm
                        font.pixelSize: root.px(16)
                        textFormat: Text.PlainText
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
                        Fact { label: "Amount"; value: root.grouped(sendAmount.text.trim().replace(/,/g, "")) + (root.sendKind === "chain" ? "" : " " + root.sendTitle) }
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
                    Hint { visible: root.sendingLine() !== ""; color: root.brass; font.pixelSize: root.px(16); text: root.sendingLine() }
                    Hint {
                        visible: root.sendKind === "chain" && root.chainSent !== null && root.chainSent.state === "in_block"
                        color: root.good
                        font.pixelSize: root.px(16)
                        text: root.chainSent === null ? "" : "Sent " + root.grouped(root.chainSent.amount) + ": it's in a block." + (root.chainSent.fee ? " The fee was " + root.grouped(root.chainSent.fee) + "." : "")
                    }
                    Hint { visible: root.notice !== "" && root.sendingLine() === ""; color: root.noticeIsError ? root.alarm : root.dim; font.pixelSize: root.px(16); text: root.notice }
                }

                // ── Receive ──
                ColumnLayout {
                    visible: root.walletView === "receive"
                    Layout.fillWidth: true
                    spacing: 14
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
                    Text { text: root.receiveKind === "chain" ? "Receive on the Logos Blockchain" : "Receive LEZ and tokens"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }

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
                                Text { visible: modelData.name === root.mainName; text: "main"; color: root.brass; font.family: root.sans; font.pixelSize: root.px(14) }
                                Link { text: "Copy"; onPressed: root.copy(modelData.name, "Your name") }
                            }
                        }
                        Hint {
                            text: root.cliNames.length > 0 ? "Anyone can pay any of these from Names. It arrives privately, in that name's own private account, and the chain doesn't show who paid you or how much."
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
                        model: root.receiveKind === "chain" && root.chainAccounts !== null ? root.chainAccounts : []
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
                    Hint {
                        visible: root.receiveKind === "chain" && root.chainAccounts === null
                        color: root.alarm
                        text: root.chainError !== "" ? root.chainError : "Reading your node…"
                    }
                }

                // ── Stake: the amounts, then how it works and how to unstake ──
                ColumnLayout {
                    visible: root.walletView === "stake"
                    readonly property var table: Logic.stakeTable(root.chainStake, root.chainAccounts)
                    id: stakePage
                    Layout.fillWidth: true
                    spacing: 14
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
                    RowLayout {
                        Layout.fillWidth: true
                        Text { text: "Staking"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                        Link { text: root.chainReading ? "Reading…" : "Refresh"; color: root.faint; font.pixelSize: root.px(15); onPressed: root.loadChain() }
                    }
                    Hint { visible: root.chainError !== ""; color: root.alarm; textFormat: Text.PlainText; text: root.chainError }
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
                                    Hint { font.pixelSize: root.px(14); color: root.brass; text: "One way for now: LEZ can't send back to the Logos Blockchain yet. Arrives in about 12 hours." }
                                    Action { label: "Bridge…"; onPressed: root.openGetTokens() }
                                }
                            }
                        }
                    }
                    Hint { visible: root.chainProblem !== ""; color: root.alarm; textFormat: Text.PlainText; text: root.chainProblem }
                    Hint { visible: root.notice !== ""; color: root.noticeIsError ? root.alarm : root.dim; font.pixelSize: root.px(16); text: root.notice }
                }

                // ── Names: register one, and the ones you hold ──
                ColumnLayout {
                    visible: root.walletView === "names"
                    Layout.fillWidth: true
                    Layout.maximumWidth: 680
                    spacing: 14
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
                    Text { text: "Names"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
                    Hint { text: "Your wallet can hold many names. Names held privately can't be linked to each other on chain; a name held publicly shows its holder. Each name is paid into its own private account unless it publishes a public one." }

                    Card {
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "Register a name" }
                        Hint { text: "Pick a name people can pay and message you by. It's private from the start: the chain never learns which account holds it." }
                        Action { label: "Register a name"; strong: true; onPressed: root.openRegister() }
                    }

                    Card {
                        Layout.fillWidth: true
                        Part { Layout.topMargin: 0; text: "My names" }
                        Hint { visible: root.namesNote !== ""; color: root.brass; text: root.namesNote }
                        Hint { visible: root.cliNames.length === 0 && root.cliPending.length === 0; text: root.cliKnown ? "None yet." : "Reading your names…" }
                        Repeater {
                            model: root.cliNames
                            RowLayout {
                                id: myName
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 14
                                Text { text: myName.modelData.name; color: root.ink; font.family: root.sans; font.pixelSize: root.px(22); font.weight: Font.Bold }
                                Text { visible: myName.modelData.name === root.mainName; text: "main"; color: root.brass; font.family: root.sans; font.pixelSize: root.px(14) }
                                Text {
                                    text: myName.modelData["private"] === false ? "Public" : "Private"
                                    color: myName.modelData["private"] === false ? root.dim : root.brass
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
                                Link { visible: myName.modelData["private"] !== false; text: "Make public…"; font.pixelSize: root.px(15); onPressed: { root.go("settings"); root.loadAccounts() } }
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
                                Text { text: modelData.failed ? "didn't register" : "registering"; color: modelData.failed ? root.alarm : root.brass; font.family: root.sans; font.pixelSize: root.px(14) }
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
                        Hint { visible: root.transferProblem() !== ""; color: root.alarm; font.pixelSize: root.px(14); text: root.transferProblem() }
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

                    // Take a name someone handed to you (names receive-privately).
                    Card {
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
                        color: root.brass
                        font.pixelSize: root.px(15)
                        text: root.boardJob === null ? "" : root.boardJob.label + ": proving it on this computer. Usually 5–12 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                    }
                    Hint { visible: root.notice !== "" && root.walletView === "names"; color: root.noticeIsError ? root.alarm : root.good; font.pixelSize: root.px(15); text: root.notice }
                }

                // ── Add a token by its address ──
                ColumnLayout {
                    visible: root.walletView === "add"
                    Layout.fillWidth: true
                    spacing: 12
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
                    Text { text: "Add a token"; color: root.ink; font.family: root.sans; font.pixelSize: root.px(28); font.weight: Font.DemiBold }
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
                    Hint { visible: root.addTokenProblem !== ""; color: root.alarm; textFormat: Text.PlainText; text: root.addTokenProblem }

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
                    readonly property var mine: Logic.activityFor(root.activityRows, root.rowAccount(row))
                    Layout.fillWidth: true
                    Layout.maximumWidth: 680
                    spacing: 12
                    Link { text: "‹ Wallet"; color: root.dim; font.pixelSize: root.px(15); onPressed: root.walletView = "home" }
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
                        visible: accountPage.row !== null && accountPage.row.address !== undefined && accountPage.row.address !== ""
                        label: "Address"
                        code: true
                        value: accountPage.row ? (accountPage.row.address.length > 50 ? Logic.keyShown(accountPage.row.address) : accountPage.row.address) : ""
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
                        Action { label: "Receive"; onPressed: root.openReceive(accountPage.row) }
                        Action {
                            visible: accountPage.row !== null && (accountPage.row.kind === "chain" || accountPage.row.kind === "lez")
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
                    Hint { visible: accountPage.send.switchFirst; font.pixelSize: root.px(14); text: "Send makes this the account payments come from, then opens the form." }
                    Hint { visible: root.accountsError !== ""; color: root.alarm; font.pixelSize: root.px(14); text: root.accountsError }
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
                    Layout.maximumWidth: 680
                    spacing: 10
                    Link {
                        text: "‹ Wallet"
                        color: root.dim
                        font.pixelSize: root.px(15)
                        onPressed: root.walletView = "home"
                    }
                    Text { text: "Bridge"; color: root.ink; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: root.px(28) }
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
                            model: root.chainAccounts || []
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
                    Hint {
                        visible: root.chainAccounts === null || root.chainAccounts.length === 0
                        color: root.alarm
                        font.pixelSize: root.px(15)
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
                        Hint { visible: root.accountsError !== ""; color: root.alarm; font.pixelSize: root.px(14); text: root.accountsError }

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
                        // The node address: kept for the module call, no longer shown (one node: Basecamp's).
                        Field { id: bridgeNodeField; visible: false }
                        Hint {
                            visible: root.bridgeToPrivate && root.bridgeAmountShort()
                            color: root.brass
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
                            color: root.brass
                            font.pixelSize: root.px(15)
                            text: "Making the note: waiting for your node's next blocks. The bridge is checked again as soon as it lands."
                        }
                        Card {
                            visible: root.bridgeConfirm && root.bridgePlan !== null
                            Layout.fillWidth: true
                            Hint {
                                color: root.ink
                                text: root.bridgePlan === null ? "" : "This moves exactly " + root.grouped(root.bridgeAsked()[0]) + " from your Logos Blockchain key into LEZ"
                                    + (root.bridgeToPrivate ? " (your main public account, then you move it to private)" : "")
                                    + ", through LEZ's deposit channel " + Logic.keyShown(root.bridgePlan.channel || "") + " (your node knows it). It can't be undone. It arrives in about 12 hours."
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
                        Hint { visible: root.bridgeError !== ""; color: root.alarm; font.pixelSize: root.px(15); text: root.bridgeError }
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
                            color: root.cliBalance === undefined ? root.dim : root.alarm
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
                        Hint { visible: root.walletError !== ""; color: root.alarm; text: root.walletError }
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
                        Hint { visible: root.shieldJob !== null; color: root.brass; font.pixelSize: root.px(15); text: root.shieldLine() }
                        Hint { visible: root.shieldMoved && root.shieldJob === null; color: root.good; font.pixelSize: root.px(15); text: "Moved. Your private account is ready for names." }
                        Hint { visible: root.shieldProblem !== "" && root.shieldJob === null; color: root.alarm; font.pixelSize: root.px(15); text: root.shieldProblem }
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
                    Link {
                        visible: root.boardView === "thread"
                        text: "← Back"
                        color: root.faint
                        font.underline: false
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
                            color: root.brass
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
                        color: root.heldPublicly(root.boardAs) ? root.faint : root.brass
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
                            Tab { label: "Private anon (4 min posts)"; current: !root.anonPublic; onPressed: root.anonPublic = false }
                            Tab { label: "Public anon (30 s posts)"; current: root.anonPublic; onPressed: root.anonPublic = true }
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
                        Text { Layout.fillWidth: true; text: "Replying to " + root.boardPreviewOf(root.boardReplyTo); color: root.brass; font.family: root.sans; font.pixelSize: root.px(15); elide: Text.ElideRight }
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
                                    color: root.alarm
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
                    color: root.brass
                    font.pixelSize: root.px(16)
                    text: root.boardJob === null ? "" : root.boardJobLine(root.boardJob)
                }
                Hint {
                    visible: root.boardJob === null && root.notice !== "" && root.section === "posts"
                    color: root.noticeIsError ? root.alarm : root.dim
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
                                        color: liked ? root.alarm : root.faint
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
                            Text { Layout.fillWidth: true; text: root.pendingLine(modelData); color: modelData.failed ? root.alarm : root.brass; font.family: root.sans; font.pixelSize: root.px(14); elide: Text.ElideRight }
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
                                color: root.brass
                                font.pixelSize: root.px(14)
                                text: "About 9 minutes. It moves the name to a new public account and funds that with 10,000,000 for gas from your private account, so the chain can't see where the money came from."
                                    + (nameRow.isAnon ? " Keep the anon private in practice: never send between its account and your other public accounts, and don't make it public right after your main name." : "")
                            }
                        }
                    }
                    Hint {
                        visible: root.boardJob !== null && root.section === "settings"
                        color: root.brass
                        text: root.boardJob === null ? "" : root.boardJob.label + ": about 9 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                    }
                    Action {
                        Layout.topMargin: 6
                        label: "+  Register another name"
                        strong: true
                        onPressed: root.openRegister()
                    }
                    Part { text: "Accounts" }
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
                                color: root.brass
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
                    Hint { visible: root.accountsError !== "" && !root.cliOld; color: root.alarm; text: root.accountsError }
                    Hint { visible: root.accountsNote !== "" && root.accountsError === ""; color: root.brass; text: root.accountsNote }
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
                        text: root.cliAccounts === null ? "These are the two accounts your names program pays from. Seeing every account, and making or naming more, comes with its LEZ 0.4 update."
                            : "Making, naming and choosing accounts comes with your names program's LEZ 0.4 update."
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
                    Part { text: "Sequencer" }
                    Field {
                        id: sequencerField
                        Layout.fillWidth: true
                        size: 17
                        text: root.sequencer
                        hint: "https://testnet.lez.logos.co"
                    }
                    // Saved only on purpose: a slip here points every read and change elsewhere (M-7).
                    RowLayout {
                        visible: !root.networkConfirm
                        spacing: 8
                        Action {
                            label: "Save"
                            enabledNow: sequencerField.text.trim() !== root.sequencer || registryField.text.trim() !== root.registry
                            onPressed: root.networkConfirm = true
                        }
                        Link {
                            visible: sequencerField.text.trim() !== root.sequencer || registryField.text.trim() !== root.registry
                            text: "Undo"
                            font.pixelSize: root.px(16)
                            onPressed: { sequencerField.text = root.sequencer; registryField.text = root.registry }
                        }
                    }
                    Card {
                        visible: root.networkConfirm
                        Layout.fillWidth: true
                        Hint {
                            color: root.ink
                            text: "Switch to " + (sequencerField.text.trim() !== root.sequencer ? "the sequencer " + sequencerField.text.trim() : "")
                                + (sequencerField.text.trim() !== root.sequencer && registryField.text.trim() !== root.registry ? " and " : "")
                                + (registryField.text.trim() !== root.registry ? "the registry " + Logic.keyShown(registryField.text.trim()) : "")
                                + "? Names, balances and every change you make then go there. Only switch to one you trust."
                        }
                        RowLayout {
                            spacing: 8
                            Action {
                                label: "Yes, switch"
                                strong: true
                                onPressed: { root.networkConfirm = false; root.saveConfig(sequencerField.text.trim(), registryField.text.trim()) }
                            }
                            Action {
                                label: "Cancel"
                                onPressed: { root.networkConfirm = false; sequencerField.text = root.sequencer; registryField.text = root.registry }
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

                Text {
                    visible: root.notice !== ""
                    Layout.fillWidth: true
                    text: root.notice
                    color: root.noticeIsError ? root.alarm : root.brass
                    font.family: root.sans
                    font.weight: Font.DemiBold
                    font.pixelSize: root.px(18)
                    wrapMode: Text.Wrap
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
                    Hint {
                        font.pixelSize: root.px(18)
                        text: "Pick a name people can use to find your profile and page. Held privately, with a deposit you get back when you release it."
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
                            label: "Check"
                            strong: true
                            enabledNow: registerField.text.trim() !== ""
                            onPressed: root.checkRegister(registerField.text)
                        }
                    }
                    RowLayout {
                        visible: root.registerTaken !== ""
                        spacing: 12
                        Rectangle { width: 10; height: 10; radius: 5; color: root.alarm }
                        Text {
                            text: root.registerTaken + " is taken. Try another."
                            color: root.alarm
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
                        text: "1 to 32 letters, digits or hyphens. One- and two-letter names are reserved."
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
                        Rectangle { width: 10; height: 10; radius: 5; color: parent.reserved ? root.alarm : parent.underway ? root.brass : root.good }
                        Text {
                            text: parent.underway ? "Registering privately for you" : parent.reserved ? "Reserved: names of one to four characters and the reserved list are placed by the registry's admin" : "Available"
                            color: parent.reserved ? root.alarm : parent.underway ? root.brass : root.good
                            font.family: root.sans
                            font.pixelSize: root.px(19)
                        }
                    }
                    // A private registration through the names CLI, under way or stopped.
                    Hint {
                        visible: root.cliPendingFor(root.missing) !== null && root.cliPendingFor(root.missing).failed
                        font.pixelSize: root.px(18)
                        color: root.alarm
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
                        Hint {
                            visible: root.deposit !== null && root.deposit.claims_until !== null && root.deposit.claims_until !== undefined
                            color: root.alarm
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
                    Card {
                        visible: root.cliPath !== "" && !(root.deposit !== null && root.deposit.tier === "locked")
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
                            color: root.cliBalance === null || !root.cliCanPay() ? root.alarm : root.dim
                            font.family: root.sans
                            font.pixelSize: root.px(15)
                            wrapMode: Text.Wrap
                        }
                        Hint {
                            visible: root.cliBalance !== undefined && root.cliBalance !== null && !root.cliCanPay()
                            color: root.alarm
                            font.pixelSize: root.px(15)
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
                        visible: root.cliPath !== "" && root.wallet.length > 0 && !(root.deposit !== null && root.deposit.tier === "locked")
                        text: root.publicRegisterOpen ? "Hide the public option" : "Register from a public account instead"
                        color: root.faint
                        font.pixelSize: root.px(16)
                        onPressed: root.publicRegisterOpen = !root.publicRegisterOpen
                    }
                    Panel {
                        visible: root.wallet.length > 0 && (root.cliPath === "" || root.publicRegisterOpen)
                        title: root.cliPath !== "" ? "From a public account" : "Register it to"
                        Accounts {}
                        Hint {
                            color: root.cliPath !== "" ? root.dim : root.brass
                            text: "Anyone who knows the name can find this account, and so its balance and history." + (root.cliPath !== "" ? "" : " To hide it, install the names CLI and register privately.") + " Payments to the name still go to your private account."
                        }
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
                                color: root.brass
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
                            Link {
                                visible: root.section !== "profile"
                                text: "← Back"
                                color: root.faint
                                font.underline: false
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

                        Hint {
                            visible: root.cliHeld && root.busy && root.notice !== ""
                            color: root.noticeIsError ? root.alarm : root.brass
                            text: root.notice
                        }
                        Hint {
                            visible: root.cliHeld && !root.busy
                            color: root.brass
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
                                    Text { text: root.cliHeld ? "Picture removed when you save, unless you give a PNG file below" : "Picture removed when you save"; color: root.alarm; font.family: root.sans; font.pixelSize: root.px(17); wrapMode: Text.WordWrap; Layout.fillWidth: true }
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
                                        color: root.brass
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
                                        color: root.alarm
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
                                        color: liked ? root.alarm : root.faint
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
        text: "Names " + root.appVersion
        color: root.dim
        font.family: root.sans
        font.pixelSize: root.px(15)
    }

    // Another app's request, over everything but a recovery phrase, until the
    // person answers it: who asks, the account to use, then what it will do.
    Rectangle {
        id: appSheet
        visible: root.appRequest !== null
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
                Text {
                    Layout.fillWidth: true
                    text: root.appRequestTitle(appSheet.request)
                    textFormat: Text.PlainText
                    color: root.ink
                    font.family: root.sans
                    font.pixelSize: root.px(24)
                    font.weight: Font.DemiBold
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                }
                Hint {
                    font.pixelSize: root.px(15)
                    text: appSheet.request === null ? "" : "Asked by the app “" + appSheet.request.app + "”, as Basecamp names it. "
                        + root.appExpiresIn(appSheet.request) + (root.appRequests.length > 1 ? " " + (root.appRequests.length - 1) + " more waiting." : "")
                }
                // Who publishes the app, checked against its LNS name by the Program Registry.
                Text {
                    readonly property var line: appSheet.request === null ? null : Logic.publisherLine(appSheet.request.publisher)
                    visible: line !== null
                    Layout.fillWidth: true
                    text: line === null ? "" : line.text
                    textFormat: Text.PlainText
                    color: line === null ? root.dim : line.tone === "good" ? root.good : line.tone === "warn" ? root.alarm : root.dim
                    font.family: root.sans
                    font.pixelSize: root.px(16)
                    font.weight: line !== null && line.tone !== "plain" ? Font.DemiBold : Font.Normal
                    wrapMode: Text.WordWrap
                }

                // Access: why, and what sharing means.
                Hint {
                    visible: appSheet.kind === "connect" && appSheet.request.reason !== ""
                    textFormat: Text.PlainText
                    color: root.ink
                    text: appSheet.kind === "connect" ? "It says: “" + appSheet.request.reason + "”" : ""
                }
                Hint {
                    visible: appSheet.kind === "connect"
                    text: "Choose the accounts it may see. It will see their addresses and balances, and can then ask you to approve payments. It can't send anything without you."
                }

                // A token transfer: in one line, then the token, its program and how that program stands.
                Hint {
                    visible: root.appIsToken(appSheet.request)
                    textFormat: Text.PlainText
                    color: root.ink
                    font.pixelSize: root.px(19)
                    font.weight: Font.DemiBold
                    text: root.appTokenLine(appSheet.request)
                }
                ColumnLayout {
                    visible: root.appIsToken(appSheet.request)
                    Layout.fillWidth: true
                    spacing: 6
                    Fact { label: "Token"; value: root.appIsToken(appSheet.request) ? Logic.tokenShown(appSheet.request.token_name) + " (named by its maker; not LEZ)" : "" }
                    Fact { label: "Token ID"; code: true; value: root.appIsToken(appSheet.request) ? root.shorten(appSheet.request.asset) : ""; full: root.appIsToken(appSheet.request) ? appSheet.request.asset : "" }
                    Fact {
                        label: "Program"
                        value: !root.appIsToken(appSheet.request) ? ""
                            : appSheet.request.program_label && appSheet.request.program_label.text ? appSheet.request.program_label.text
                            : root.shorten(appSheet.request.token_program)
                        full: root.appIsToken(appSheet.request) ? appSheet.request.token_program : ""
                    }
                    Fact { label: "Source"; value: root.appIsToken(appSheet.request) ? root.programStatus(appSheet.request.program_label) : "" }
                    Fact {
                        label: "Time"
                        value: !root.appIsToken(appSheet.request) ? ""
                            : Logic.tokenSendProves(/^Private\//.test(root.appChosen[0] || ""), appSheet.request.destination)
                            ? "Proven on this computer first: 7-12 minutes"
                            : "One block, about 30 seconds; public: everyone sees who sent it"
                    }
                    Repeater {
                        model: root.appIsToken(appSheet.request) ? (appSheet.request.warnings || []) : []
                        Hint {
                            required property string modelData
                            textFormat: Text.PlainText
                            color: root.alarm
                            text: "!  " + modelData
                        }
                    }
                }

                // A payment: asset, amount, destination.
                ColumnLayout {
                    visible: appSheet.kind === "transfer"
                    Layout.fillWidth: true
                    spacing: 6
                    Fact { visible: !root.appIsToken(appSheet.request); label: "Asset"; value: appSheet.kind === "transfer" ? appSheet.request.asset + " (the native token)" : "" }
                    Fact { label: "Amount"; value: appSheet.kind === "transfer" ? root.appSends(appSheet.request) : "" }
                    Fact {
                        label: "To"
                        code: appSheet.kind === "transfer" && appSheet.request.destination_kind === "account"
                        value: appSheet.kind !== "transfer" ? "" : appSheet.request.destination_kind === "name" ? appSheet.request.destination + " (a name)" : appSheet.request.destination
                        full: appSheet.kind === "transfer" ? appSheet.request.destination : ""
                    }
                    Fact {
                        label: "Network fee"
                        value: {
                            if (appSheet.kind !== "transfer")
                                return ""
                            var fromPrivate = /^Private\//.test(root.appChosen[0] || "")
                            // A token transfer runs the token program: its gas is sized when it's tried.
                            if (root.appIsToken(appSheet.request) && !Logic.tokenSendProves(fromPrivate, appSheet.request.destination))
                                return "Sized when it's tried, just before sending"
                            var estimate = Logic.feeEstimate(root.feeQuote, fromPrivate || appSheet.request.destination_kind === "name")
                            return estimate !== "" ? estimate : "Reading the chain's fees…"
                        }
                    }
                }

                // A program call: what it is, what it can do, what the app says.
                ColumnLayout {
                    visible: appSheet.kind === "call"
                    Layout.fillWidth: true
                    spacing: 6
                    Fact {
                        label: "Program"
                        value: appSheet.kind !== "call" ? "" : appSheet.request.program_label && appSheet.request.program_label.text ? appSheet.request.program_label.text : root.shorten(appSheet.request.program)
                    }
                    Fact { label: "Program ID"; code: true; value: appSheet.kind === "call" ? root.shorten(appSheet.request.program) : ""; full: appSheet.kind === "call" ? appSheet.request.program : "" }
                    Fact {
                        label: "Source"
                        value: appSheet.kind === "call" ? root.programStatus(appSheet.request.program_label) : ""
                    }
                    Fact { label: "Data"; value: appSheet.kind === "call" ? appSheet.request.instruction_len + " bytes" : "" }
                    Fact { label: "Gas"; value: "Sized when it's tried, just before sending; if the call would fail, nothing is sent" }
                    Part { text: "What it can do" }
                    Repeater {
                        model: appSheet.kind === "call" ? (appSheet.request.effects || []) : []
                        Hint {
                            required property string modelData
                            textFormat: Text.PlainText
                            color: root.ink
                            text: "•  " + modelData
                        }
                    }
                    Repeater {
                        model: appSheet.kind === "call" ? (appSheet.request.warnings || []) : []
                        Hint {
                            required property string modelData
                            textFormat: Text.PlainText
                            color: root.alarm
                            text: "!  " + modelData
                        }
                    }
                    Hint {
                        visible: appSheet.kind === "call" && appSheet.request.app_summary !== ""
                        textFormat: Text.PlainText
                        font.pixelSize: root.px(15)
                        text: appSheet.kind === "call" ? "The app describes it as: “" + appSheet.request.app_summary + "” (its own words, not checked)" : ""
                    }
                }

                // The account to use.
                Part { text: appSheet.kind === "connect" ? "Accounts it may see" : appSheet.kind === "call" ? "Sign with" : "Pay from" }
                Repeater {
                    model: appSheet.request !== null ? (appSheet.request.choices || []) : []
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
                                radius: appSheet.kind === "connect" ? 4 : 9
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
                Hint {
                    visible: appSheet.request !== null && (appSheet.request.choices || []).length === 0
                    color: root.alarm
                    text: appSheet.kind === "call" ? "No account can sign this yet. Your wallet's public accounts are being read; if none shows, create your wallet first (Welcome, Create my wallet)."
                        : "Your wallet has no accounts yet. Create your wallet first (Welcome, Create my wallet)."
                }

                // A risky call needs a second, deliberate tick.
                RowLayout {
                    visible: appSheet.request !== null && root.appRequestRisky(appSheet.request)
                    Layout.fillWidth: true
                    spacing: 10
                    Rectangle {
                        Layout.preferredWidth: 18
                        Layout.preferredHeight: 18
                        Layout.alignment: Qt.AlignTop
                        radius: 4
                        color: root.appUnderstood ? root.alarm : "transparent"
                        border.width: 2
                        border.color: root.alarm
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.appUnderstood = !root.appUnderstood }
                    }
                    Hint {
                        color: root.ink
                        text: root.appIsToken(appSheet.request) ? "I've read the warnings above and still want to send this token."
                            : "I've read the warnings above and trust " + (appSheet.request !== null ? appSheet.request.app : "this app") + " with this account."
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.appUnderstood = !root.appUnderstood }
                    }
                }

                Hint { visible: root.appError !== ""; color: root.alarm; textFormat: Text.PlainText; text: root.appError }
                Hint { visible: root.appBusy; color: root.brass; text: appSheet.kind === "call" ? "Trying it, then signing and sending it. This waits for its block, up to two minutes…" : "One moment…" }
                RowLayout {
                    Layout.topMargin: 8
                    spacing: 10
                    Action {
                        label: appSheet.kind === "connect" ? "Share" : appSheet.kind === "call" ? "Approve and sign" : "Approve and send"
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
