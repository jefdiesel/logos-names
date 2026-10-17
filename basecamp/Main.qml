// Logos Names for Basecamp: find, register and manage .lez names.
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

    property var myNames: []           // [{name, display, owner, expires, never, lapsed}]: names the wallet holds; owner is the wallet account
    property var pendingNames: []      // [{name, display, owner, stage, ready_at, tx}]: registrations still finishing
    property bool revealing: false     // a pending registration is being asked about
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
    readonly property bool mine: holder !== ""
    readonly property string chatAddress: record !== null && record.texts && /^[0-9a-f]{64}$/.test(record.texts.chat || "") ? record.texts.chat : ""
    property bool pageSourceOpen: false  // the page's source is shown under its row

    // The name's profile, as the holder set it. These entries are how the name
    // is shown, so they are drawn rather than listed. Each is checked against a
    // fixed form first: the holder writes them, and nothing else gets through.
    readonly property var presentationKeys: ["display", "bio", "avatar", "color", "theme", "font"]
    readonly property string profileDisplay: profileText("display")
    readonly property string profileBio: profileText("bio")
    readonly property string profileAvatar: /^data:image\/(png|jpeg|gif|webp);base64,[A-Za-z0-9+\/]+=*$/.test(profileText("avatar")) ? profileText("avatar") : ""
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
                    say(item.name + ".lez is yours.", false)
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
        pageSourceOpen = false
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
                if (missing !== "")
                    loadDeposit(missing)
                else
                    say("A name is 1 to 32 characters of a-z, 0-9 or a hyphen, such as jef.lez.", true)
                return
            }
            missing = ""
            record = found
            page = ask("page", [found.name])
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
            say(typeof job.done === "function" ? job.done(job.result) : job.done, false)
        else
            say("That was sent but has not taken effect, so it may have been refused. Check the name and try again.", true)
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
        accountField.text = record.account !== null ? record.account : ""
        pageArea.text = page !== null && page.text !== null ? page.text : ""
        newKeyField.text = ""
        newValueField.text = ""
        newOwnerField.text = ""
        confirmingRelease = false
        tool = "settings"
    }

    function saveAccount() {
        var name = record.name
        var account = accountField.text.trim()
        change("set_account", [name, account], name, function() {
            return ask("resolve", [name]).account === account
        }, name + ".lez now pays to " + shorten(account) + ".")
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
        }, name + ".lez no longer takes payments.")
    }

    /// Sets one entry; an empty value removes it.
    function saveText(key, value) {
        var name = record.name
        key = key.trim()
        change("set_text", [name, key, value], name, function() {
            var texts = ask("resolve", [name]).texts
            return value === "" ? texts[key] === undefined : texts[key] === value
        }, value === "" ? "Removed " + key + " from " + name + ".lez." : "Saved " + key + " on " + name + ".lez.")
    }

    /// The Profile fields, with the text-entry key each one sets.
    readonly property var profileFields: [
        { key: "display", label: "Display name", hint: "How you want to be shown" },
        { key: "bio", label: "Bio", hint: "A line about you" },
        { key: "avatar", label: "Avatar", hint: "data:image/png;base64,… (names profile --avatar)" },
        { key: "theme", label: "Theme", hint: "light, dark or auto" },
        { key: "color", label: "Color", hint: "#rrggbb, such as #96d678" },
        { key: "font", label: "Font", hint: "serif, mono or rounded" },
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
        }, "Saved the profile of " + name + ".lez.")
    }

    /// Publishes the page; an empty body removes it. A body that opens with a
    /// tag is published as HTML, anything else as plain text.
    function savePage(body) {
        var name = record.name
        var kind = /^\s*</.test(body) ? "text/html; charset=utf-8" : "text/plain; charset=utf-8"
        change("set_page", [name, kind, body], name, function() {
            var published = ask("page", [name])
            return body === "" ? published === null : published !== null && published.text === body
        }, body === "" ? "Removed the page of " + name + ".lez." : "Published the page of " + name + ".lez.")
    }

    function showAccountAsName() {
        var name = record.name
        var account = holder
        change("set_primary", [account, name], name, function() {
            var shown = ask("reverse", [account])
            return shown !== null && shown.name === name
        }, shorten(account) + " is now shown as " + name + ".lez.")
    }

    function transferName() {
        var name = record.name
        var newOwner = newOwnerField.text.trim()
        var before = record.owner_tag
        change("transfer", [name, newOwner], name, function() {
            return ask("resolve", [name]).owner_tag !== before
        }, name + ".lez now belongs to " + shorten(newOwner) + ".")
    }

    function releaseName() {
        var name = record.name
        change("release", [name], name, function() {
            return ask("resolve", [name]) === null
        }, function(result) {
            return result && result.refund ? "Released " + name + ".lez. " + grouped(result.refund) + " returned to you." : "Released " + name + ".lez."
        })
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

    /// A name as people read it: `jef.lez`.
    function shown(name) {
        return name + ".lez"
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

    /// The page's source as plain text. It is never rendered: a page is its
    /// holder's words, and nothing in it should be able to act inside this app.
    function pageSource() {
        if (page === null || page.text === null || page.text === undefined)
            return "This page is not text."
        return String(page.text).substr(0, 4000).replace(/\s+$/, "")
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

    /// A name with its suffix. The name leads; `.lez` follows in smaller,
    /// quieter capitals, so it reads as a mark rather than part of the word.
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
            visible: label.name !== ""
            text: ".lez"
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
                font.pixelSize: 16
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
                        text: accountRow.modelData.name ? accountRow.modelData.name + ".lez" : ""
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
    /// drawn crisp so pixel art stays pixel art, otherwise the owner's seal.
    /// The plate behind it is plain slate, so the round cut is painted over
    /// the corners rather than masked, which needs no extra Qt module.
    component Avatar: Item {
        id: avatar
        property string source: ""
        property string owner: ""
        property string letter: ""
        property color ring: root.brass
        readonly property bool showsImage: picture.status === Image.Ready
        Seal {
            anchors.fill: parent
            visible: !avatar.showsImage
            owner: avatar.owner
            letter: avatar.letter
        }
        Image {
            id: picture
            anchors.fill: parent
            visible: avatar.showsImage
            source: avatar.source
            sourceSize: Qt.size(256, 256)
            fillMode: Image.PreserveAspectCrop
            smooth: false
            mipmap: false
            asynchronous: false
            cache: false
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
        onShowsImageChanged: cut.requestPaint()
        onWidthChanged: cut.requestPaint()
    }

    // ── Layout ──────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: root.slate }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Search, and the holder's own names.
        Rectangle {
            Layout.preferredWidth: 320
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
                        hint: "Type a name"
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
                        count: root.myNames.length + root.pendingNames.length > 0 ? root.myNames.length + root.pendingNames.length : ""
                    }
                    Repeater {
                        model: root.pendingNames
                        Text {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.leftMargin: 8
                            text: (modelData.display ? modelData.display : modelData.name + ".lez") + ": registering" + (modelData.ready_at ? ", ready at " + root.clock(modelData.ready_at) : "")
                            color: root.dim
                            font.family: root.sans
                            font.pixelSize: 17
                            elide: Text.ElideRight
                        }
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
                                    size: 19
                                    weight: Font.Medium
                                }
                                Text {
                                    visible: !!myRow.modelData.lapsed && !myRow.modelData.never
                                    text: "Lapsed"
                                    color: root.alarm
                                    font.family: root.sans
                                    font.pixelSize: 16
                                }
                                IconButton {
                                    kind: "gear"
                                    onPressed: root.manage(myRow.modelData.name)
                                }
                            }
                        }
                    }
                    Hint {
                        visible: root.myNames.length === 0 && root.pendingNames.length === 0 && root.wallet.length > 0
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
                        text: "App settings"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: 28
                    }
                    Part { text: "Registry" }
                    Hint { text: "Everyone using the same registry sees the same .lez names." }
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
                    Hint { text: "The LEZ wallet that signs for your names. It opens by itself when needed." }
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
                    visible: root.record === null && root.missing === "" && !root.settingsOpen
                    Layout.fillWidth: true
                    Layout.topMargin: 60
                    spacing: 10
                    Text {
                        text: !root.configKnown ? "Starting" : root.registry === "" ? "Choose a registry to begin" : "Look up a .lez name"
                        color: root.ink
                        font.family: root.sans
                        font.weight: Font.DemiBold
                        font.pixelSize: 36
                    }
                    Hint {
                        font.pixelSize: 18
                        color: root.ink
                        text: !root.configKnown ? "Waiting for the names module to answer." : root.registry === "" ? "Open App settings and enter the registry's program ID." : "Type a name to see how to pay or reach its holder. If nobody holds it, you can register it."
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
                    Text {
                        text: "Nobody holds this name."
                        color: root.ink
                        font.family: root.sans
                        font.pixelSize: 19
                    }
                    Panel {
                        visible: root.wallet.length > 0
                        title: "Register it to"
                        Accounts {}
                        Hint { text: "This account holds the name and pays the deposit. Payments to the name go to your private account." }
                        Part { text: "Deposit" }
                        Hint {
                            font.pixelSize: 20
                            color: root.ink
                            text: root.deposit === null ? "Checking the deposit." : root.deposit.tier === "locked" ? "One- and two-character names are reserved." : root.deposit.deposit === null || root.deposit.deposit === undefined ? (root.deposit.fee ? root.grouped(root.deposit.fee) + " to register" : "The registry is not set up for registering yet.") : root.grouped(root.deposit.deposit)
                        }
                        Hint {
                            visible: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode === "deposit"
                            text: (root.deposit !== null && root.deposit.tier === "three" ? "Three-character names take a larger deposit. " : "") + "Refunded in full when you release the name, which never expires. Network fee extra."
                        }
                        Action {
                            label: "Register " + root.missing + ".lez"
                            strong: true
                            changes: true
                            enabledNow: root.deposit !== null && root.deposit.tier !== "locked" && root.deposit.mode !== "unconfigured"
                            onPressed: root.registerName()
                        }
                    }
                    ColumnLayout {
                        visible: root.wallet.length === 0
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
                        Layout.fillWidth: true
                        spacing: 22
                        Avatar {
                            Layout.preferredWidth: 104
                            Layout.preferredHeight: 104
                            Layout.alignment: Qt.AlignTop
                            source: root.profileAvatar
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
                                maximumLineCount: 4
                                elide: Text.ElideRight
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Action {
                            visible: root.record !== null && (root.record.account !== null || root.record.npk !== null)
                            label: "Send"
                            strong: true
                            tint: root.accent
                            onPressed: root.openPay()
                        }
                        Action {
                            visible: root.chatAddress !== ""
                            label: "Message"
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
                            visible: root.record !== null && root.record.lapsed && !root.record.never
                            color: root.alarm
                            text: root.record !== null && root.record.lapsed && !root.record.never ? root.shown(root.record.name) + " lapsed on " + root.dated(root.record.expires) + ". Check with its holder before you pay." : ""
                        }
                        Hint { text: root.record === null ? "" : (root.record.npk !== null ? root.payHidden(true) : root.payHidden(false)) + (root.record.npk !== null ? " Private payments take a few minutes to prove." : "") }
                    }

                    // The holder's settings for this name.
                    Panel {
                        visible: root.tool === "settings"
                        title: root.record !== null ? "Settings for " + root.shown(root.record.name) : ""

                        Part { text: "Profile" }
                        Hint { text: "All optional. Shown wherever your name is." }
                        Repeater {
                            model: root.profileFields
                            RowLayout {
                                id: profileRow
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    Layout.preferredWidth: 110
                                    text: profileRow.modelData.label
                                    color: root.dim
                                    font.family: root.sans
                                    font.pixelSize: 17
                                    elide: Text.ElideRight
                                }
                                Field {
                                    Layout.fillWidth: true
                                    size: 19
                                    hint: profileRow.modelData.hint
                                    text: root.record !== null && root.record.texts && root.record.texts[profileRow.modelData.key] !== undefined ? root.record.texts[profileRow.modelData.key] : ""
                                    onTextChanged: {
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

                        Part { text: "Where it is paid" }
                        Hint {
                            text: root.record === null ? "" : root.record.npk !== null && root.record.account === null ? "Private payments only. Nobody can link them to your accounts."
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
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Field {
                                id: accountField
                                Layout.fillWidth: true
                                code: true
                                hint: "Or a public account ID (visible to anyone)"
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

                        Part { text: "Page" }
                        Area {
                            id: pageArea
                            Layout.fillWidth: true
                            hint: "Plain text or HTML, public to anyone"
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
                        Hint { text: root.record !== null ? "Apps then show " + root.shorten(root.holder) + " as " + root.shown(root.record.name) + ". This links the two publicly." : "" }
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
                        Hint { text: "Payments, entries and page are cleared for the new holder." }

                        Part { text: "Release" }
                        Hint { text: "Clears everything and refunds your deposit. Anyone can then register the name." }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Action {
                                visible: !root.confirmingRelease
                                label: root.record !== null ? "Release " + root.shown(root.record.name) : ""
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
                            Item { Layout.fillWidth: true }
                            Action { label: "Done"; onPressed: root.tool = "" }
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: root.line }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Fact {
                            label: "Holder"
                            value: root.record === null ? "" : root.mine ? "You · " + root.shorten(root.holder) : "Held privately"
                            full: root.mine ? root.holder : ""
                        }
                        Fact {
                            label: "Payments"
                            value: root.paymentsText()
                        }
                        Fact {
                            visible: root.record !== null && root.record.account !== null
                            label: "Pays to"
                            code: true
                            value: root.record !== null && root.record.account !== null ? root.shorten(root.record.account) : ""
                            full: root.record !== null && root.record.account !== null ? root.record.account : ""
                        }
                        Fact {
                            visible: root.record !== null && !root.record.never
                            label: root.record !== null && root.record.lapsed ? "Lapsed" : "Held until"
                            value: root.record === null || root.record.never ? "" : root.dated(root.record.expires)
                        }
                        Fact {
                            visible: root.record !== null && root.someAmount(root.record.deposit)
                            label: "Deposit"
                            value: root.record !== null && root.someAmount(root.record.deposit) ? root.grouped(root.record.deposit) + ", refunded when released" : ""
                        }
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

                        // The page: what it is, and its source on request. It is
                        // never rendered here (see pageSource).
                        RowLayout {
                            visible: root.page !== null
                            Layout.fillWidth: true
                            spacing: 12
                            Text {
                                Layout.preferredWidth: 120
                                text: "Page"
                                color: root.dim
                                font.family: root.sans
                                font.weight: Font.Medium
                                font.pixelSize: 18
                            }
                            Text {
                                Layout.fillWidth: true
                                text: root.pageSummary() + (root.record !== null ? " · open with names open " + root.shown(root.record.name) : "")
                                textFormat: Text.PlainText
                                color: root.ink
                                font.family: root.sans
                                font.weight: Font.Medium
                                font.pixelSize: 18
                                elide: Text.ElideRight
                            }
                            Link {
                                text: root.pageSourceOpen ? "Hide source" : "Show source"
                                onPressed: root.pageSourceOpen = !root.pageSourceOpen
                            }
                        }
                        Rectangle {
                            visible: root.page !== null && root.pageSourceOpen
                            Layout.fillWidth: true
                            Layout.leftMargin: 132
                            implicitHeight: sourceText.implicitHeight + 24
                            radius: 6
                            color: root.panel
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
