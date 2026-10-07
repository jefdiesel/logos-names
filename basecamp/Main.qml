// Logos Names for Basecamp: find, register and manage .LEZ names.
//
// A front end on the `names` module. The module reads the chain itself, so
// browsing needs no wallet. Registering a name, changing it, transferring it
// and deleting it are signed by the LEZ wallet (`lez_core`), which the module
// asks on this screen's behalf. Paying goes to `lez_core` directly and
// messaging through `chat_module`.
//
// Controls are drawn here rather than taken from Qt Quick Controls so the app
// looks the same whatever style its host applies.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    // ── State ───────────────────────────────────────────────────────────

    property var myNames: []           // [{name, owner}]: names this device remembers that the wallet holds
    property var record: null          // the name on the plate
    property var page: null
    property string missing: ""        // a looked-up name that is not registered
    property var wallet: []            // the wallet's funded public accounts: [{account, balance, name}]
    property var privateWallet: []     // its funded private accounts: [{account, balance}]
    property int senderIndex: 0        // which of senders() pays
    property bool sending: false       // a payment is being prepared or sent
    property string walletProblem: ""  // why the wallet cannot be used, if it cannot
    property int fromIndex: 0
    property string tool: ""           // the open panel: "", "pay" or "settings"
    property bool confirmingDelete: false
    property var pending: null         // a change that was sent and is not confirmed yet
    property string registry: ""
    property string sequencer: ""
    property string walletConfig: ""   // the wallet files the names module opens by itself
    property string walletStorage: ""
    property bool wokeWallet: false
    property var terms: []             // [{term, label, days, fee}]: how long a name can be held, and the price
    property int termIndex: 1
    property string notice: ""
    property bool noticeIsError: false
    property bool settingsOpen: false
    property bool configKnown: false   // the names module has answered once
    property string lastReply: ""

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property bool busy: pending !== null
    readonly property bool mine: record !== null && holds(record.owner, wallet)
    readonly property string chatAddress: record !== null && record.texts && /^[0-9a-f]{64}$/.test(record.texts.chat || "") ? record.texts.chat : ""

    // ── Palette and type ────────────────────────────────────────────────

    // Text is white on near-black, and even the secondary grey stays above
    // 9:1 against the background, so nothing on screen reads as faint.
    readonly property color slate: "#0F1115"
    readonly property color panel: "#171A20"
    readonly property color raised: "#2B303B"
    readonly property color hover: "#20242D"
    readonly property color line: "#434A59"
    readonly property color ink: "#FFFFFF"
    readonly property color dim: "#DCE1EA"
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
    function askLater(method, args, done, failed) {
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
            logos.callModuleAsync("names", method, args, settle, 30000)
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
        if (!/^\S+\.lez\b/i.test(text))
            text = text.charAt(0).toUpperCase() + text.substr(1)
        return /[.?!]$/.test(text) ? text : text + "."
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
            configKnown = true
            settingsOpen = registry === ""
            loadPrices()
            loadWallet(false)
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

    /// Reads what each term costs. Until it is known, registering waits.
    function loadPrices() {
        if (registry === "")
            return
        askLater("prices", [], function(value) {
            terms = value.terms
        }, function(message) {
            console.log("names_ui: prices:", message)
        })
    }

    /// A fee as people read it.
    function priced(fee) {
        return fee === "0" ? "Free" : grouped(fee)
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

    /// Reads the names this device remembers that the open wallet holds. The
    /// chain keeps no list of names, so this is the module's own memory.
    function loadMine() {
        if (registry === "" || wallet.length === 0) {
            myNames = []
            return
        }
        askLater("mine", [], function(value) {
            myNames = value.names
        }, function(message) {
            console.log("names_ui: my names:", message)
        })
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
            walletProblem = funded.length > 0 ? "" : "None of the wallet's accounts holds funds to pay the fee."
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
    function show(name) {
        tool = ""
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
                if (missing === "")
                    say("A name is 1 to 32 characters of a-z, 0-9 or a hyphen, such as jef.LEZ.", true)
                return
            }
            missing = ""
            record = found
            page = ask("page", [found.name])
            if (!mine && tool !== "pay")
                tool = ""
            // A name registered elsewhere joins "My names" once it is looked up.
            if (mine && !remembers(found.name))
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
        var job = { name: name, settled: settled, done: done, sent: false, tries: 0 }
        pending = job
        say("Asking the wallet to sign.", false)
        askLater(method, args, function() {
            job.sent = true
            say("Sent. Waiting for it to be confirmed.", false)
        }, function(message) {
            pending = null
            say(message, true)
        })
    }

    function checkPending() {
        var job = pending
        if (!job.sent)
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
            say(job.done, false)
        else
            say("That was sent but has not taken effect, so it may have been refused. Check the name and try again.", true)
    }

    function registerName() {
        var name = missing
        var owner = wallet[fromIndex].account
        change("register", [name, owner, "auto", termIndex], name, function() {
            var found = ask("resolve", [name])
            return found !== null && found.owner === owner
        }, name + ".LEZ is yours.")
    }

    /// Opens the settings of a name picked from "My names".
    function manage(name) {
        show(name)
        if (record !== null && mine)
            openSettings()
    }

    /// Everything the holder can do with the name on the plate: where it
    /// pays, its entries and page, transferring it, deleting it.
    function renewName() {
        var name = record.name
        var until = record.expires
        change("renew", [name, record.owner, termIndex], name, function() {
            return ask("resolve", [name]).expires > until
        }, shown(name) + " is renewed.")
    }

    function openSettings() {
        say("", false)
        accountField.text = record.account !== null ? record.account : ""
        pageArea.text = page !== null && page.text !== null ? page.text : ""
        newKeyField.text = ""
        newValueField.text = ""
        newOwnerField.text = ""
        confirmingDelete = false
        tool = "settings"
    }

    function saveAccount() {
        var name = record.name
        var account = accountField.text.trim()
        change("set_account", [name, account], name, function() {
            return ask("resolve", [name]).account === account
        }, name + ".LEZ now pays to " + shorten(account) + ".")
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
        }, name + ".LEZ no longer takes payments.")
    }

    /// Sets one entry; an empty value removes it.
    function saveText(key, value) {
        var name = record.name
        key = key.trim()
        change("set_text", [name, key, value], name, function() {
            var texts = ask("resolve", [name]).texts
            return value === "" ? texts[key] === undefined : texts[key] === value
        }, value === "" ? "Removed " + key + " from " + name + ".LEZ." : "Saved " + key + " on " + name + ".LEZ.")
    }

    /// Publishes the page; an empty body removes it. A body that opens with a
    /// tag is published as HTML, anything else as plain text.
    function savePage(body) {
        var name = record.name
        var kind = /^\s*</.test(body) ? "text/html; charset=utf-8" : "text/plain; charset=utf-8"
        change("set_page", [name, kind, body], name, function() {
            var published = ask("page", [name])
            return body === "" ? published === null : published !== null && published.text === body
        }, body === "" ? "Removed the page of " + name + ".LEZ." : "Published the page of " + name + ".LEZ.")
    }

    function showAccountAsName() {
        var name = record.name
        var account = record.owner
        change("set_primary", [account, name], name, function() {
            var shown = ask("reverse", [account])
            return shown !== null && shown.name === name
        }, shorten(account) + " is now shown as " + name + ".LEZ.")
    }

    function transferName() {
        var name = record.name
        var newOwner = newOwnerField.text.trim()
        change("transfer", [name, newOwner], name, function() {
            return ask("resolve", [name]).owner === newOwner
        }, name + ".LEZ now belongs to " + shorten(newOwner) + ".")
    }

    function deleteName() {
        var name = record.name
        change("release", [name], name, function() {
            return ask("resolve", [name]) === null
        }, "Deleted " + name + ".LEZ.")
    }

    // ── Paying and messaging ────────────────────────────────────────────

    function openPay() {
        if (wallet.length === 0) {
            loadWallet(true)
            return
        }
        say("", false)
        tool = "pay"
    }

    /// A whole amount as the 16-byte little-endian hex the wallet expects, or "".
    function amountHex(text) {
        if (!/^[0-9]{1,15}$/.test(text.trim()))
            return ""
        var value = parseInt(text.trim(), 10)
        if (value <= 0)
            return ""
        var hex = ""
        for (var i = 0; i < 16; i++) {
            var low = value % 256
            hex += (low < 16 ? "0" : "") + low.toString(16)
            value = Math.floor(value / 256)
        }
        return hex
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

    function sender() {
        var all = senders()
        return senderIndex < all.length ? all[senderIndex] : null
    }

    /// What a payment from the chosen account would hide.
    function payHidden(usePrivateKeys) {
        var from = sender()
        if (from === null)
            return ""
        if (from.isPrivate && usePrivateKeys)
            return "Nothing about this payment is visible: not who sent it, who received it, or how much."
        if (from.isPrivate)
            return "The sender stays hidden. The amount and the receiving account are visible, because the name is paid at a public account."
        if (usePrivateKeys)
            return "The receiver stays hidden. The amount and your account are visible, because it leaves a public account."
        return "Everything about this payment is visible: your account, the receiving account and the amount."
    }

    /// Pays the name from the chosen account. A payment that hides anything
    /// is proved on this machine first, which takes minutes, so the wallet is
    /// asked without holding the screen still.
    function pay(usePrivateKeys) {
        var amount = amountHex(amountInput.text)
        if (amount === "") {
            say("Enter a whole amount.", true)
            return
        }
        var from = sender()
        if (from === null) {
            say("Choose an account to pay from.", true)
            return
        }
        try {
            // Ask again at the moment of paying, so a name changed since it was shown is not paid stale.
            var to = ask("recipient", [record.name])
            if (to === null)
                throw new Error(shown(record.name) + " is no longer registered.")
            if (usePrivateKeys && to.private === null)
                throw new Error(shown(record.name) + " does not take private payments.")
            if (!usePrivateKeys && to.public_hex === null)
                throw new Error(shown(record.name) + " does not take public payments.")
            var fromHex = String(call("lez_core", "account_id_from_base58", [from.account]))
            var method = from.isPrivate ? (usePrivateKeys ? "transfer_private" : "transfer_deshielded")
                                        : (usePrivateKeys ? "transfer_shielded" : "transfer_public")
            var args = [fromHex, usePrivateKeys ? JSON.stringify(to.private) : to.public_hex, amount]
            var slow = from.isPrivate || usePrivateKeys
            sending = true
            say(slow ? "Preparing the payment. Proving it takes a few minutes; you can keep using the app." : "Sending.", false)
            var amountText = amountInput.text.trim()
            var display = shown(record.name)
            function settle(reply) {
                sending = false
                reply = parsed(reply)
                if (reply && reply.success) {
                    tool = ""
                    say("Sent " + amountText + " to " + display + ".", false)
                    loadWallet(false)
                } else {
                    say("The wallet refused: " + (reply && reply.error ? reply.error : "no reason given") + ".", true)
                }
            }
            if (logos.callModuleAsync)
                logos.callModuleAsync("lez_core", method, args, settle, 20 * 60 * 1000)
            else
                settle(logos.callModule("lez_core", method, args))
        } catch (error) {
            sending = false
            say(error.message, true)
        }
    }

    function message() {
        try {
            var reply = call("chat_module", "create_conversation", [chatAddress])
            if (reply && reply.success === false) {
                say("Chat could not start the conversation: " + reply.error, true)
                return
            }
            say("Conversation with " + shown(record.name) + " started. Opening Chat.", false)
            if (logos.request)
                logos.request("basecamp.apps.launch", { app: "chat_ui" }, function() {})
        } catch (error) {
            say(error.message, true)
        }
    }

    // ── Small helpers ───────────────────────────────────────────────────

    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied.", false)
    }

    /// A name as people read it: `jef.LEZ`.
    function shown(name) {
        return name + ".LEZ"
    }

    function shorten(id) {
        return id.length > 16 ? id.substr(0, 8) + "…" + id.substr(id.length - 6) : id
    }

    function linkFor(name) {
        return "basecamp://intent/names.open?p=" + Qt.btoa(JSON.stringify({ name: name + ".LEZ" })).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
    }

    /// Digits grouped in threes, so a balance can be read at a glance.
    function grouped(digits) {
        return String(digits).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    /// The readable words of a page. Markup is removed, never rendered: a page is
    /// its owner's words, and nothing in it should be able to act inside this app.
    function pageWords() {
        if (page === null || page.text === null)
            return ""
        var words = page.text
        if (/html/i.test(page.content_type)) {
            words = words.replace(/<(script|style|title|head)[^>]*>[\s\S]*?<\/\1>/gi, " ")
                         .replace(/<\/(p|div|h[1-6]|li|tr|section|article)>|<br\s*\/?>/gi, "\n")
                         .replace(/<[^>]*>/g, "")
                         .replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&#39;|&apos;/g, "'").replace(/&nbsp;/g, " ")
        }
        return words.replace(/[ \t]+/g, " ").replace(/\s*\n\s*/g, "\n").trim().substr(0, 2400)
    }

    function textKeys() {
        return record !== null && record.texts ? Object.keys(record.texts) : []
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
        readonly property bool live: enabledNow && !(changes && root.busy)
        readonly property color fill: grave ? root.alarm : root.brass
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
            font.pixelSize: 16
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
            font.pixelSize: 16
            font.weight: Font.Bold
        }
        Text {
            text: parent.count
            color: root.dim
            font.family: root.sans
            font.pixelSize: 15
        }
    }

    /// A name with its suffix. The name leads; `.LEZ` follows in smaller,
    /// quieter capitals, so it reads as a mark rather than part of the word.
    component NameLabel: Item {
        id: label
        property string name: ""
        property int size: 15
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
            visible: label.name !== ""
            text: ".LEZ"
            color: root.dim
            font.family: root.sans
            font.weight: label.weight
            font.pixelSize: Math.round(label.size * 0.7)
            font.letterSpacing: label.size * 0.04
        }
    }

    /// A quiet text button, for the small things beside the main actions.
    component Link: Text {
        id: link
        signal pressed()
        color: linkArea.containsMouse ? root.brass : root.ink
        font.family: root.sans
        font.pixelSize: 15
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
        property int size: 16
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
                font.family: root.mono
                font.pixelSize: 14
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
            font.pixelSize: 15
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
                font.pixelSize: 17
                font.weight: Font.DemiBold
            }
        }
    }

    /// The terms a name can be held for, each with its price; one is chosen.
    component Terms: RowLayout {
        Layout.fillWidth: true
        spacing: 8
        Repeater {
            model: root.terms
            Rectangle {
                id: term
                required property var modelData
                required property int index
                readonly property bool chosen: root.termIndex === index
                Layout.fillWidth: true
                implicitHeight: 60
                radius: 6
                color: chosen ? root.raised : termArea.containsMouse ? root.hover : "transparent"
                border.width: 1
                border.color: chosen ? root.brass : root.line
                Column {
                    anchors.centerIn: parent
                    spacing: 2
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: term.modelData.label
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: 16
                        font.weight: Font.DemiBold
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: root.priced(term.modelData.fee)
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 15
                    }
                }
                MouseArea {
                    id: termArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.termIndex = term.index
                }
            }
        }
    }

    /// A line of explanation under a control.
    component Hint: Text {
        Layout.fillWidth: true
        color: root.dim
        font.family: root.sans
        font.pixelSize: 15
        lineHeight: 1.25
        wrapMode: Text.Wrap
    }

    /// The heading of one part of the edit panel.
    component Part: Text {
        Layout.topMargin: 10
        color: root.ink
        font.family: root.sans
        font.pixelSize: 16
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
                        font.pixelSize: 15
                    }
                    Text {
                        visible: !!accountRow.modelData.isPrivate
                        text: "Private"
                        color: root.brass
                        font.family: root.sans
                        font.pixelSize: 14
                    }
                    Text {
                        visible: !!accountRow.modelData.name
                        text: accountRow.modelData.name ? accountRow.modelData.name + ".LEZ" : ""
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 15
                    }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: root.grouped(accountRow.modelData.balance)
                        color: root.dim
                        font.family: root.sans
                        font.pixelSize: 15
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
            Layout.preferredWidth: 110
            text: fact.label
            color: root.dim
            font.family: root.sans
            font.pixelSize: 16
        }
        Text {
            Layout.fillWidth: true
            text: fact.value
            color: root.ink
            font.family: fact.code ? root.mono : root.sans
            font.pixelSize: 16
            wrapMode: Text.WrapAnywhere
        }
        Link {
            visible: fact.full !== ""
            text: "Copy"
            onPressed: root.copy(fact.full, fact.label)
        }
    }

    // ── Layout ──────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Search, and the holder's own names.
        Rectangle {
            Layout.preferredWidth: 292
            Layout.fillHeight: true
            color: root.panel

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 18
                spacing: 14

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Field {
                        id: searchField
                        Layout.fillWidth: true
                        hint: "Find or register"
                        onAccepted: root.find(searchField.text)
                    }
                    IconButton {
                        kind: "search"
                        boxed: true
                        onPressed: root.find(searchField.text)
                    }
                }

                // The names this wallet holds.
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Hint {
                        visible: root.configKnown && root.registry === ""
                        text: "No registry chosen. Open App settings."
                    }
                    ListHead {
                        visible: root.registry !== ""
                        label: "My names"
                        count: root.myNames.length > 0 ? root.myNames.length : ""
                    }
                    ListView {
                        id: myList
                        visible: root.myNames.length > 0
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(root.myNames.length, 12) * 42
                        clip: true
                        spacing: 2
                        model: root.myNames
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
                                    size: 17
                                    weight: Font.Medium
                                }
                                Text {
                                    visible: !!myRow.modelData.lapsed
                                    text: "Lapsed"
                                    color: root.alarm
                                    font.family: root.sans
                                    font.pixelSize: 14
                                }
                                IconButton {
                                    kind: "gear"
                                    onPressed: root.manage(myRow.modelData.name)
                                }
                            }
                        }
                    }
                    Hint {
                        visible: root.myNames.length === 0 && root.wallet.length > 0
                        text: "None yet. Search for a .LEZ name; if nobody holds it, you can register it."
                    }
                    Hint {
                        visible: root.registry !== "" && root.wallet.length === 0
                        text: /not running/.test(root.walletProblem) ? "The LEZ wallet is not running. Opening the LEZ Wallet app starts it; the names you hold appear here after that." : "Open your wallet in the LEZ Wallet app to see the .LEZ names you hold."
                    }
                    Link {
                        visible: root.registry !== "" && root.wallet.length === 0
                        text: /not running/.test(root.walletProblem) ? "Open LEZ Wallet" : "Connect wallet"
                        onPressed: root.connectWallet()
                    }
                }

                Item { Layout.fillHeight: true }

                Link {
                    text: root.settingsOpen ? "Close app settings" : "App settings"
                    onPressed: root.settingsOpen = !root.settingsOpen
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: root.line }

        // The plate.
        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: plate.implicitHeight + 72
            clip: true

            ColumnLayout {
                id: plate
                x: 40
                y: 36
                width: Math.min(parent.width - 80, 640)
                spacing: 22

                // Settings.
                ColumnLayout {
                    visible: root.settingsOpen
                    Layout.fillWidth: true
                    spacing: 10
                    Text {
                        text: "Which registry"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: 24
                    }
                    Hint {
                        font.pixelSize: 15
                        text: "A registry is one program on the Logos Execution Zone. Everyone who uses the same program ID resolves the same .LEZ names."
                    }
                    Field {
                        id: registryField
                        Layout.fillWidth: true
                        size: 15
                        text: root.registry
                        hint: "Program ID of the registry"
                        onEdited: root.saveConfig(root.sequencer, registryField.text.trim())
                    }
                    Field {
                        id: sequencerField
                        Layout.fillWidth: true
                        size: 15
                        text: root.sequencer
                        hint: "https://testnet.lez.logos.co"
                        onEdited: root.saveConfig(sequencerField.text.trim(), root.registry)
                    }
                    Part { text: "Wallet" }
                    Hint { text: "The wallet that signs for your names. Give the two files the LEZ Wallet keeps it in and it is opened for you whenever it is needed." }
                    Field {
                        id: walletConfigField
                        Layout.fillWidth: true
                        size: 15
                        text: root.walletConfig
                        hint: "Path of the wallet's config file"
                    }
                    Field {
                        id: walletStorageField
                        Layout.fillWidth: true
                        size: 15
                        text: root.walletStorage
                        hint: "Path of the wallet's storage file"
                    }
                    Action {
                        label: "Use this wallet"
                        onPressed: root.saveWalletFiles(walletConfigField.text.trim(), walletStorageField.text.trim())
                    }
                    Text {
                        visible: root.lastReply !== ""
                        Layout.fillWidth: true
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
                    font.pixelSize: 15
                    wrapMode: Text.Wrap
                }

                // Nothing chosen yet.
                ColumnLayout {
                    visible: root.record === null && root.missing === "" && !root.settingsOpen
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 10
                    Text {
                        text: !root.configKnown ? "Starting" : root.registry === "" ? "Choose a registry to begin" : "Find a .LEZ name"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: 32
                    }
                    Hint {
                        font.pixelSize: 16
                        text: !root.configKnown ? "Waiting for the names module to answer." : root.registry === "" ? "Open App settings and enter the registry's program ID." : "Search for a .LEZ name to see where it pays and how to reach its holder. If nobody holds it, you can register it. .LEZ names are private: nobody can browse them, only look up one they already know."
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
                        size: 48
                        weight: Font.DemiBold
                    }
                    Text {
                        text: "Nobody holds this name."
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: 17
                    }
                    Panel {
                        visible: root.wallet.length > 0
                        title: "Register it to"
                        Accounts {}
                        Hint { text: "The account you choose holds the name and pays for it. Only it can change, transfer or delete the name. The name is set up to be paid privately, to your wallet's private account." }
                        Part { text: "For how long" }
                        Terms {}
                        Hint { text: "The price goes to the registry's treasury; the network's own fee is extra. You can renew at any time, and for 30 days after it runs out." }
                        Action {
                            label: "Register " + root.missing + ".LEZ"
                            strong: true
                            changes: true
                            enabledNow: root.terms.length > 0
                            onPressed: root.registerName()
                        }
                    }
                    ColumnLayout {
                        visible: root.wallet.length === 0
                        Layout.fillWidth: true
                        spacing: 12
                        Hint {
                            font.pixelSize: 15
                            text: "Registering it takes an account that can pay the fee. " + root.walletProblem
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
                        Layout.fillWidth: true
                        spacing: 22
                        Seal {
                            Layout.preferredWidth: 104
                            Layout.preferredHeight: 104
                            owner: root.record !== null ? root.record.owner : ""
                            letter: root.record !== null ? root.record.name.charAt(0) : ""
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            NameLabel {
                                Layout.fillWidth: true
                                name: root.record !== null ? root.record.name : ""
                                size: 48
                                weight: Font.DemiBold
                            }
                            Text {
                                text: root.record === null ? "" : root.record.npk !== null && root.record.account !== null ? "Takes public and private payments" : root.record.npk !== null ? "Takes private payments" : root.record.account !== null ? "Takes public payments" : "Does not take payments yet"
                                color: root.dim
                                font.family: root.sans
                                font.pixelSize: 16
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Action {
                            label: "Send"
                            strong: true
                            enabledNow: root.record !== null && (root.record.account !== null || root.record.npk !== null)
                            onPressed: root.openPay()
                        }
                        Action {
                            label: "Message"
                            enabledNow: root.chatAddress !== ""
                            onPressed: root.message()
                        }
                        Action {
                            label: "Copy link"
                            onPressed: root.copy(root.linkFor(root.record.name), "Link")
                        }
                        Item { Layout.fillWidth: true }
                        Action {
                            visible: root.mine
                            label: "Settings"
                            onPressed: root.openSettings()
                        }
                    }

                    // Paying.
                    Panel {
                        visible: root.tool === "pay"
                        title: root.record !== null ? "Send to " + root.shown(root.record.name) : ""
                        Accounts { withPrivate: true }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Field {
                                id: amountInput
                                Layout.preferredWidth: 160
                                hint: "Amount"
                                numbersOnly: true
                            }
                            Action {
                                label: root.sending ? "Preparing" : "Send privately"
                                strong: root.record !== null && root.record.npk !== null
                                enabledNow: !root.sending && root.record !== null && root.record.npk !== null
                                onPressed: root.pay(true)
                            }
                            Action {
                                label: "Send publicly"
                                strong: root.record !== null && root.record.npk === null
                                enabledNow: !root.sending && root.record !== null && root.record.account !== null
                                onPressed: root.pay(false)
                            }
                            Item { Layout.fillWidth: true }
                            Action { label: "Cancel"; onPressed: root.tool = "" }
                        }
                        Hint {
                            visible: root.record !== null && root.record.lapsed
                            color: root.alarm
                            text: root.record !== null && root.record.lapsed ? root.shown(root.record.name) + " lapsed on " + root.dated(root.record.expires) + ". Its holder can still renew it, but check with them before you pay." : ""
                        }
                        Hint { text: root.record === null ? "" : (root.record.npk !== null ? root.payHidden(true) : root.payHidden(false)) + " A payment that hides anything is proved on this machine first and takes a few minutes." }
                    }

                    // The holder's settings for this name.
                    Panel {
                        visible: root.tool === "settings"
                        title: root.record !== null ? "Settings for " + root.shown(root.record.name) : ""

                        Part { text: "Renew" }
                        Hint { text: root.record === null ? "" : root.record.lapsed ? "It lapsed on " + root.dated(root.record.expires) + ". Renew it by " + root.dated(root.record.expires + 2592000000) + " or anyone can register it." : "Yours until " + root.dated(root.record.expires) + ". Renewing adds to that." }
                        Terms {}
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Action {
                                label: root.terms.length > root.termIndex ? "Renew for " + root.terms[root.termIndex].label : "Renew"
                                strong: root.record !== null && root.record.lapsed
                                changes: true
                                enabledNow: root.terms.length > 0
                                onPressed: root.renewName()
                            }
                            Item { Layout.fillWidth: true }
                        }

                        Part { text: "Where it is paid" }
                        Hint {
                            text: root.record === null ? "" : root.record.npk !== null && root.record.account === null ? "Private payments only. Someone who looks the name up sees receiving keys that say nothing about your accounts, and each payment lands where only you can find it."
                                : root.record.npk !== null ? "Private payments, and a public account. Anyone who looks the name up sees that account, its balance and its history."
                                : root.record.account !== null ? "A public account only. Anyone who looks the name up sees that account, its balance and its history."
                                : "Nowhere yet. Nobody can pay this name."
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
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Field {
                                id: accountField
                                Layout.fillWidth: true
                                code: true
                                hint: "Or a public account ID, visible to anyone who looks the name up"
                            }
                            Action {
                                label: "Save"
                                changes: true
                                enabledNow: accountField.text.trim() !== "" && root.record !== null && accountField.text.trim() !== root.record.account
                                onPressed: root.saveAccount()
                            }
                        }

                        Part { text: "Entries" }
                        Repeater {
                            model: root.textKeys()
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
                                    font.pixelSize: 15
                                    elide: Text.ElideRight
                                }
                                Field {
                                    id: entryValue
                                    Layout.fillWidth: true
                                    size: 17
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
                                size: 15
                                hint: "chat"
                            }
                            Field {
                                id: newValueField
                                Layout.fillWidth: true
                                size: 15
                                hint: "Value"
                            }
                            Action {
                                label: "Add"
                                changes: true
                                enabledNow: newKeyField.text.trim() !== "" && newValueField.text !== ""
                                onPressed: root.saveText(newKeyField.text, newValueField.text)
                            }
                        }
                        Hint { text: "Other apps read these. chat is your Logos Chat address, inbox a Logos Delivery topic, agent where your AI agent answers, cid a file in Logos Storage." }

                        Part { text: "Page" }
                        Area {
                            id: pageArea
                            Layout.fillWidth: true
                            hint: "Write plain text or HTML. Anyone can read it at this name."
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Action {
                                label: "Publish page"
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

                        Part { text: "Your account's name" }
                        Hint { text: root.record !== null ? "Apps can then show " + root.shorten(root.record.owner) + " as " + root.shown(root.record.name) + ". This publishes the name on the chain for anyone to see, which nothing else here does." : "" }
                        Action {
                            label: root.record !== null ? "Show my account as " + root.shown(root.record.name) : ""
                            changes: true
                            onPressed: root.showAccountAsName()
                        }

                        Rectangle { Layout.fillWidth: true; Layout.topMargin: 8; height: 1; color: root.line }

                        Part { text: "Transfer to another wallet" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Field {
                                id: newOwnerField
                                Layout.fillWidth: true
                                code: true
                                hint: "Account ID of the new holder"
                            }
                            Action {
                                label: "Transfer"
                                changes: true
                                enabledNow: newOwnerField.text.trim() !== ""
                                onPressed: root.transferName()
                            }
                        }
                        Hint { text: "The new holder starts clean: where the name pays, its entries and its page are cleared. After that only they can change it or give it back." }

                        Part { text: "Delete" }
                        Hint { text: "Its record and page are emptied, and nothing of it stays behind. The name is then free for anyone to register, including someone else." }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Action {
                                visible: !root.confirmingDelete
                                label: root.record !== null ? "Delete " + root.shown(root.record.name) : ""
                                grave: true
                                onPressed: root.confirmingDelete = true
                            }
                            Text {
                                visible: root.confirmingDelete
                                text: "Delete it for good?"
                                color: root.alarm
                                font.family: root.sans
                                font.pixelSize: 16
                                font.weight: Font.DemiBold
                            }
                            Action {
                                visible: root.confirmingDelete
                                label: "Yes, delete it"
                                strong: true
                                grave: true
                                changes: true
                                onPressed: root.deleteName()
                            }
                            Action {
                                visible: root.confirmingDelete
                                label: "Keep it"
                                onPressed: root.confirmingDelete = false
                            }
                            Item { Layout.fillWidth: true }
                            Action { label: "Done"; onPressed: root.tool = "" }
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: root.line }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Fact {
                            label: "Held by"
                            code: !root.mine
                            value: root.record === null ? "" : root.mine ? "Your account " + root.shorten(root.record.owner) : root.shorten(root.record.owner)
                            full: root.record !== null ? root.record.owner : ""
                        }
                        Fact {
                            visible: root.record !== null && root.record.account !== null
                            label: "Pays to"
                            code: true
                            value: root.record !== null && root.record.account !== null ? root.shorten(root.record.account) : ""
                            full: root.record !== null && root.record.account !== null ? root.record.account : ""
                        }
                        Fact {
                            label: "Held until"
                            value: root.record === null ? "" : root.dated(root.record.expires) + (root.record.lapsed ? ". It has lapsed and can be renewed until " + root.dated(root.record.expires + 2592000000) : "")
                        }
                        Fact {
                            label: "Paid privately"
                            value: root.record !== null && root.record.npk !== null ? "Yes" : "Not set up"
                        }
                        Repeater {
                            model: root.textKeys()
                            Fact {
                                required property string modelData
                                label: modelData
                                value: root.record !== null && root.record.texts[modelData] !== undefined ? root.record.texts[modelData] : ""
                                full: value
                            }
                        }
                    }

                    // The name's page.
                    ColumnLayout {
                        visible: root.page !== null
                        Layout.fillWidth: true
                        spacing: 8
                        Rectangle { Layout.fillWidth: true; height: 1; color: root.line }
                        Text {
                            text: root.record !== null ? "On " + root.record.name + "'s page" : ""
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: 15
                        }
                        Text {
                            Layout.fillWidth: true
                            Layout.maximumHeight: 300
                            clip: true
                            textFormat: Text.PlainText
                            text: root.pageWords() !== "" ? root.pageWords() : "The page holds no readable text."
                            color: root.ink
                            font.family: root.sans
                            font.pixelSize: 17
                            lineHeight: 1.4
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }
    }
}
