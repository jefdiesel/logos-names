// Logos Names for Basecamp: find, register and manage names.
//
// A front end on the `names` module. The module reads the chain itself, so
// browsing needs no wallet. Registering a name, changing it, transferring it
// and deleting it are signed by the LEZ wallet (`lez_core`), which the module
// asks on this screen's behalf. Paying goes to `lez_core` directly and
// messaging through `chat_module`.
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
    property var page: null
    property string editTab: "profile"   // the Edit screen's tab: profile, page or giveup
    property bool publicRegisterOpen: false
    property bool registering: false       // the "Register a name" screen is open
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
    property string section: "posts"   // posts, pay, profile, search or settings
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
    property bool confirmingRelease: false
    property var pending: null         // a change that was sent and is not confirmed yet
    property string registry: ""
    property string sequencer: ""
    property string walletConfig: ""   // the wallet files the names module opens by itself
    property string walletStorage: ""
    property bool wokeWallet: false
    property var deposit: null         // what the typed name would lock: {name, tier, deposit, mode}
    property var profileDraft: ({})    // the Profile fields as typed, by key
    property string notice: ""
    property bool noticeIsError: false
    property bool settingsOpen: false
    property bool configKnown: false   // the names module has answered once
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
    function loadConfig() {
        function take(config) {
            if (!config || typeof config !== "object" || !("sequencer" in config))
                return false
            registry = config.registry ? config.registry : ""
            sequencer = config.sequencer ? config.sequencer : ""
            walletConfig = config.wallet_config ? config.wallet_config : ""
            walletStorage = config.wallet_storage ? config.wallet_storage : ""
            cliPath = config.cli ? config.cli : ""
            cliWallet = config.cli_wallet ? config.cli_wallet : ""
            configKnown = true
            settingsOpen = registry === ""
            savedMain = config.main_name ? config.main_name : ""
            loadWallet(false)
            loadCliMine()
            loadCliBalance()
            boardLoad()
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

    /// Brings the wallet module up: opening the LEZ Wallet app is what starts
    /// it, and the names module opens the named wallet from there.
    function connectWallet() {
        if (/not running/.test(walletProblem) && logos.request) {
            say("Starting the LEZ wallet.", false)
            logos.request("basecamp.apps.launch", { app: "lez_wallet_ui" }, function() { root.loadWallet(true) })
            return
        }
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
        askLater("cli_mine", [], function(value) {
            value = value || []
            cliNames = value.filter(function(entry) { return !entry.registering && !entry.failed })
            cliPending = value.filter(function(entry) { return entry.registering || entry.failed })
            cliKnown = true
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
        askLater("cli_balance", [], function(value) {
            cliBalance = value
        }, function(message) {
            cliBalance = null
            console.log("names_ui: CLI balance:", message)
        }, 120000)
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
            askLater("register", [item.name, item.owner, "auto"], function(value) {
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
            // The wallet module starts when something asks for it. Ask once,
            // then look again: the names module can open the wallet from there.
            if (/not running/.test(message) && !wokeWallet && logos.callModuleAsync) {
                wokeWallet = true
                logos.callModuleAsync("lez_core", "list_accounts", [], function() { root.loadWallet(false) }, 20000)
            }
            if (announce)
                say(/not running/.test(message) ? "Open the LEZ Wallet app first." : walletProblem, !/not running/.test(message))
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
        tool = ""
        pageSourceOpen = false
        pageAll = false
        pageLink = ""
        read(name)
    }

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
    readonly property var cliMethods: ({ set_text: "cli_set_text", set_profile: "cli_profile", set_page: "cli_set_page", register: "cli_register" })
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
            return "Step 1 of 3: proving the commitment on this computer. About 4 min" + sofar + "."
        return "Proving the change on this computer, so the chain never learns your account. About 4 min" + sofar + "."
    }

    /// The three steps of a private registration, for the register card and its progress.
    readonly property var registerSteps: [
        { title: "Prove the commitment", time: "about 4 min",
          why: "Your computer builds a zero-knowledge proof. The chain checks it without learning which account is yours." },
        { title: "Wait", time: "until 15 min after step 1",
          why: "The commitment hides which name you picked. Waiting means nobody who sees it can rush to take the name first." },
        { title: "Prove and register", time: "about 4 min",
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
            return "Step 3 of 3 · about 4 min left"
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
        cliChange("register", [name], name, function() {
            return true
        }, "Committed privately. " + name + " finishes registering in about 15 to 20 minutes, in the background.")
    }

    /// Sends the commitment. The reveal follows by itself (see revealDue).
    function registerName() {
        var name = missing
        var owner = wallet[fromIndex].account
        if (busy)
            return
        pending = { name: name, sent: false }
        say("Asking the wallet to sign.", false)
        askLater("register", [name, owner, "auto"], function(value) {
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
        var title = typeof page.title === "string" && page.title !== "" ? page.title + " · " : ""
        return title + pageSummary()
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

    /// An entry's value for reading: a long unbroken value is shortened in the middle.
    function entryShown(value) {
        var text = String(value)
        return text.length > 40 && !/\s/.test(text) && !/^https?:/.test(text) ? text.substr(0, 12) + "…" + text.substr(text.length - 8) : text
    }

    /// Whether an amount (decimal text) is more than nothing.
    function someAmount(digits) {
        return digits !== null && digits !== undefined && !/^0*$/.test(String(digits))
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
            return "Yours · public account " + shorten(holder)
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
        var own = boardNames.filter(function(entry) { return entry.name !== anonName })
        if (savedMain !== "" && own.some(function(entry) { return entry.name === savedMain }))
            return savedMain
        return own.length > 0 ? own[0].name : ""
    }
    readonly property bool anonReady: anonName !== "" && cliHolds(anonName)
    /// Who the post box posts as.
    readonly property string boardAs: anonOn && anonReady ? anonName : mainName
    /// First run: no names CLI yet, or no name to post as.
    readonly property bool needsWelcome: configKnown && registry !== "" && (cliPath === "" || (cliKnown && mainName === "" && cliPending.length === 0))

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
        boardStart("make_anon", [], "Setting up your anon name", function() {
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
        boardStart("board_toggle_like", [boardAs, post], liked ? "Your unlike" : "Your like")
        var next = boardLiked.filter(function(item) { return item !== post })
        if (!liked)
            next.push(post)
        boardLiked = next
    }

    function payName() {
        var to = payTo.text.trim()
        var amount = payAmount.text.trim()
        if (to === "" || amount === "")
            return
        boardStart("cli_pay", [to, amount], "Your payment to " + to, function() {
            payAmount.text = ""
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
    function boardStart(method, args, label, after) {
        if (boardJob !== null)
            return
        say("", false)
        var job = { id: "", label: label, started: Date.now(), after: after }
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
                say(job.label + " done.", false)
                if (job.after)
                    job.after()
                boardLoad()
            } else {
                say("It didn't go through: " + lastLogLine(value.log_tail), true)
            }
        }, function() { job.asking = false })
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
        })
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
        interval: 60000
        repeat: true
        running: root.section === "posts" && root.boardJob === null && root.boardView !== "thread"
        onTriggered: { root.clockTick = Date.now(); root.boardLoad() }
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
            font.pixelSize: 18
            font.weight: Font.Bold
        }
        Text {
            text: parent.count
            color: root.dim
            font.family: root.sans
            font.pixelSize: 17
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

    /// The box one task is done in: paying, editing, transferring, deleting.
    component Panel: Rectangle {
        default property alias content: panelColumn.data
        property string title: ""
        Layout.fillWidth: true
        implicitHeight: panelColumn.implicitHeight + 36
        radius: 8
        color: root.panel
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
                font.pixelSize: 19
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
                    color: stepRow.now ? root.brass : stepRow.done ? "#96D678" : "transparent"
                    border.width: stepRow.now || stepRow.done ? 0 : 1
                    border.color: root.line
                    Text {
                        anchors.centerIn: parent
                        text: stepRow.done ? "✓" : String(stepRow.index + 1)
                        color: stepRow.now || stepRow.done ? root.slate : root.dim
                        font.family: root.sans
                        font.pixelSize: 15
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
                        font.pixelSize: 17
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.fillWidth: true
                        text: stepRow.modelData.why
                        color: root.faint
                        font.family: root.sans
                        font.pixelSize: 14
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
        Text { text: parent.label; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
        Text { text: parent.value; color: root.ink; font.family: root.sans; font.pixelSize: 22; font.weight: Font.DemiBold }
        Text { visible: parent.note !== ""; Layout.fillWidth: true; text: parent.note; color: root.faint; font.family: root.sans; font.pixelSize: 14; wrapMode: Text.Wrap }
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
                        font.pixelSize: 17
                    }
                    Text {
                        visible: !!accountRow.modelData.isPrivate
                        text: "Private"
                        color: root.brass
                        font.family: root.sans
                        font.pixelSize: 16
                    }
                    Text {
                        visible: !!accountRow.modelData.name
                        text: accountRow.modelData.name ? accountRow.modelData.name : ""
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 17
                    }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: root.grouped(accountRow.modelData.balance)
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 17
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
            font.pixelSize: 18
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
                            text: root.mainName !== "" && root.boardPeople[root.mainName] && root.boardPeople[root.mainName].display !== "" ? root.boardPeople[root.mainName].display : (root.mainName !== "" ? root.mainName : "Welcome")
                            textFormat: Text.PlainText
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: 19
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            visible: root.mainName !== ""
                            text: root.mainName
                            color: root.faint
                            font.family: root.sans
                            font.pixelSize: 14
                        }
                    }
                }

                Tab { Layout.fillWidth: true; alignLeft: true; label: "Posts"; current: root.section === "posts"; onPressed: root.boardShow(root.boardView === "thread" ? "all" : root.boardView) }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Pay"; current: root.section === "pay"; onPressed: { root.section = "pay"; root.loadCliBalance() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Profile"; current: root.section === "profile"; onPressed: root.openProfile() }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Search"; current: root.section === "search"; onPressed: { root.section = "search"; searchField.forceActiveFocus() } }
                Tab { Layout.fillWidth: true; alignLeft: true; label: "Settings"; current: root.section === "settings"; onPressed: { root.section = "settings"; root.registering = false; root.record = null; root.missing = "" } }

                Item { Layout.fillHeight: true }

                RowLayout {
                    visible: root.sequencer !== ""
                    spacing: 8
                    Rectangle { width: 8; height: 8; radius: 4; color: "#96D678" }
                    Text { text: root.networkName(); color: root.faint; font.family: root.sans; font.pixelSize: 14 }
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }

        // First run: install the CLI, then pick your name.
        Flickable {
            visible: root.needsWelcome && root.section !== "settings"
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
                Text { text: "Welcome"; color: root.ink; font.family: root.sans; font.pixelSize: 36; font.weight: Font.DemiBold }
                Hint { font.pixelSize: 18; text: "Get a name, then post, pay and be paid as it. Two steps." }
                Card {
                    Layout.fillWidth: true
                    RowLayout {
                        spacing: 12
                        Rectangle {
                            width: 30; height: 30; radius: 15
                            color: root.cliPath !== "" ? "#96D678" : root.brass
                            Text { anchors.centerIn: parent; text: root.cliPath !== "" ? "✓" : "1"; color: root.slate; font.family: root.sans; font.pixelSize: 15; font.weight: Font.DemiBold }
                        }
                        Text { text: "Install the names program"; color: root.ink; font.family: root.sans; font.pixelSize: 20; font.weight: Font.DemiBold }
                    }
                    Hint {
                        visible: root.cliPath === ""
                        text: "It proves your changes on this computer, so the chain never learns your account. Paste this in Terminal, then come back:"
                    }
                    RowLayout {
                        visible: root.cliPath === ""
                        Layout.fillWidth: true
                        spacing: 8
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 44
                            radius: 6
                            color: root.slate
                            border.width: 1
                            border.color: root.line
                            Text {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                verticalAlignment: Text.AlignVCenter
                                text: "curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh"
                                color: root.dim
                                font.family: root.mono
                                font.pixelSize: 14
                                elide: Text.ElideRight
                            }
                        }
                        Action { label: "Copy"; onPressed: root.copy("curl -fsSL https://logos-names.wrapit.workers.dev/install.sh | sh && names setup", "The command") }
                    }
                    Link { visible: root.cliPath === ""; text: "I've installed it"; color: root.brass; onPressed: root.loadConfig() }
                    Hint { visible: root.cliPath !== ""; text: "Installed." }
                }
                Card {
                    Layout.fillWidth: true
                    opacity: root.cliPath !== "" ? 1 : 0.5
                    RowLayout {
                        spacing: 12
                        Rectangle {
                            width: 30; height: 30; radius: 15
                            color: root.brass
                            Text { anchors.centerIn: parent; text: "2"; color: root.slate; font.family: root.sans; font.pixelSize: 15; font.weight: Font.DemiBold }
                        }
                        Text { text: "Pick your name"; color: root.ink; font.family: root.sans; font.pixelSize: 20; font.weight: Font.DemiBold }
                    }
                    Hint {
                        text: "Held privately: nobody can find which account holds it. It takes a 1,000,000 deposit, returned in full if you ever give the name up, and about 20 minutes to register."
                            + (root.cliBalance !== undefined && root.cliBalance !== null ? " Your private account holds " + root.grouped(root.cliBalance.balance) + "." : "")
                    }
                    Action {
                        label: "Choose my name"
                        strong: true
                        enabledNow: root.cliPath !== ""
                        onPressed: root.openRegister()
                    }
                }
            }
        }

        // Pay: send to a name, privately.
        Flickable {
            visible: root.section === "pay" && !root.needsWelcome
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: payPage.implicitHeight + 72
            clip: true
            ColumnLayout {
                id: payPage
                x: 56
                y: 40
                width: Math.min(parent.width - 112, 560)
                spacing: 16
                Text { text: "Pay"; color: root.ink; font.family: root.sans; font.pixelSize: 28; font.weight: Font.DemiBold }
                Hint { text: "Send to a name. It goes from your private account to the name's private keys: the chain doesn't show who paid whom, or how much." }
                Text { text: "To"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
                Field { id: payTo; Layout.fillWidth: true; size: 19; hint: "A name, such as ada" }
                Text { text: "Amount"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
                Field { id: payAmount; Layout.fillWidth: true; size: 19; numbersOnly: true; hint: "1000" }
                Hint {
                    font.pixelSize: 15
                    text: root.cliBalance === undefined ? "Checking your private account…"
                        : root.cliBalance === null ? "No private account found."
                        : "Your private account holds " + root.grouped(root.cliBalance.balance) + "."
                }
                Action {
                    label: "Pay privately"
                    strong: true
                    enabledNow: root.boardJob === null && payTo.text.trim() !== "" && payAmount.text.trim() !== ""
                    onPressed: root.payName()
                }
                Hint {
                    visible: root.boardJob !== null
                    color: root.brass
                    font.pixelSize: 16
                    text: root.boardJob === null ? "" : root.boardJob.label + ": proving it on this computer. About 4 min, "
                        + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                }
                Hint {
                    visible: root.boardJob === null && root.notice !== ""
                    color: root.noticeIsError ? root.alarm : root.dim
                    text: root.notice
                }
                Hint { font.pixelSize: 14; text: "The other side sees the payment once their wallet catches up with the chain." }
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
                    Text { visible: root.boardView === "thread"; text: "Thread"; color: root.ink; font.family: root.sans; font.pixelSize: 22; font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    Link { text: root.boardLoading ? "Loading…" : "Refresh"; color: root.faint; font.pixelSize: 15; onPressed: root.boardLoad() }
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
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                        }
                        Item { Layout.fillWidth: true }
                        Chip {
                            label: root.anonOn ? "Anon: on" : "Anon: off"
                            link: true
                            onPressed: root.anonOn = !root.anonOn
                        }
                    }
                    // A private name posts by a proof: say so before anyone presses Post.
                    Rectangle {
                        visible: root.boardAs !== "" && !(root.anonOn && !root.anonReady) && root.cliNames.some(function(entry) { return entry.name === root.boardAs && entry.private !== false })
                        Layout.fillWidth: true
                        implicitHeight: privateWarning.implicitHeight + 20
                        radius: 8
                        color: "#2A2418"
                        Text {
                            id: privateWarning
                            x: 14
                            y: 10
                            width: parent.width - 28
                            text: root.boardAs + " is held privately, so each post, reply and like is proved on this computer first: about 4 minutes each, and free. "
                                + (root.anonOn ? "That's what keeps an anon name unlinkable." : "The chain sees " + root.boardAs + ", never your account.")
                            color: root.brass
                            font.family: root.sans
                            font.pixelSize: 15
                            wrapMode: Text.Wrap
                        }
                    }
                    // The anon name, before it can post.
                    ColumnLayout {
                        visible: root.anonOn && !root.anonReady
                        Layout.fillWidth: true
                        spacing: 8
                        Hint {
                            font.pixelSize: 15
                            text: root.anonName !== "" && root.cliPendingFor(root.anonName) !== null
                                ? root.anonName + " is being set up: " + root.pendingLine(root.cliPendingFor(root.anonName)) + "."
                                : "Anon posts come from a separate name, like anon-quiet-otter, held by its own private account. Nothing on chain links it to "
                                  + (root.mainName !== "" ? root.mainName : "your name") + "; only your wallet knows both. Setting it up takes about 25 minutes, once, and a 1,000,000 deposit you get back if you release it."
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
                        Text { Layout.fillWidth: true; text: "Replying to " + root.boardReplyTo; color: root.brass; font.family: root.sans; font.pixelSize: 15 }
                        Link { text: "Cancel"; color: root.faint; font.pixelSize: 14; onPressed: root.boardReplyTo = "" }
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
                        Text { text: "Spelling:"; color: root.faint; font.family: root.sans; font.pixelSize: 14; height: 34; verticalAlignment: Text.AlignVCenter }
                        Repeater {
                            model: root.boardSpelling
                            Row {
                                required property var modelData
                                spacing: 4
                                Text {
                                    text: modelData.word + (modelData.suggestions.length > 0 ? " →" : " (no suggestion)")
                                    color: root.alarm
                                    font.family: root.sans
                                    font.pixelSize: 14
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
                        Field {
                            id: boardCid
                            Layout.fillWidth: true
                            size: 15
                            code: true
                            hint: "Attach a file: its Logos Storage CID (optional)"
                        }
                        Text {
                            text: boardComposer.text.length + " / 1024"
                            color: boardComposer.text.length > 1024 ? root.alarm : root.faint
                            font.family: root.sans
                            font.pixelSize: 13
                        }
                        Action {
                            label: root.boardReplyTo !== "" ? "Reply" : "Post"
                            strong: true
                            enabledNow: root.boardJob === null && root.boardAs !== "" && !(root.anonOn && !root.anonReady) && boardComposer.text.length <= 1024 && (boardComposer.text.trim() !== "" || boardCid.text.trim() !== "")
                            onPressed: root.boardPost()
                        }
                    }
                    Hint { font.pixelSize: 13; text: "Posts are public. Anon can't hide your writing style or when you post." }
                }

                Hint {
                    visible: root.boardJob !== null
                    color: root.brass
                    font.pixelSize: 16
                    text: root.boardJob === null ? "" : root.boardJob.label + ": proving it on this computer, so the chain never learns your account. About 4 min, "
                        + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                }
                Hint {
                    visible: root.boardJob === null && root.notice !== "" && root.section === "posts"
                    color: root.noticeIsError ? root.alarm : root.dim
                    text: root.notice
                }
                Hint {
                    visible: !root.boardLoading && root.boardPosts.length === 0
                    font.pixelSize: 17
                    text: root.cliPath === "" ? "The board needs the names CLI. Install it, then open Advanced to point at it."
                        : root.boardError !== "" ? "Waiting for the board: " + root.boardError
                        : root.boardView === "following" && root.boardFollows.length === 0 ? "You don't follow anyone yet. Press Follow on a post."
                        : "No posts in the last few hours."
                }

                Repeater {
                    model: root.boardPosts
                    Card {
                        id: postCard
                        required property var modelData
                        readonly property var who: root.boardPeople[modelData.name] || ({ display: "", color: "", pixels: null })
                        readonly property bool isMine: root.cliNames.some(function(entry) { return entry.name === postCard.modelData.name })
                        visible: !modelData.hidden
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 14
                            Avatar {
                                Layout.preferredWidth: 44
                                Layout.preferredHeight: 44
                                Layout.alignment: Qt.AlignTop
                                pixels: postCard.who.pixels
                                owner: postCard.modelData.name
                                letter: postCard.modelData.name.charAt(0)
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
                                        text: postCard.who.display
                                        textFormat: Text.PlainText
                                        color: root.ink
                                        font.family: root.sans
                                        font.pixelSize: 17
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        text: postCard.modelData.name
                                        color: postCard.who.display !== "" ? root.faint : root.ink
                                        font.family: root.sans
                                        font.pixelSize: postCard.who.display !== "" ? 15 : 17
                                        font.weight: postCard.who.display !== "" ? Font.Normal : Font.DemiBold
                                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.show(postCard.modelData.name) }
                                    }
                                    Text { text: "· " + root.boardAgo(postCard.modelData.time); color: root.faint; font.family: root.sans; font.pixelSize: 14 }
                                    Item { Layout.fillWidth: true }
                                    Link {
                                        visible: !postCard.isMine
                                        text: root.boardFollowing(postCard.modelData.name) ? "Following" : "Follow"
                                        color: root.boardFollowing(postCard.modelData.name) ? root.faint : root.brass
                                        font.pixelSize: 14
                                        onPressed: root.boardFollow(postCard.modelData.name, root.boardFollowing(postCard.modelData.name))
                                    }
                                }
                                Text {
                                    visible: !!postCard.modelData.parent
                                    text: "replying to " + (postCard.modelData.parent ? String(postCard.modelData.parent).split("/")[0] : "")
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: 13
                                }
                                Text {
                                    Layout.fillWidth: true
                                    visible: postCard.modelData.text !== ""
                                    text: postCard.modelData.text
                                    textFormat: Text.PlainText
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: 17
                                    lineHeight: 1.2
                                    wrapMode: Text.Wrap
                                }
                                Flow {
                                    Layout.fillWidth: true
                                    visible: postCard.modelData.cids.length > 0
                                    spacing: 8
                                    Repeater {
                                        model: postCard.modelData.cids
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
                                    Link { text: "Reply"; color: root.faint; font.pixelSize: 14; onPressed: root.boardReply(postCard.modelData.post) }
                                    Link {
                                        readonly property bool liked: root.boardLiked.indexOf(postCard.modelData.post) >= 0
                                        text: (liked ? "♥ " : "♡ ") + Math.max(liked ? 1 : 0, postCard.modelData.likes)
                                        color: liked ? root.alarm : root.faint
                                        font.pixelSize: 14
                                        font.underline: false
                                        onPressed: root.boardToggleLike(postCard.modelData.post)
                                    }
                                    Link {
                                        visible: root.boardView !== "thread"
                                        text: "Thread"
                                        color: root.faint
                                        font.pixelSize: 14
                                        onPressed: root.boardShow("thread", postCard.modelData.parent ? postCard.modelData.parent : postCard.modelData.post)
                                    }
                                    Link {
                                        visible: postCard.isMine
                                        text: "Take down"
                                        color: root.faint
                                        font.pixelSize: 14
                                        onPressed: root.boardStart("board_hide", [postCard.modelData.post], "Taking it down")
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
            visible: (root.section === "search" || root.section === "profile" || root.section === "settings") && !(root.needsWelcome && root.section !== "settings")
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
                    visible: root.section === "settings" && !root.registering
                    Layout.fillWidth: true
                    spacing: 14
                    Text { text: "Settings"; color: root.ink; font.family: root.sans; font.weight: Font.DemiBold; font.pixelSize: 28 }
                    Part { text: "Main name" }
                    Hint { text: "Your posts, likes and payments are made as this name." }
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
                    Part { text: "Private or public" }
                    Hint { text: "A name held privately posts in about 4 minutes, free, and nobody can find which account holds it. Make it public and its posts take about 15 seconds and a little gas, but anyone who knows the name can find the account that holds it. Payments to it stay private either way." }
                    Repeater {
                        model: root.cliNames.filter(function(entry) { return entry.name !== root.anonName })
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 12
                            Text { Layout.preferredWidth: 160; text: modelData.name; color: root.ink; font.family: root.sans; font.pixelSize: 17; elide: Text.ElideRight }
                            Text { text: modelData.private === false ? "Public" : "Private"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
                            Item { Layout.fillWidth: true }
                            Action {
                                visible: modelData.private !== false && root.publicAsk !== modelData.name
                                label: "Make public"
                                enabledNow: root.boardJob === null
                                onPressed: root.publicAsk = modelData.name
                            }
                            Text {
                                visible: root.publicAsk === modelData.name
                                text: "About 9 min; funds it with 10,000,000 for gas."
                                color: root.brass
                                font.family: root.sans
                                font.pixelSize: 14
                            }
                            Action {
                                visible: root.publicAsk === modelData.name
                                label: "Make it public"
                                strong: true
                                onPressed: {
                                    var name = modelData.name
                                    root.publicAsk = ""
                                    root.boardStart("make_public", [name], "Making " + name + " public", function() { root.loadCliMine() })
                                }
                            }
                            Action {
                                visible: root.publicAsk === modelData.name
                                label: "Cancel"
                                onPressed: root.publicAsk = ""
                            }
                        }
                    }
                    Hint {
                        visible: root.boardJob !== null && root.section === "settings"
                        color: root.brass
                        text: root.boardJob === null ? "" : root.boardJob.label + ": about 9 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.boardJob.started) / 60000)) + " so far."
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 6
                // The names this wallet holds.
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Hint {
                        visible: root.configKnown && root.registry === ""
                        text: "No registry chosen. Open Advanced."
                    }
                    ListHead {
                        visible: root.registry !== ""
                        label: "My names"
                        count: root.allMine.length + root.pendingNames.length + root.cliPending.length > 0 ? root.allMine.length + root.pendingNames.length + root.cliPending.length : ""
                    }
                    Repeater {
                        model: root.cliPending
                        Rectangle {
                            id: pendingRow
                            required property var modelData
                            Layout.fillWidth: true
                            implicitHeight: pendingColumn.implicitHeight + 20
                            radius: 8
                            color: pendingArea.containsMouse ? root.hover : "transparent"
                            border.width: 1
                            border.color: root.line
                            MouseArea {
                                id: pendingArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.show(pendingRow.modelData.name)
                            }
                            ColumnLayout {
                                id: pendingColumn
                                x: 10
                                y: 10
                                width: parent.width - 20
                                spacing: 4
                                RowLayout {
                                    Layout.fillWidth: true
                                    NameLabel {
                                        Layout.fillWidth: true
                                        name: pendingRow.modelData.name
                                        size: 18
                                    }
                                    Text {
                                        text: pendingRow.modelData.failed ? "Not registered" : "Registering"
                                        color: pendingRow.modelData.failed ? root.alarm : root.brass
                                        font.family: root.sans
                                        font.pixelSize: 13
                                    }
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.pendingLine(pendingRow.modelData)
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: 13
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                    Repeater {
                        model: root.pendingNames
                        Text {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.leftMargin: 8
                            text: (modelData.name) + ": registering" + (modelData.ready_at ? ", ready at " + root.clock(modelData.ready_at) : "")
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: 17
                            elide: Text.ElideRight
                        }
                    }
                    ListView {
                        id: myList
                        visible: root.allMine.length > 0
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(root.allMine.length, 12) * 42
                        clip: true
                        spacing: 2
                        model: root.allMine
                        delegate: Rectangle {
                            id: myRow
                            required property var modelData
                            readonly property bool current: root.record !== null && root.record.name === modelData.name
                            width: myList.width
                            height: 40
                            radius: 6
                            color: current ? root.raised : myRowArea.containsMouse ? root.hover : "transparent"
                            MouseArea {
                                id: myRowArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    root.say("", false)
                                    root.show(myRow.modelData.name)
                                }
                            }
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 8
                                anchors.rightMargin: 4
                                spacing: 4
                                NameLabel {
                                    Layout.fillWidth: true
                                    name: myRow.modelData.name
                                    size: 19
                                    weight: Font.Medium
                                }
                                Text {
                                    text: myRow.modelData.signer !== "cli" ? "Public" : myRow.modelData.private === true ? "Private" : myRow.modelData.private === false ? "Public" : "CLI"
                                    color: root.faint
                                    font.family: root.sans
                                    font.pixelSize: 13
                                }
                                Text {
                                    visible: !!myRow.modelData.lapsed && !myRow.modelData.never
                                    text: "Lapsed"
                                    color: root.alarm
                                    font.family: root.sans
                                    font.pixelSize: 16
                                }
                            }
                        }
                    }
                    Hint {
                        visible: root.allMine.length === 0 && root.pendingNames.length === 0 && root.cliPending.length === 0 && (root.wallet.length > 0 || root.cliPath !== "")
                        text: "None yet. Look one up to register it."
                    }
                    Hint {
                        visible: root.registry !== "" && root.wallet.length === 0
                        text: /not running/.test(root.walletProblem) ? "Wallet not open" : /funds/.test(root.walletProblem) ? "No funded account in the wallet" : "Wallet not connected"
                    }
                    Link {
                        visible: root.registry !== "" && root.wallet.length === 0
                        text: /not running/.test(root.walletProblem) ? "Open LEZ Wallet" : "Connect wallet"
                        onPressed: root.connectWallet()
                    }
                }

                    }
                    Action {
                        Layout.topMargin: 6
                        label: "+  Register another name"
                        strong: true
                        onPressed: root.openRegister()
                    }
                    Link {
                        Layout.topMargin: 10
                        text: root.settingsOpen ? "Hide advanced" : "Advanced"
                        color: root.faint
                        font.pixelSize: 15
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
                        font.pixelSize: 28
                    }
                    Part { text: "Registry" }
                    Hint { text: "Everyone using the same registry sees the same names." }
                    Field {
                        id: registryField
                        Layout.fillWidth: true
                        size: 17
                        text: root.registry
                        hint: "Program ID of the registry"
                        onEdited: root.saveConfig(root.sequencer, registryField.text.trim())
                    }
                    Part { text: "Sequencer" }
                    Field {
                        id: sequencerField
                        Layout.fillWidth: true
                        size: 17
                        text: root.sequencer
                        hint: "https://testnet.lez.logos.co"
                        onEdited: root.saveConfig(sequencerField.text.trim(), root.registry)
                    }
                    Part { text: "Wallet files" }
                    Hint { text: "The LEZ wallet that signs for your names. Open the LEZ Wallet app before making changes." }
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
                        font.pixelSize: 15
                        onPressed: root.showLastReply = !root.showLastReply
                    }
                    Text {
                        visible: root.lastReply !== "" && root.showLastReply
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: root.lastReply
                        color: root.dim
                        font.family: root.mono
                        font.pixelSize: 10
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
                    font.pixelSize: 18
                    wrapMode: Text.Wrap
                }

                // Nothing chosen yet.
                ColumnLayout {
                    visible: root.section === "search" && root.record === null && root.missing === "" && !root.registering
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 10
                    Text {
                        text: !root.configKnown ? "Starting" : root.registry === "" ? "Choose a registry to begin" : "Find someone"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: 36
                    }
                    Hint {
                        font.pixelSize: 18
                        color: root.ink
                        text: !root.configKnown ? "Waiting for the names module to answer." : root.registry === "" ? "Open Advanced and enter the registry's program ID." : "Type a name in the box on the left to see its profile and page."
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
                        font.pixelSize: 36
                    }
                    Hint {
                        font.pixelSize: 18
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
                            font.pixelSize: 18
                        }
                        Link {
                            text: "See it"
                            font.pixelSize: 16
                            onPressed: root.show(root.registerTaken)
                        }
                    }
                    Hint {
                        font.pixelSize: 15
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
                        Rectangle { width: 10; height: 10; radius: 5; color: parent.reserved ? root.alarm : parent.underway ? root.brass : "#96D678" }
                        Text {
                            text: parent.underway ? "Registering privately for you" : parent.reserved ? "Reserved: one- and two-character names are released by the registry's admin" : "Available"
                            color: parent.reserved ? root.alarm : parent.underway ? root.brass : "#96D678"
                            font.family: root.sans
                            font.pixelSize: 19
                        }
                    }
                    // A private registration through the names CLI, under way or stopped.
                    Hint {
                        visible: root.cliPendingFor(root.missing) !== null && root.cliPendingFor(root.missing).failed
                        font.pixelSize: 18
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
                            font.pixelSize: 22
                            font.weight: Font.DemiBold
                        }
                        Steps {
                            Layout.fillWidth: true
                            active: root.registerStepOf(root.missing)
                            note: {
                                var step = root.registerStepOf(root.missing)
                                var under = root.cliPendingFor(root.missing)
                                if (step === 1 && root.pending !== null)
                                    return "about 4 min, " + Math.max(0, Math.floor((Math.max(root.clockTick, Date.now()) - root.pending.started) / 60000)) + " so far"
                                if (step === 2 && under !== null) {
                                    var minutes = /about (\d+) minutes/.exec(under.state)
                                    return minutes ? minutes[1] + " min to go" : "about a minute to go"
                                }
                                return ""
                            }
                        }
                        Hint { font.pixelSize: 15; text: "Keep this computer on and awake. You can close this screen and keep using the app; the name shows under My names when it's done." }
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
                            font.pixelSize: 20
                            color: root.ink
                            text: root.deposit === null ? "Checking the deposit." : root.deposit.tier === "locked" ? "One- and two-character names are reserved." : root.deposit.deposit === null || root.deposit.deposit === undefined ? (root.deposit.fee ? root.grouped(root.deposit.fee) + " to register" : "The registry is not set up for registering yet.") : root.grouped(root.deposit.deposit)
                        }
                        Hint {
                            visible: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode === "deposit"
                            text: "Refunded in full when you release the name. Testnet: names don't expire here, but to keep one on mainnet you claim it within 30 days of launch. Network fee extra."
                        }
                    }
                    Card {
                        visible: root.cliPath !== "" && !(root.deposit !== null && root.deposit.tier === "locked")
                        Layout.fillWidth: true
                        Text {
                            text: "Register privately"
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: 22
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
                                note: "Three steps, below. Keep this computer on and awake until it's done."
                            }
                        }
                        Steps {
                            Layout.fillWidth: true
                            Layout.topMargin: 4
                        }
                        Text {
                            Layout.fillWidth: true
                            text: root.cliBalance === undefined ? "Checking your private account…"
                                : root.cliBalance === null ? "No private account found. Run names setup in a terminal first."
                                : "Pays from your private account " + root.shorten(root.cliBalance.account) + " · balance " + root.grouped(root.cliBalance.balance)
                            color: root.cliBalance === null || !root.cliCanPay() ? root.alarm : root.dim
                            font.family: root.sans
                            font.pixelSize: 15
                            wrapMode: Text.Wrap
                        }
                        Hint {
                            visible: root.cliBalance !== undefined && root.cliBalance !== null && !root.cliCanPay()
                            color: root.alarm
                            font.pixelSize: 15
                            text: "That's less than the deposit. Add funds to this private account first."
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
                            text: "Testnet. Names don't expire here, but to keep one on mainnet you claim it within 30 days of launch. Network fee extra."
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: root.faint
                            font.family: root.sans
                            font.pixelSize: 14
                        }
                    }
                    Link {
                        visible: root.cliPath !== "" && root.wallet.length > 0 && !(root.deposit !== null && root.deposit.tier === "locked")
                        text: root.publicRegisterOpen ? "Hide the public option" : "Register from a public account instead"
                        color: root.faint
                        font.pixelSize: 16
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
                            font.pixelSize: 17
                            text: /not running/.test(root.walletProblem) ? "Open your wallet to register it." : root.walletProblem
                        }
                        Action {
                            label: /not running/.test(root.walletProblem) ? "Open LEZ Wallet" : "Check the wallet again"
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
                                font.pixelSize: 44
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
                                visible: root.profileDisplay !== ""
                                text: root.record !== null ? root.shown(root.record.name) : ""
                                textFormat: Text.PlainText
                                color: root.accent
                                font.family: root.sans
                                font.weight: Font.DemiBold
                                font.pixelSize: 20
                            }
                            Text {
                                visible: root.profileBio !== ""
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                text: root.profileBio
                                textFormat: Text.PlainText
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: 18
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
                            visible: root.record !== null && root.someAmount(root.record.deposit)
                            label: root.record !== null && root.someAmount(root.record.deposit) ? "Deposit " + root.grouped(root.record.deposit) : ""
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

                    // The holder's settings for this name.
                    ColumnLayout {
                        visible: root.tool === "settings"
                        Layout.fillWidth: true
                        spacing: 16
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 20
                            Link {
                                text: "← Back"
                                color: root.faint
                                font.underline: false
                                onPressed: root.tool = ""
                            }
                            Text {
                                text: "Edit"
                                color: root.ink
                                font.family: root.sans
                                font.pixelSize: 24
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
                            text: "Held privately. Each save is proven on this machine and takes a few minutes; keep Basecamp open until it is done."
                        }
                        RowLayout {
                        visible: root.editTab === "profile"
                        Layout.fillWidth: true
                        spacing: 40
                        ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        spacing: 12
                        Hint { text: "All optional." + (root.cliHeld ? " For the avatar, give the full path of a PNG file." : "") }
                        Repeater {
                            model: root.profileFields
                            RowLayout {
                                id: profileRow
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    Layout.preferredWidth: 110
                                    Layout.alignment: profileRow.modelData.lines ? Qt.AlignTop : Qt.AlignVCenter
                                    Layout.topMargin: profileRow.modelData.lines ? 12 : 0
                                    text: profileRow.modelData.label
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: 17
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
                                    readonly property bool shows: profileRow.modelData.key === "avatar" && !root.cliHeld && /^data:/.test(root.draftText("avatar"))
                                    visible: shows
                                    Layout.fillWidth: true
                                    spacing: 12
                                    Text { text: "Picture set"; color: root.ink; font.family: root.sans; font.pixelSize: 17 }
                                    Link {
                                        text: "Remove"
                                        font.pixelSize: 16
                                        onPressed: profileField.text = ""
                                    }
                                    Item { Layout.fillWidth: true }
                                }
                                RowLayout {
                                    visible: profileRow.modelData.key === "avatar" && !root.cliHeld && root.draftText("avatar") === "" && root.profileText("avatar") !== ""
                                    Layout.fillWidth: true
                                    spacing: 12
                                    Text { text: "Picture removed when you save"; color: root.alarm; font.family: root.sans; font.pixelSize: 17 }
                                    Link {
                                        text: "Undo"
                                        font.pixelSize: 16
                                        onPressed: profileField.text = root.profileText("avatar")
                                    }
                                    Item { Layout.fillWidth: true }
                                }
                                Field {
                                    id: profileField
                                    visible: profileRow.modelData.lines !== true && profileRow.modelData.choices === undefined && !(profileRow.modelData.key === "avatar" && !root.cliHeld && (/^data:/.test(root.draftText("avatar")) || root.profileText("avatar") !== ""))
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
                            text: root.entriesOpen ? "Hide entries for other apps" : "Entries for other apps (chat, inbox, agent)"
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
                                        font.pixelSize: 17
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
                            Text { text: "How others see it"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
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
                                            font.pixelSize: 24
                                            font.weight: Font.DemiBold
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            visible: root.draftText("display") !== ""
                                            text: root.record !== null ? root.shown(root.record.name) : ""
                                            color: root.draftAccent
                                            font.family: root.sans
                                            font.pixelSize: 15
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
                                    font.pixelSize: 16
                                    lineHeight: 1.2
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 8
                                    elide: Text.ElideRight
                                }
                            }
                            Hint { font.pixelSize: 14; text: "Anyone who knows the name can read the profile." }
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
                                Text { text: "Paste your page's HTML"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
                                Area {
                                    id: pageArea
                                    Layout.fillWidth: true
                                    implicitHeight: 420
                                    hint: "<h1>Hi</h1>\n<p>About me…</p>"
                                    onTextChanged: previewTimer.restart()
                                }
                                Text { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "This app leaves out scripts, styles and images. In a browser, the page runs as written."; color: root.faint; font.family: root.sans; font.pixelSize: 14 }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                spacing: 8
                                Text { text: "Preview · how this app shows it"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
                                Rectangle {
                                    Layout.fillWidth: true
                                    implicitHeight: 420
                                    radius: 6
                                    color: root.panel
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
                                Text { text: "Public: anyone who knows the name can read it."; color: root.faint; font.family: root.sans; font.pixelSize: 14 }
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
                            text: "For a name held privately, transfer and release it with the names CLI: names transfer, names release."
                        }
                        RowLayout {
                            visible: root.editTab === "giveup" && !root.cliHeld
                            Layout.fillWidth: true
                            spacing: 24
                            Card {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                Layout.alignment: Qt.AlignTop
                                Text { text: "Transfer"; color: root.ink; font.family: root.sans; font.pixelSize: 22; font.weight: Font.DemiBold }
                                Hint { text: root.record !== null ? "Give " + root.shown(root.record.name) + " to someone else. Its profile and page are cleared for them." : "" }
                                Text { text: "Their account"; color: root.faint; font.family: root.sans; font.pixelSize: 15 }
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
                                    color: "#2A2418"
                                    Text {
                                        id: depositWarning
                                        x: 14
                                        y: 12
                                        width: parent.width - 28
                                        text: root.record !== null && root.someAmount(root.record.deposit) ? "Your deposit of " + root.grouped(root.record.deposit) + " goes with the name. They get it back when they release it, not you." : "Any deposit goes with the name."
                                        color: root.brass
                                        font.family: root.sans
                                        font.pixelSize: 15
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
                                Text { text: "Release"; color: root.ink; font.family: root.sans; font.pixelSize: 22; font.weight: Font.DemiBold }
                                Hint { text: "Give the name up. Its profile and page are deleted and anyone can register it." }
                                Figure {
                                    visible: root.record !== null && root.someAmount(root.record.deposit)
                                    label: "You get back"
                                    value: root.record !== null && root.someAmount(root.record.deposit) ? root.grouped(root.record.deposit) : ""
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
                                        font.pixelSize: 18
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
                                value: root.record !== null && root.record.texts[modelData] !== undefined ? root.entryShown(root.record.texts[modelData]) : ""
                                full: root.record !== null && root.record.texts[modelData] !== undefined ? root.record.texts[modelData] : ""
                            }
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
                                font.pixelSize: 18
                                font.weight: Font.Bold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: root.pageHeading()
                                textFormat: Text.PlainText
                                color: root.dim
                                font.family: root.sans
                                font.weight: Font.Medium
                                font.pixelSize: 17
                                elide: Text.ElideRight
                            }
                            Link {
                                visible: root.page !== null && root.page.text !== null && root.page.text !== undefined
                                text: root.pageSourceOpen ? "Hide source" : "Show source"
                                font.pixelSize: 16
                                onPressed: root.pageSourceOpen = !root.pageSourceOpen
                            }
                        }
                        Rectangle {
                            visible: root.pageBody() !== ""
                            Layout.fillWidth: true
                            implicitHeight: pageClip.height + 36
                            radius: 8
                            color: root.panel
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
                                    GradientStop { position: 1; color: root.panel }
                                }
                            }
                        }
                        Link {
                            visible: root.pageIsLong || (root.page !== null && root.page.truncated === true)
                            text: root.pageIsLong ? (root.pageAll ? "Show less" : "Show all") : "Only the first 64 KB is shown"
                            font.pixelSize: 16
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
                                    font.pixelSize: 17
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.pageLink
                                    textFormat: Text.PlainText
                                    color: root.ink
                                    font.family: root.mono
                                    font.pixelSize: 15
                                    elide: Text.ElideMiddle
                                }
                                Link {
                                    text: "Copy"
                                    font.pixelSize: 16
                                    onPressed: root.copy(root.pageLink, "Link")
                                }
                                Link {
                                    text: "Open in browser"
                                    font.pixelSize: 16
                                    onPressed: root.openPageLink()
                                }
                                Link {
                                    text: "Close"
                                    font.pixelSize: 16
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
                                font.pixelSize: 14
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
}
