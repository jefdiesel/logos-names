// NFTs for Basecamp: launch collections, mint, and hold NFTs on the Logos
// Execution Zone, from the app's own private wallet.
//
// The `lez_nft` module runs every chain command as a background job with the
// app's wallet, which this screen sets up once: a public account to top up
// from, and a private account that holds the funds, pays, mints and holds.
// Anything that private account signs is proven on this computer (about 5
// minutes) and the chain never learns the account.
//
// Images live on Logos Storage: this screen uploads and downloads them through
// Basecamp's `storage_module`, and the module decodes them into pixels, since
// the view's sandbox can't read files outside its folder. A sealed NFT's image
// is encrypted before upload and decrypted after download, by the CLI.

import QtQuick
import QtQuick.Layouts
import Logos.Theme

Rectangle {
    id: root
    color: slate

    // ── State ───────────────────────────────────────────────────────────

    property string tab: "wallet"      // wallet, launch, mint, private, portfolio, activity
    property var config: null          // the module's get_config
    property var account: null         // setup's answer: {public, private, receive_code}
    property var collections: []
    property var tokens: []
    property var jobList: []
    property var pictures: ({})        // cid (or cid.plain) → {width, height, pixels} or {error}
    property var pictureWanted: ({})   // cids being fetched
    property string notice: ""
    property bool noticeIsError: false
    property bool settingsOpen: false
    property var watching: ({})        // job id → {label, done, failed}
    property var uploads: ({})         // storage session → {done, failed}
    property var downloads: ({})       // cid → [{done, failed}]

    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModule
    readonly property bool walletReady: config !== null && config.wallet_ready && account !== null
    readonly property bool busyWriting: {
        for (var i = 0; i < jobList.length; i++)
            if (jobList[i].state === "running" && jobList[i].writer)
                return true
        return false
    }
    readonly property string privateAccount: account ? account.private.account : ""
    readonly property string publicAccount: account ? account.public.account : ""

    // ── Palette and type ────────────────────────────────────────────────

    readonly property color slate: Theme.palette.background
    readonly property color card: Theme.isDark ? Theme.palette.backgroundSecondary : Theme.palette.surfaceRaised
    readonly property color raised: Theme.isDark ? Theme.palette.surface : Theme.palette.backgroundButton
    readonly property color line: Theme.palette.border
    readonly property color ink: Theme.palette.text
    readonly property color dim: Theme.palette.textSecondary
    readonly property color faint: Theme.palette.textPlaceholder
    readonly property color accent: Theme.palette.primary
    readonly property color alarm: Theme.palette.error
    readonly property color good: Theme.palette.success
    function px(size) { return Math.round(size * (Theme.typography.scale || 1)) }
    readonly property string sans: "Avenir Next"
    readonly property string mono: "Menlo"

    // ── The Logos bridge ────────────────────────────────────────────────

    function parsed(reply) {
        for (var depth = 0; depth < 2 && typeof reply === "string"; depth++) {
            try { reply = JSON.parse(reply) } catch (error) { break }
        }
        return reply
    }

    /// A quick call to the NFT module: its value, or throws its error.
    function ask(method, args) {
        if (!bridgeReady)
            throw new Error("This screen needs to run inside Basecamp.")
        var reply = parsed(logos.callModule("lez_nft", method, args))
        if (!reply || reply.success === undefined)
            throw new Error("The NFT module did not answer. Is it installed?")
        if (!reply.success)
            throw new Error(reply.error)
        return reply.value
    }

    /// A call to Basecamp's storage module: its value, or throws its error.
    function storage(method, args) {
        var reply = parsed(logos.callModule("storage_module", method, args))
        if (reply && reply.success === false)
            throw new Error(reply.error || ("storage " + method + " failed"))
        return reply && reply.value !== undefined ? reply.value : reply
    }

    function say(message, isError) {
        var text = String(message).trim()
        if (text !== "" && !/[.?!]$/.test(text))
            text += "."
        notice = text.charAt(0).toUpperCase() + text.substr(1)
        noticeIsError = !!isError
    }

    /// Runs `lez-nft <args>` as a job; `done(value)` or `failed(message)` runs
    /// when it ends. `label` names it in Activity.
    function run(args, label, done, failed) {
        var job
        try {
            job = ask("start", [JSON.stringify(args)]).job
        } catch (error) {
            if (failed) failed(error.message)
            else say(error.message, true)
            return
        }
        var next = Object.assign({}, watching)
        next[job] = { label: label, done: done, failed: failed }
        watching = next
        refreshJobs()
        poll.start()
    }

    Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: {
            var ids = Object.keys(root.watching)
            if (ids.length === 0) {
                stop()
                return
            }
            var still = Object.assign({}, root.watching)
            for (var i = 0; i < ids.length; i++) {
                var state
                try { state = root.ask("job", [ids[i]]) } catch (error) { continue }
                if (state.state === "running")
                    continue
                var watcher = still[ids[i]]
                delete still[ids[i]]
                if (state.state === "done") {
                    if (watcher.done) watcher.done(state.value)
                } else {
                    var why = state.error || "it failed"
                    if (watcher.failed) watcher.failed(why)
                    else root.say(watcher.label + ": " + why, true)
                }
            }
            root.watching = still
            root.refreshJobs()
        }
    }

    function refreshJobs() {
        try { jobList = ask("jobs", []) } catch (error) {}
    }

    // ── Storage ─────────────────────────────────────────────────────────

    Component.onCompleted: {
        if (bridgeReady) {
            logos.onModuleEvent("storage_module", "storageUploadDone")
            logos.onModuleEvent("storage_module", "storageDownloadDone")
            logos.onModuleEvent("storage_module", "storageStart")
        }
        loadConfig()
    }

    Connections {
        target: root.bridgeReady ? logos : null
        ignoreUnknownSignals: true
        function onModuleEventReceived(moduleName, eventName, data) {
            if (moduleName !== "storage_module")
                return
            var payload = root.parsed(data && data.length ? data[0] : data) || {}
            if (eventName === "storageUploadDone") {
                var waiting = root.uploads[payload.sessionId]
                if (!waiting)
                    return
                var rest = Object.assign({}, root.uploads)
                delete rest[payload.sessionId]
                root.uploads = rest
                if (payload.success && payload.cid) waiting.done(payload.cid)
                else waiting.failed(payload.error || "the upload failed")
            } else if (eventName === "storageDownloadDone") {
                var cid = payload.sessionId
                var list = root.downloads[cid] || []
                var others = Object.assign({}, root.downloads)
                delete others[cid]
                root.downloads = others
                for (var i = 0; i < list.length; i++) {
                    if (payload.success) list[i].done()
                    else list[i].failed(payload.error || "the download failed")
                }
            }
        }
    }

    /// Uploads the file at `path` to Logos Storage and gives its CID to `done`.
    function upload(path, done, failed) {
        try {
            try { storage("start", []) } catch (ignored) {}
            var session = storage("uploadUrl", [path, 65536, true])
            if (!session)
                throw new Error("the storage node did not take the file; is Storage running?")
            var next = Object.assign({}, uploads)
            next[session] = { done: done, failed: failed }
            uploads = next
        } catch (error) {
            failed(error.message)
        }
    }

    /// Downloads `cid` to `path`; `done()` once it is there.
    function download(cid, path, done, failed) {
        var waiting = downloads[cid]
        var next = Object.assign({}, downloads)
        next[cid] = (waiting || []).concat([{ done: done, failed: failed }])
        downloads = next
        if (waiting)
            return
        try {
            try { storage("start", []) } catch (ignored) {}
            storage("downloadToUrl", [cid, path, false, 65536, false, true])
        } catch (error) {
            var rest = Object.assign({}, downloads)
            delete rest[cid]
            downloads = rest
            failed(error.message)
        }
    }

    function setPicture(key, value) {
        var next = Object.assign({}, pictures)
        next[key] = value
        pictures = next
    }

    /// Fetches, opens if sealed, and decodes the image `cid`, once; the
    /// result lands in `pictures` under the key `pictureKey` gives.
    function wantPicture(cid, sealedToken) {
        if (!cid || !/^[A-Za-z0-9]+$/.test(cid))
            return
        var key = pictureKey(cid, sealedToken)
        if (pictures[key] || pictureWanted[key])
            return
        var wanted = Object.assign({}, pictureWanted)
        wanted[key] = true
        pictureWanted = wanted
        function failed(why) { setPicture(key, { error: why }) }
        function decode(path) {
            try { setPicture(key, ask("thumbnail", [path, 160])) } catch (error) { failed(error.message) }
        }
        function opened(raw) {
            if (!sealedToken) {
                decode(raw)
                return
            }
            var plain = ask("cid_path", [cid, "plain"])
            if (plain.exists) {
                decode(plain.path)
                return
            }
            run(["open-file", "--token-id", sealedToken, "--in", raw, "--out", plain.path], "Open sealed image",
                function() { decode(plain.path) }, failed)
        }
        try {
            var raw = ask("cid_path", [cid, ""])
            if (raw.exists)
                opened(raw.path)
            else
                download(cid, raw.path, function() { opened(raw.path) }, failed)
        } catch (error) {
            failed(error.message)
        }
    }

    function pictureKey(cid, sealedToken) {
        return sealedToken ? cid + ".plain" : cid
    }

    // ── Loading ─────────────────────────────────────────────────────────

    function loadConfig() {
        try {
            config = ask("get_config", [])
        } catch (error) {
            configRetry.start()
            return
        }
        refreshJobs()
        if (config.wallet_ready && config.cli_found)
            loadAccount()
        if (Object.keys(watching).length === 0 && jobList.some(function(job) { return job.state === "running" }))
            poll.start()
    }

    Timer { id: configRetry; interval: 2000; onTriggered: root.loadConfig() }

    function loadAccount() {
        run(["setup"], "Wallet", function(value) {
            account = value
        }, function(why) {
            if (!/busy/.test(why)) say(why, true)
        })
    }

    function loadCollections() {
        run(["collections"], "Collections", function(value) {
            collections = value.collections.slice().reverse()
        })
    }

    function loadPortfolio() {
        run(["portfolio"], "Portfolio", function(value) {
            tokens = value.tokens
        })
    }

    function contentOf(item) {
        return item && item.content && typeof item.content === "object" ? item.content : {}
    }

    function shorten(id) {
        id = String(id || "")
        return id.length > 20 ? id.substr(0, 10) + "…" + id.substr(id.length - 6) : id
    }

    function amount(text) {
        var digits = String(text || "0")
        return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    function copy(text, what) {
        clipboard.text = text
        clipboard.selectAll()
        clipboard.copy()
        say(what + " copied", false)
    }

    TextEdit { id: clipboard; visible: false }

    // ── Actions ─────────────────────────────────────────────────────────

    /// Checks a picked image and returns its path, or throws.
    function checkedImage(path) {
        path = String(path).trim().replace(/^file:\/\//, "")
        if (path === "")
            throw new Error("Choose an image file")
        var info = ask("file_info", [path])
        if (!info.exists) throw new Error("There is no file at " + path)
        if (info.too_big) throw new Error("The image is over 20 MB")
        if (!info.image) throw new Error("That file is not a PNG, JPEG, GIF or WebP image")
        return path
    }

    function launch(form) {
        var path
        try { path = checkedImage(form.image) } catch (error) { say(error.message, true); return }
        var supply = parseInt(form.supply, 10)
        if (!(supply >= 1 && supply <= 1000000)) { say("Supply is 1 to 1,000,000", true); return }
        if (form.name.trim() === "") { say("Give the collection a name", true); return }
        say("Uploading the image to Logos Storage…", false)
        upload(path, function(cid) {
            var content = JSON.stringify({ name: form.name.trim(), description: form.description.trim(), image: cid })
            var args = ["create", "--content", content, "--supply", String(supply), "--price", form.price || "0", "--from", "private"]
            if (form.price && form.price !== "0")
                args.push("--payee", form.payee)
            say("Launching " + form.name.trim() + ": proving it on this computer, about 5 minutes", false)
            run(args, "Launch " + form.name.trim(), function(value) {
                say("Launched. Collection ID " + shorten(value.collection_id) + " is in Mint", false)
                launchForm.reset()
                tab = "mint"
                loadCollections()
            })
        }, function(why) { say("Upload failed: " + why, true) })
    }

    function buy(collection, payPrivately, to) {
        var args = ["buy", "--collection", collection.id, "--from", payPrivately ? "private" : "public"]
        if (to.trim() !== "")
            args.push("--to", to.trim())
        var name = contentOf(collection).name || shorten(collection.id)
        say(payPrivately ? "Minting from " + name + " privately: about 5 minutes" : "Minting from " + name + "…", false)
        run(args, "Mint " + name, function(value) {
            say("Minted #" + value.serial + " of " + name, false)
            loadCollections()
            loadPortfolio()
            loadAccount()
        })
    }

    function mintPrivate(form) {
        var path
        try { path = checkedImage(form.image) } catch (error) { say(error.message, true); return }
        if (form.name.trim() === "") { say("Give the NFT a name", true); return }
        if (form.encrypt && form.to.trim() !== "" && form.to.trim().indexOf("lez-nft-recv") !== 0) {
            say("A sealed NFT for someone else needs their receive code", true)
            return
        }
        function mint(cid, keyFile) {
            var content = JSON.stringify({ name: form.name.trim(), description: form.description.trim(), image: cid, sealed: form.encrypt })
            var args = ["mint", "--content", content, "--from", "private"]
            if (form.to.trim() !== "")
                args.push("--to", form.to.trim())
            if (form.encrypt)
                args.push("--encrypt", "--key-file", keyFile)
            say("Minting " + form.name.trim() + " privately: about 5 minutes", false)
            run(args, "Private NFT " + form.name.trim(), function(value) {
                // The content key now lives sealed on chain; the loose copy goes.
                if (keyFile !== "")
                    try { ask("remove_file", [keyFile]) } catch (ignored) {}
                say("Minted " + form.name.trim() + (form.to.trim() === "" ? ". It is in Portfolio" : " for its new owner"), false)
                privateForm.reset()
                loadPortfolio()
            })
        }
        say("Uploading the image to Logos Storage…", false)
        if (!form.encrypt) {
            upload(path, function(cid) { mint(cid, "") }, function(why) { say("Upload failed: " + why, true) })
            return
        }
        var sealedPath = config.files + "/sealed-" + Date.now() + ".bin"
        run(["seal-file", "--in", path, "--out", sealedPath], "Encrypt image", function(value) {
            upload(sealedPath, function(cid) { mint(cid, value.key_file) }, function(why) { say("Upload failed: " + why, true) })
        })
    }

    function transfer(token, to) {
        if (to.trim() === "") { say("Paste the new owner's receive code or account", true); return }
        say("Sending it: about 5 minutes", false)
        run(["transfer", "--token-id", token.token_id, "--to", to.trim()], "Send " + (contentOf(token).name || "NFT"), function() {
            say("Sent", false)
            loadPortfolio()
        })
    }

    function burn(token) {
        run(["burn", "--token-id", token.token_id], "Burn " + (contentOf(token).name || "NFT"), function() {
            say("Burned", false)
            loadPortfolio()
        })
    }

    function shield(amountText) {
        var digits = String(amountText).replace(/[, ]/g, "")
        if (!/^[0-9]+$/.test(digits) || digits === "0") { say("Type an amount", true); return }
        say("Moving " + amount(digits) + " into the private account: about 5 minutes", false)
        run(["shield", "--amount", digits],
            "Move funds to private", function() {
                say("Moved", false)
                loadAccount()
            })
    }

    // ── Layout ──────────────────────────────────────────────────────────

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        RowLayout {
            Layout.fillWidth: true
            spacing: 18
            Text {
                text: "NFTs"
                color: root.ink
                font.family: root.sans
                font.pixelSize: root.px(30)
                font.weight: Font.Bold
            }
            Repeater {
                model: [["wallet", "Wallet"], ["launch", "Launch"], ["mint", "Mint"], ["private", "Private NFT"], ["portfolio", "Portfolio"], ["activity", "Activity"]]
                delegate: Text {
                    required property var modelData
                    text: modelData[1]
                    color: root.tab === modelData[0] ? root.accent : root.dim
                    font.family: root.sans
                    font.pixelSize: root.px(17)
                    font.weight: root.tab === modelData[0] ? Font.DemiBold : Font.Normal
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            root.tab = modelData[0]
                            if (modelData[0] === "mint" && root.collections.length === 0) root.loadCollections()
                            if (modelData[0] === "portfolio" && root.tokens.length === 0) root.loadPortfolio()
                            if (modelData[0] === "activity") root.refreshJobs()
                        }
                    }
                }
            }
            Item { Layout.fillWidth: true }
            Text {
                visible: root.busyWriting
                text: "working…"
                color: root.dim
                font.family: root.sans
                font.pixelSize: root.px(15)
            }
            Action { label: "Settings"; onPressed: root.settingsOpen = !root.settingsOpen }
        }

        Rectangle {
            Layout.fillWidth: true
            visible: root.notice !== ""
            implicitHeight: noticeText.implicitHeight + 20
            radius: 6
            color: root.noticeIsError ? Qt.rgba(root.alarm.r, root.alarm.g, root.alarm.b, 0.14) : Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.14)
            Text {
                id: noticeText
                anchors.fill: parent
                anchors.margins: 10
                text: root.notice
                wrapMode: Text.Wrap
                color: root.noticeIsError ? root.alarm : root.ink
                font.family: root.sans
                font.pixelSize: root.px(16)
            }
        }

        Panel {
            visible: root.settingsOpen
            Layout.fillWidth: true
            ColumnLayout {
                width: parent.width
                spacing: 8
                Label { text: "Settings" ; strong: true }
                Label { text: "Command line: " + (root.config && root.config.cli ? root.config.cli : "~/.local/bin/lez-nft") }
                Label { text: "Wallet: " + (root.config && root.config.wallet ? root.config.wallet : "~/.config/lez-nft/wallet") }
                Label { text: "Program: " + (root.config && root.config.program ? root.config.program : "the command line's default") }
                Label { text: "These three change only by editing ~/.config/lez-nft/config.json, so no other app can point this one at another program or wallet." }
                Field { id: rpcField; Layout.fillWidth: true; hint: "Sequencer (https://…; empty for the default)"; text: root.config ? root.config.rpc : "" }
                Action {
                    label: "Save"
                    strong: true
                    onPressed: {
                        try {
                            root.config = root.ask("set_config", ["", "", "", rpcField.text])
                            root.settingsOpen = false
                            root.loadConfig()
                        } catch (error) {
                            root.say(error.message, true)
                        }
                    }
                }
            }
        }

        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: width
            contentHeight: pages.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            ColumnLayout {
                id: pages
                width: parent.width
                spacing: 16

                // ── Wallet ──────────────────────────────────────────────
                ColumnLayout {
                    visible: root.tab === "wallet"
                    Layout.fillWidth: true
                    spacing: 14

                    Panel {
                        visible: root.config !== null && !root.config.cli_found
                        Layout.fillWidth: true
                        ColumnLayout {
                            width: parent.width
                            Label { text: "The lez-nft command line is missing"; strong: true }
                            Label { text: "The app runs it for everything on chain. Put it in ~/.local/bin, or name it as \"cli\" in ~/.config/lez-nft/config.json." }
                        }
                    }

                    Panel {
                        visible: root.config !== null && root.config.cli_found && !root.config.wallet_ready
                        Layout.fillWidth: true
                        ColumnLayout {
                            width: parent.width
                            spacing: 10
                            Label { text: "Set up the NFT wallet"; strong: true }
                            Label { text: "Once, this makes the app its own wallet: a public account to top up, and a private account that holds the funds, mints and holds your NFTs. Nothing the private account does names it on chain." }
                            Action {
                                label: "Set up"
                                strong: true
                                onPressed: root.run(["setup"], "Set up wallet", function(value) {
                                    root.account = value
                                    root.config = root.ask("get_config", [])
                                    root.say("Your NFT wallet is ready. Write down its recovery phrase from Activity, then top it up", false)
                                })
                            }
                        }
                    }

                    Panel {
                        visible: root.walletReady
                        Layout.fillWidth: true
                        ColumnLayout {
                            width: parent.width
                            spacing: 10
                            RowLayout {
                                Label { text: "Private account"; strong: true; Layout.fillWidth: true }
                                Action { label: "Refresh"; onPressed: root.loadAccount() }
                            }
                            Label { text: root.amount(root.account ? root.account.private.balance : "0") + " — pays, mints and holds. Fee-free; each action is proven here (about 5 minutes)." }
                            Code { text: root.privateAccount; onCopy: root.copy(root.privateAccount, "Private account") }
                            Label { text: "Receive code: give it to anyone sending you an NFT, sealed or not." }
                            Code { text: root.shorten(root.account ? root.account.receive_code : ""); onCopy: root.copy(root.account.receive_code, "Receive code") }
                        }
                    }

                    Panel {
                        visible: root.walletReady
                        Layout.fillWidth: true
                        ColumnLayout {
                            width: parent.width
                            spacing: 10
                            Label { text: "Top up"; strong: true }
                            Label { text: "Public account: " + root.amount(root.account ? root.account.public.balance : "0") + ". Send funds here from LEZ Wallet, then move them into the private account." }
                            Code { text: root.publicAccount; onCopy: root.copy(root.publicAccount, "Public account") }
                            RowLayout {
                                Field { id: shieldAmount; Layout.preferredWidth: 220; hint: "Amount"; numbersOnly: true }
                                Action { label: "Move to private"; strong: true; enabledNow: !root.busyWriting; onPressed: root.shield(shieldAmount.text) }
                            }
                        }
                    }
                }

                // ── Launch ──────────────────────────────────────────────
                Panel {
                    visible: root.tab === "launch"
                    Layout.fillWidth: true
                    ColumnLayout {
                        id: launchForm
                        width: parent.width
                        spacing: 10
                        function reset() { launchName.text = ""; launchDescription.text = ""; launchImage.text = ""; launchSupply.text = ""; launchPrice.text = "" }
                        Label { text: "Launch a collection"; strong: true }
                        Label { text: "Minted in order, 1 to its supply, from anyone's wallet. The creator is the app's private account, which the chain never sees." }
                        Field { id: launchName; Layout.fillWidth: true; hint: "Name" }
                        Field { id: launchDescription; Layout.fillWidth: true; hint: "Description" }
                        ImagePick { id: launchImage; Layout.fillWidth: true }
                        RowLayout {
                            Field { id: launchSupply; Layout.preferredWidth: 200; hint: "Supply"; numbersOnly: true }
                            Field { id: launchPrice; Layout.preferredWidth: 260; hint: "Price (0 = free)"; numbersOnly: true }
                        }
                        RowLayout {
                            visible: launchPrice.text !== "" && launchPrice.text !== "0"
                            Label { text: "Payments go to" }
                            Choice { id: launchPayee; options: [["private", "My private account"], ["public", "My public account"]] }
                        }
                        Label {
                            visible: launchPayee.value === "private" && launchPrice.text !== "" && launchPrice.text !== "0"
                            text: "Each sale arrives as a new private account under your keys; the wallet picks them up when it syncs."
                        }
                        Action {
                            label: "Launch"
                            strong: true
                            enabledNow: root.walletReady && !root.busyWriting
                            onPressed: root.launch({ name: launchName.text, description: launchDescription.text, image: launchImage.text,
                                                     supply: launchSupply.text, price: launchPrice.text, payee: launchPayee.value })
                        }
                    }
                }

                // ── Mint ────────────────────────────────────────────────
                ColumnLayout {
                    visible: root.tab === "mint"
                    Layout.fillWidth: true
                    spacing: 14
                    RowLayout {
                        Field { id: openId; Layout.fillWidth: true; hint: "Collection ID (64 hex), or pick one below"; code: true }
                        Action {
                            label: "Open"
                            onPressed: root.run(["collection", "--id", openId.text.trim()], "Collection", function(value) {
                                var rest = root.collections.filter(function(c) { return c.id !== value.id })
                                root.collections = [value].concat(rest)
                            })
                        }
                        Action { label: "Refresh"; onPressed: root.loadCollections() }
                    }
                    Label { visible: root.collections.length === 0; text: "No collections yet. Launch one, or open one by its ID." }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 14
                        Repeater {
                            model: root.collections
                            delegate: CollectionCard { required property var modelData; collection: modelData }
                        }
                    }
                }

                // ── Private NFT ─────────────────────────────────────────
                Panel {
                    visible: root.tab === "private"
                    Layout.fillWidth: true
                    ColumnLayout {
                        id: privateForm
                        width: parent.width
                        spacing: 10
                        function reset() { privateName.text = ""; privateDescription.text = ""; privateImage.text = ""; privateTo.text = "" }
                        Label { text: "Make a private NFT"; strong: true }
                        Label { text: "A one-off, made by your private account. Keep it, or give it to someone's private account with their receive code. Sealed, only its owner (and you) can see the picture and text. Anyone who held it before keeps that access, since a key once shared can't be taken back." }
                        Field { id: privateName; Layout.fillWidth: true; hint: "Name" }
                        Field { id: privateDescription; Layout.fillWidth: true; hint: "Description" }
                        ImagePick { id: privateImage; Layout.fillWidth: true }
                        Field { id: privateTo; Layout.fillWidth: true; hint: "For: empty for me, or their receive code (lez-nft-recv2:…) or account"; code: true }
                        RowLayout {
                            Toggle { id: privateSeal; checked: true }
                            Label { text: privateSeal.checked ? "Sealed: encrypted to its owner (past owners keep access)" : "Open: anyone can see it" }
                        }
                        Action {
                            label: "Mint privately"
                            strong: true
                            enabledNow: root.walletReady && !root.busyWriting
                            onPressed: root.mintPrivate({ name: privateName.text, description: privateDescription.text, image: privateImage.text,
                                                          to: privateTo.text, encrypt: privateSeal.checked })
                        }
                    }
                }

                // ── Portfolio ───────────────────────────────────────────
                ColumnLayout {
                    visible: root.tab === "portfolio"
                    Layout.fillWidth: true
                    spacing: 14
                    RowLayout {
                        Label { text: root.tokens.length + (root.tokens.length === 1 ? " NFT" : " NFTs") + " in this wallet"; Layout.fillWidth: true }
                        Action { label: "Refresh"; onPressed: root.loadPortfolio() }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 14
                        Repeater {
                            model: root.tokens
                            delegate: TokenCard { required property var modelData; token: modelData }
                        }
                    }
                }

                // ── Activity ────────────────────────────────────────────
                ColumnLayout {
                    visible: root.tab === "activity"
                    Layout.fillWidth: true
                    spacing: 10
                    Repeater {
                        model: root.jobList
                        delegate: Panel {
                            required property var modelData
                            Layout.fillWidth: true
                            ColumnLayout {
                                width: parent.width
                                spacing: 4
                                Label {
                                    strong: true
                                    text: (modelData.args && modelData.args.length ? modelData.args[0] : "job") + " — " + modelData.state
                                    color: modelData.state === "failed" ? root.alarm : modelData.state === "done" ? root.good : root.ink
                                }
                                Label { visible: !!modelData.error; text: String(modelData.error || "") }
                                Text {
                                    Layout.fillWidth: true
                                    text: String(modelData.log || "").split("\n").slice(-12).join("\n")
                                    wrapMode: Text.WrapAnywhere
                                    color: root.dim
                                    font.family: root.mono
                                    font.pixelSize: root.px(12)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Parts ───────────────────────────────────────────────────────────

    component Panel: Rectangle {
        default property alias content: holder.data
        implicitHeight: holder.childrenRect.height + 32
        radius: 8
        color: root.card
        border.width: 1
        border.color: root.line
        Item {
            id: holder
            anchors.fill: parent
            anchors.margins: 16
        }
    }

    component Label: Text {
        property bool strong: false
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        color: strong ? root.ink : root.dim
        font.family: root.sans
        font.pixelSize: root.px(strong ? 19 : 15)
        font.weight: strong ? Font.DemiBold : Font.Normal
    }

    component Action: Rectangle {
        id: action
        property string label: ""
        property bool strong: false
        property bool enabledNow: true
        signal pressed()
        implicitWidth: actionText.implicitWidth + 28
        implicitHeight: 36
        radius: 6
        color: !enabledNow ? "transparent" : strong ? (actionArea.containsMouse ? Qt.lighter(root.accent, 1.08) : root.accent) : (actionArea.containsMouse ? root.raised : "transparent")
        border.width: strong && enabledNow ? 0 : 1
        border.color: root.line
        opacity: enabledNow ? 1 : 0.5
        Text {
            id: actionText
            anchors.centerIn: parent
            text: action.label
            color: action.strong && action.enabledNow ? root.slate : root.ink
            font.family: root.sans
            font.pixelSize: root.px(16)
            font.weight: Font.DemiBold
        }
        MouseArea {
            id: actionArea
            anchors.fill: parent
            hoverEnabled: true
            enabled: action.enabledNow
            cursorShape: Qt.PointingHandCursor
            onClicked: action.pressed()
        }
    }

    component Field: Rectangle {
        id: field
        property alias text: input.text
        property string hint: ""
        property bool numbersOnly: false
        property bool code: false
        implicitHeight: 40
        implicitWidth: 240
        radius: 6
        color: root.slate
        border.width: 1
        border.color: input.activeFocus ? root.accent : root.line
        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            verticalAlignment: TextInput.AlignVCenter
            color: root.ink
            selectionColor: root.accent
            font.family: field.code ? root.mono : root.sans
            font.pixelSize: root.px(field.code ? 14 : 16)
            clip: true
            selectByMouse: true
            validator: field.numbersOnly ? digits : null
        }
        RegularExpressionValidator { id: digits; regularExpression: /[0-9]{0,30}/ }
        Text {
            anchors.fill: input
            verticalAlignment: Text.AlignVCenter
            visible: input.text === ""
            text: field.hint
            color: root.faint
            font.family: root.sans
            font.pixelSize: root.px(15)
            elide: Text.ElideRight
        }
    }

    /// A long value shown in mono, with a Copy button.
    component Code: RowLayout {
        id: code
        property string text: ""
        signal copy()
        Layout.fillWidth: true
        Text {
            Layout.fillWidth: true
            text: code.text
            color: root.ink
            font.family: root.mono
            font.pixelSize: root.px(13)
            elide: Text.ElideMiddle
        }
        Action { label: "Copy"; visible: code.text !== ""; onPressed: code.copy() }
    }

    component Toggle: Rectangle {
        id: toggle
        property bool checked: false
        implicitWidth: 46
        implicitHeight: 26
        radius: 13
        color: checked ? root.accent : root.raised
        border.width: 1
        border.color: root.line
        Rectangle {
            width: 20; height: 20; radius: 10
            y: 3
            x: toggle.checked ? toggle.width - width - 3 : 3
            color: root.slate
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: toggle.checked = !toggle.checked }
    }

    component Choice: Row {
        id: choice
        property var options: []
        property string value: options.length ? options[0][0] : ""
        spacing: 6
        Repeater {
            model: choice.options
            delegate: Action {
                required property var modelData
                label: modelData[1]
                strong: choice.value === modelData[0]
                onPressed: choice.value = modelData[0]
            }
        }
    }

    /// A path to an image on this computer, with a preview once it reads.
    component ImagePick: ColumnLayout {
        id: pick
        property alias text: pathField.text
        property var preview: null
        spacing: 8
        RowLayout {
            Field { id: pathField; Layout.fillWidth: true; hint: "Image file: drag it here from Finder, or type its path"; code: true }
            Action {
                label: "Preview"
                onPressed: {
                    try {
                        pick.preview = root.ask("thumbnail", [root.checkedImage(pathField.text), 160])
                    } catch (error) {
                        pick.preview = null
                        root.say(error.message, true)
                    }
                }
            }
        }
        DropArea {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            onDropped: function(drop) { if (drop.hasUrls) pathField.text = String(drop.urls[0]).replace(/^file:\/\//, "") }
        }
        Picture { visible: pick.preview !== null; picture: pick.preview; Layout.preferredWidth: 160; Layout.preferredHeight: 160 }
    }

    /// Pixels the module decoded: {width, height, pixels (base64 RGBA8)}.
    component Picture: Rectangle {
        id: picture
        property var picture: null
        property string placeholder: ""
        color: root.raised
        radius: 6
        clip: true
        onPictureChanged: art.requestPaint()
        onWidthChanged: art.requestPaint()
        Canvas {
            id: art
            anchors.fill: parent
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var p = picture.picture
                if (!p || !p.pixels || !p.width)
                    return
                var bytes = base64Bytes(p.pixels)
                if (bytes.length !== p.width * p.height * 4)
                    return
                // Qt's putImageData draws nothing, so each run of one colour in
                // a row is a filled block, scaled to fit.
                var w = p.width, h = p.height
                var scale = Math.min(width / w, height / h)
                var left = (width - w * scale) / 2, top = (height - h * scale) / 2
                for (var y = 0; y < h; y++) {
                    var y0 = Math.round(top + y * scale), y1 = Math.round(top + (y + 1) * scale)
                    var x = 0
                    while (x < w) {
                        var at = (y * w + x) * 4
                        var run = x + 1
                        while (run < w && bytes[at] === bytes[(y * w + run) * 4] && bytes[at + 1] === bytes[(y * w + run) * 4 + 1]
                               && bytes[at + 2] === bytes[(y * w + run) * 4 + 2] && bytes[at + 3] === bytes[(y * w + run) * 4 + 3])
                            run++
                        if (bytes[at + 3] > 0) {
                            ctx.fillStyle = "rgba(" + bytes[at] + "," + bytes[at + 1] + "," + bytes[at + 2] + "," + (bytes[at + 3] / 255) + ")"
                            var x0 = Math.round(left + x * scale), x1 = Math.round(left + run * scale)
                            ctx.fillRect(x0, y0, x1 - x0, y1 - y0)
                        }
                        x = run
                    }
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: !picture.picture || !picture.picture.pixels
            width: parent.width - 16
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: picture.picture && picture.picture.error ? "Image not available" : picture.placeholder
            color: root.faint
            font.family: root.sans
            font.pixelSize: root.px(13)
        }
    }

    function base64Bytes(text) {
        var table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        var out = []
        var bits = 0, count = 0
        for (var i = 0; i < text.length; i++) {
            var value = table.indexOf(text.charAt(i))
            if (value < 0)
                continue
            bits = ((bits << 6) | value) & 0xffffff
            count += 6
            if (count >= 8) {
                count -= 8
                out.push((bits >> count) & 255)
            }
        }
        return out
    }

    component CollectionCard: Rectangle {
        id: cc
        property var collection: null
        readonly property var content: root.contentOf(collection)
        readonly property bool priced: collection && collection.price !== "0"
        width: 252
        implicitHeight: ccColumn.implicitHeight + 24
        radius: 8
        color: root.card
        border.width: 1
        border.color: root.line
        Component.onCompleted: root.wantPicture(content.image, "")
        ColumnLayout {
            id: ccColumn
            x: 12; y: 12
            width: parent.width - 24
            spacing: 6
            Picture {
                Layout.preferredWidth: 228
                Layout.preferredHeight: 228
                picture: root.pictures[root.pictureKey(cc.content.image, "")] || null
                placeholder: "Fetching the image from Logos Storage…"
            }
            Label { text: cc.content.name || root.shorten(cc.collection.id); strong: true }
            Label { visible: !!cc.content.description; text: cc.content.description || "" }
            Label {
                text: cc.collection.minted + " / " + cc.collection.supply + " minted · "
                      + (cc.priced ? root.amount(cc.collection.price) + (cc.collection.payee === "private" ? " (paid privately)" : "") : "free")
            }
            Label { visible: !!cc.collection.mine; text: "You launched this"; color: root.accent }
            Code { text: cc.collection.id; onCopy: root.copy(cc.collection.id, "Collection ID") }
            Field { id: giftTo; Layout.fillWidth: true; hint: "For someone else? Receive code"; code: true; visible: !cc.collection.sold_out }
            RowLayout {
                visible: !cc.collection.sold_out
                Action { label: "Mint privately"; strong: true; enabledNow: root.walletReady && !root.busyWriting; onPressed: root.buy(cc.collection, true, giftTo.text) }
                Action { label: "Public"; enabledNow: root.walletReady && !root.busyWriting; onPressed: root.buy(cc.collection, false, giftTo.text) }
            }
            Label { visible: cc.collection.sold_out; text: "Sold out" }
        }
    }

    component TokenCard: Rectangle {
        id: tc
        property var token: null
        readonly property var content: root.contentOf(token)
        readonly property string sealedId: token && token.sealed ? token.token_id : ""
        property bool sending: false
        property bool burning: false
        width: 252
        implicitHeight: tcColumn.implicitHeight + 24
        radius: 8
        color: root.card
        border.width: 1
        border.color: root.line
        Component.onCompleted: if (token.readable) root.wantPicture(content.image, sealedId)
        ColumnLayout {
            id: tcColumn
            x: 12; y: 12
            width: parent.width - 24
            spacing: 6
            Picture {
                Layout.preferredWidth: 228
                Layout.preferredHeight: 228
                picture: root.pictures[root.pictureKey(tc.content.image, tc.sealedId)] || null
                placeholder: tc.token.readable ? "Fetching the image…" : "Sealed: this wallet can't open it"
            }
            Label {
                strong: true
                text: (tc.content.name || "NFT") + (tc.token.collection ? " #" + tc.token.collection.serial : "")
            }
            Label { visible: !!tc.content.description; text: tc.content.description || "" }
            Label { text: (tc.token.sealed ? "Sealed · " : "") + "held by " + root.shorten(tc.token.holder) }
            Code { text: tc.token.token_id; onCopy: root.copy(tc.token.token_id, "Token ID") }
            RowLayout {
                Action { label: "Send"; onPressed: { tc.sending = !tc.sending; tc.burning = false } }
                Action { label: "Burn"; onPressed: { tc.burning = !tc.burning; tc.sending = false } }
            }
            Field { id: sendTo; visible: tc.sending; Layout.fillWidth: true; hint: tc.token.sealed ? "Their receive code" : "Receive code or account"; code: true }
            Action { visible: tc.sending; label: "Send it"; strong: true; enabledNow: !root.busyWriting; onPressed: root.transfer(tc.token, sendTo.text) }
            Action { visible: tc.burning; label: "Burn it for good"; strong: true; enabledNow: !root.busyWriting; onPressed: root.burn(tc.token) }
        }
    }
}
