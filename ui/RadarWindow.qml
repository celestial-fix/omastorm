import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "Sites.js" as Sites
import "Keys.js" as KeyMap
import "Location.js" as Location
import "Timeline.js" as Timeline

Item {
    id: app
    // A standalone launcher owns its process; a plugin never does.
    property var session: null
    property var shell: null
    property var manifest: null
    readonly property var store: PluginSession
    property bool opened: session === null
    function open(payload) {
        opened = true;
        if (session) session.windowOpen = true;
        applyView();
        if (store.needsLocation || store.pendingLocationPicker) Qt.callLater(() => locationPicker.show(""));
    }
    function close() {
        if (!opened) return;
        opened = false;
        store.persist();
        if (session) session.windowOpen = false;
    }
    function dismiss() {
        if (!session) Qt.quit();
        else if (shell) shell.hide("com.omastorm.radar");
        else close();
    }
    readonly property var state: engine.state
    readonly property var scan: state ? state.frame : null
    // Every station, product, and source string on screen comes from the engine.
    readonly property string siteId: state ? state.site.id : ""
    readonly property string siteName: engine.site ? engine.site.name.toUpperCase() : ""
    readonly property string sourceBadge: state ? state.source.toUpperCase() : ""
    // The timeline (DESIGN.md): the station's frames oldest
    // first with the sweep in progress last; the engine owns the position.
    readonly property var frames: state ? state.timeline : []
    readonly property int frameIndex: scan ? frames.findIndex(f => f.id === scan.id) : -1
    readonly property bool newestShown: frameIndex >= 0 && frameIndex === frames.length - 1
    readonly property bool playing: state ? state.playing : false
    readonly property var newestComplete: { var done = frames.filter(f => f.status === "complete"); return done.length ? done[done.length - 1] : null; }
    // The connection condition while live (DESIGN.md):
    // the header's third row shows the age of the frame on screen beside
    // its time, and its status slot names the condition or the sweep in
    // progress. LIVE under ten minutes says only the age; STALE turns it
    // yellow; LOADING is accent; UNAVAILABLE and OFFLINE are red with the
    // last time. Archived, the badge and ARCHIVED SCAN say it all.
    readonly property string condition: state && state.source === "live" ? state.connection.status : ""
    readonly property bool alert: condition !== "" && condition !== "ok"
    readonly property color conditionColor: condition === "stale" ? theme.yellow
        : condition === "loading" ? theme.accent
        : condition === "unavailable" || condition === "offline" ? theme.red : theme.foreground
    // The engine gives the newest complete frame's age, ticking once a
    // second; an older frame on screen adds the distance between the two
    // scan times, so no local clock is consulted.
    readonly property int shownAge: !state || !scan || !scan.scanTime || !newestComplete ? -1
        : Math.max(0, state.connection.ageSeconds + Math.round((Date.parse(newestComplete.scanTime) - Date.parse(scan.scanTime)) / 1000))
    readonly property string ageText: condition && shownAge >= 0 ? ago(shownAge) : ""
    function ago(seconds) {
        var m = Math.floor(seconds / 60);
        if (m < 1) return "just now";
        if (m < 60) return m + " min ago";
        var h = Math.floor(m / 60);
        return h < 24 ? h + "h " + (m % 60) + "m ago" : Math.floor(h / 24) + "d " + (h % 24) + "h ago";
    }
    function lasting(seconds) {
        var m = Math.floor(seconds / 60), h = Math.floor(m / 60);
        return m < 60 ? m + " MIN" : h < 24 ? h + "H " + (m % 60) + "M" : Math.floor(h / 24) + "D " + (h % 24) + "H";
    }
    readonly property string sourceDetail: {
        if (!state) return "";
        if (state.source === "archived") return "ARCHIVED SCAN";
        var last = newestComplete ? clock(newestComplete.scanTime, true) : "";
        switch (condition) {
        case "loading": return siteId ? "LOADING · " + siteId : "NO STATION · PAN OR SEARCH";
        case "stale": return "STALE · LAST SWEEP " + last;
        case "unavailable": return !newestComplete ? siteId + " UNAVAILABLE · NO DATA"
            : siteId + " UNAVAILABLE" + (win.compact ? "" : " · NO DATA FOR " + lasting(state.connection.ageSeconds)) + " · LAST " + last;
        case "offline": return !scan.scanTime ? "OFFLINE · NOTHING CACHED"
            : "OFFLINE · " + (win.compact ? "" : "SHOWING ") + "CACHED " + clock(scan.scanTime, true) + " · " + ago(shownAge).toUpperCase();
        default: return scan.status === "partial" && scan.scanTime ? "SCANNING · " + Math.max(0, scan.rays - 1) + " RADIALS" : "";
        }
    }
    readonly property var aviation: state && state.aviation ? state.aviation : null
    readonly property var gramet: aviation && aviation.gramet ? aviation.gramet : null
    readonly property var history: state && state.history ? state.history : null
    readonly property string aviationLine: {
        if (!aviation || aviation.status === "idle") return "";
        if (aviation.status === "loading") return "AVIATION · LOADING";
        if (aviation.status === "offline") return "AVIATION · OFFLINE";
        if (aviation.status === "unavailable") return "AVIATION · NO BULLETIN";
        var id = aviation.station ? aviation.station.id : "";
        var cat = aviation.metar && aviation.metar.category ? aviation.metar.category + " · " : "";
        var raw = aviation.metar ? aviation.metar.raw : "";
        var n = aviation.hazards ? aviation.hazards.length : 0;
        var hazards = n ? " · " + n + " HAZARD" + (n === 1 ? "" : "S") : "";
        return (id ? id + " · " : "") + cat + (raw || "BRIEFING") + hazards;
    }
    readonly property string grametLine: {
        if (!gramet || gramet.status === "idle") return "";
        if (gramet.status === "loading") return "GRAMET · LOADING";
        if (gramet.status === "offline") return "GRAMET · OFFLINE";
        if (gramet.status === "unavailable") return "GRAMET · NO ROUTE";
        var o = gramet.origin && gramet.origin.icao ? gramet.origin.icao : "";
        var d = gramet.destination && gramet.destination.icao ? gramet.destination.icao : "";
        var route = o && d ? o + "–" + d : "ROUTE";
        var tas = gramet.cruiseKt ? " " + gramet.cruiseKt + " KT" : "";
        return "GRAMET · " + route + tas + (gramet.raw ? " · " + gramet.raw : "");
    }
    readonly property string historyLine: {
        if (!history || history.status === "idle") return "";
        var src = (history.source || layerSource || "archive").toUpperCase();
        if (history.status === "loading") return src + " · " + (history.time || "") + " · LOADING";
        if (history.status === "offline") return src + " · OFFLINE";
        if (history.status === "unavailable") return src + " · " + (history.time || "") + " · NO STATIONS";
        var n = history.stations ? history.stations.length : 0;
        return src + " · " + (history.time || "") + " · " + n + " STATION" + (n === 1 ? "" : "S");
    }
    // Clock readings are the machine's local time; the wire is UTC. `zone`
    // appends the zone's abbreviation where the reading stands alone.
    function clock(iso, zone) { return iso ? Qt.formatTime(new Date(iso), zone ? "HH:mm t" : "HH:mm") : ""; }
    function span(fromIso, toIso) {
        var minutes = Math.round((Date.parse(toIso) - Date.parse(fromIso)) / 60000);
        return minutes >= 60 ? Math.floor(minutes / 60) + "h " + (minutes % 60) + "m" : minutes + "m";
    }
    // One slot per tick: a frame, or a stub where the feed skipped one (a
    // gap of about two or more median intervals, at most three stubs).
    readonly property var slots: Timeline.slots(frames)
    readonly property int currentSlot: scan ? slots.findIndex(s => s.id === scan.id) : -1
    function togglePlay() { if (frames.length > 1) engine.send({type: playing ? "pause" : "play"}); }
    function step(delta) {
        if (app.historySource) { engine.send({type: "step_history", delta: delta}); return; }
        if (frames.length > 1) engine.send({type: "step", delta: delta});
    }
    function jump(toNewest) {
        if (app.historySource) { engine.send({type: "step_history", delta: toNewest ? 100000 : -100000}); return; }
        if (frames.length > 1) engine.send({type: "seek", id: frames[toNewest ? frames.length - 1 : 0].id});
    }
    readonly property int bands: scan ? scan.palette.length : 0
    function legendLabel(index) {
        var bounds = scan.bounds;
        return index === 0 ? "<" + bounds[1] : index === bands - 1 ? bounds[index] + "+" : String(bounds[index]);
    }
    // Where the weak-return floor cuts the legend strip, as a fraction of its
    // width: the bands are equal columns, so the floor interpolates inside
    // the band it falls in. 0 when the floor is off, under the scale, or not
    // in force because the frame carries no scale (an older engine).
    readonly property bool floorActive: !!scan && weakFloor !== null && scan.scale > 0
    readonly property real floorFraction: {
        if (!floorActive || weakFloor <= scan.bounds[0]) return 0;
        for (var i = 0; i < bands; i++)
            if (weakFloor < scan.bounds[i + 1]) return (i + (weakFloor - scan.bounds[i]) / (scan.bounds[i + 1] - scan.bounds[i])) / bands;
        return 1;
    }
    readonly property string weakKey: (bindings.weak || []).map(KeyMap.pretty)[0] || ""
    // Which legend numbers fit: the last always shows; each earlier one shows
    // only if it clears the previous shown number and the last one. This keeps
    // spacing even at any width and band count instead of hiding by parity.
    TextMetrics { id: legendMetrics; font.family: app.theme.font; font.pixelSize: 10; text: "0" }
    readonly property var legendShown: {
        var n = bands, shown = [];
        if (!n) return shown;
        var column = legendRow.width / n, gap = 8, last = (n - 1) * column, cursor = 0;
        for (var i = 0; i < n - 1; i++) {
            var x = i * column, right = x + legendLabel(i).length * legendMetrics.advanceWidth + gap;
            shown[i] = x >= cursor && right <= last;
            if (shown[i]) cursor = right;
        }
        shown[n - 1] = true;
        return shown;
    }
    Engine { id: engine }
    Connections { target: engine; function onStateChanged() { app.maybeApplyRememberedMode(); } }
    // Deliberate preferences and remembered view (DESIGN.md, location).
    // PluginSession owns config.toml, state.json, and the camera; this
    // window applies the view to its map and sends map-local tile requests.
    readonly property var config: store.config
    property bool applyingView: false
    Timer { id: applyingViewClear; interval: 250; onTriggered: app.applyingView = false }
    function applyView() {
        if (!store.hasView) return;
        applyingView = true;
        applyingViewClear.restart();
        map.holdSpan = true;
        map.lookAt(store.centerLat, store.centerLon);
        map.span = store.span;
        if (engine.state)
            engine.send({type: "view_center", lat: store.centerLat, lon: store.centerLon});
        Qt.callLater(() => { map.holdSpan = false; });
    }
    property bool viewApplied: false
    onStateChanged: {
        if (!state) viewApplied = false;
        else {
            store.initialize();
            if (!viewApplied) { applyView(); viewApplied = true; }
            maybeOfferLocation();
        }
    }
    function maybeOfferLocation() {
        if (!opened || !store.needsLocation || locationPicker.open) return;
        Qt.callLater(() => {
            if (app.opened && app.store.needsLocation && !locationPicker.open) locationPicker.show("");
        });
    }
    Connections {
        target: store
        function onViewChanged() {
            if (app.opened) app.applyView();
            app.maybeOfferLocation();
        }
        function onLocationPickerRequested() { if (app.opened) locationPicker.show(""); }
    }
    Connections {
        target: config
        function onReadyChanged() { app.applyView(); }
        function onKeysChanged() { app.applySettings(); }
        function onTreatmentChanged() { app.applySettings(); }
        function onWeakFloorChanged() { app.applySettings(); }
        function onValuesChanged() { app.applySettings(); }
    }
    // The keyboard map (DESIGN.md, keyboard map as built): Keys.js lays the
    // `[keys]` table over the defaults, asking Qt whether each sequence
    // parses; a bad value or a key bound twice keeps the default and is
    // named in the status slot, like a rejection, until the file is fixed.
    // The treatment and weak_floor settings are judged the same way.
    // OMASTORM_STYLE and OMASTORM_WEAK, set by the capture scripts, outrank
    // the file.
    property var bindings: ({})
    property var configErrors: []
    readonly property string configError: !configErrors.length ? "" : configErrors[0].toUpperCase() + (configErrors.length > 1 ? " (+" + (configErrors.length - 1) + " MORE)" : "")
    Shortcut { id: probe; enabled: false }
    function canon(sequence) { probe.sequence = sequence; return probe.portableText; }
    function applySettings() {
        var errors = Location.configErrors(config.values);
        var wanted = KeyMap.treatment(config.treatment, errors), floor = KeyMap.weakFloor(config.weakFloor, errors);
        var resolved = KeyMap.resolve(config.keys, canon);
        bindings = resolved.bindings;
        configErrors = errors.concat(resolved.errors);
        if (!session && !Quickshell.env("OMASTORM_STYLE") && wanted) treatment = wanted;
        if (!session && KeyMap.envFloor(Quickshell.env("OMASTORM_WEAK")) === undefined) weakFloor = floor;
    }
    Component.onCompleted: applySettings()
    readonly property bool overlayOpen: picker.open || locationPicker.open || grametPicker.open || sheet.open
    function run(action) {
        switch (action) {
        case "mode_radar": setMode("radar"); break;
        case "mode_weather": setMode("weather"); break;
        case "mode_aviation": setMode("aviation"); break;
        case "search": treatmentMenu.close(); picker.show(""); break;
        case "nearest": setMode("radar", true); nearest(); break;
        case "lock": setMode("radar", true); toggleLock(); break;
        case "home": locationPicker.show(""); break;
        case "pan_left": map.pan(-1, 0); break;
        case "pan_right": map.pan(1, 0); break;
        case "pan_up": map.pan(0, -1); break;
        case "pan_down": map.pan(0, 1); break;
        case "zoom_in": map.zoom(Math.min(map.span, map.maxSpan) / 1.25); break;
        case "zoom_out": map.zoom(Math.min(map.span, map.maxSpan) * 1.25); break;
        case "reset": resetView(); break;
        case "previous_frame": step(-1); break;
        case "next_frame": step(1); break;
        case "play": togglePlay(); break;
        case "oldest": jump(false); break;
        case "newest": jump(true); break;
        case "pixels": case "glyphs": case "stipple": setMode("radar", true); treatment = action.toUpperCase(); treatmentMenu.close(); break;
        case "layer_radar": setMode("radar", true); setLayer("REF", 0); break;
        case "layer_wind": if (!aviationMode) setMode("weather", true); setLayer("WIND", app.layerAltitude); break;
        case "layer_pressure": if (!aviationMode) setMode("weather", true); setLayer("PRES", app.layerAltitude); break;
        case "layer_water": if (!aviationMode) setMode("weather", true); setLayer("WATER", app.layerAltitude); break;
        case "layer_temp": setMode("weather", true); setLayer("TEMP", 0); break;
        case "layer_precip": setMode("weather", true); setLayer("PRECIP", 0); break;
        case "source_now": if (!aviationMode) setMode("weather", true); setSource("now"); break;
        case "source_gfs": if (!aviationMode) setMode("weather", true); setSource("gfs"); break;
        case "source_ecmwf": if (!aviationMode) setMode("weather", true); setSource("ecmwf"); break;
        case "source_wrf": setMode("weather", true); setSource("wrf"); break;
        case "source_cdo": setMode("weather", true); setSource("cdo"); break;
        case "source_meteostat": setMode("weather", true); setSource("meteostat"); break;
        case "run_wrf": setMode("weather", true); runWrf(); break;
        case "gramet": setMode("aviation", true); treatmentMenu.close(); grametPicker.show(""); break;
        case "altitude_down": stepAltitude(-1); break;
        case "altitude_up": stepAltitude(1); break;
        case "weak": weakFloor = weakFloor === null ? configuredFloor : null; break;
        case "help": treatmentMenu.close(); if (sheet.open) sheet.close(); else sheet.show(); break;
        case "close": dismiss(); break;
        }
    }
    // Drives the keyboard map from outside for checks and captures:
    // quickshell ipc --pid <pid> call keys run pan_left
    IpcHandler {
        target: "keys"
        function run(action: string): void { app.run(action); }
        function bindings(): string { return JSON.stringify(app.bindings); }
        function errors(): string { return JSON.stringify(app.configErrors); }
        function menu(open: bool): void { if (open) treatmentMenu.show(); else treatmentMenu.close(); }
        function field(name: string): string { var value = JSON.parse(status())[name]; return value === undefined ? "" : String(value); }
        function status(): string {
            return JSON.stringify({sheet: sheet.open, menu: treatmentMenu.opened, treatment: app.treatment, weakFloor: app.weakFloor === null ? "off" : app.weakFloor, error: app.configError,
                                   span: Math.round(map.span * 10) / 10, lat: Math.round(map.centerLat * 1000) / 1000, lon: Math.round(map.centerLon * 1000) / 1000,
                                   locationSource: app.store.locationSource, needsLocation: app.store.needsLocation,
                                   site: app.siteId, locked: app.locked, lockSource: app.store.lockSource, outsideCoverage: app.outsideCoverage,
                                   mode: app.appMode});
        }
    }
    // Site navigation (DESIGN.md, location): the lock pins the radar against
    // hand-offs; `n` releases it and selects the nearest radar without moving
    // the camera. The site picker locks and centres on that station.
    readonly property bool locked: state ? state.site.locked : false
    readonly property bool following: state ? state.site.follow && !state.site.locked : false
    readonly property var resetTarget: Location.resolveReset(Location.configCenter(config.values), config.location)
    readonly property bool outsideCoverage: {
        var s = engine.site;
        return !!(locked && s && Location.distanceKm(map.centerLat, map.centerLon, s.lat, s.lon) > map.coverageKm);
    }
    function toggleLock() {
        if (!state || !siteId) return;
        store.setLock(locked ? "" : siteId, !locked);
    }
    property string notice: ""
    Timer { id: noticeTimer; interval: 3000; onTriggered: app.notice = "" }
    function resetView() {
        store.resetView();
        applyView();
    }
    property string selectedSource: "nexrad"
    readonly property string layerSource: selectedSource || (scan && scan.layerSource) || (scan && scan.kind === "field" ? "now" : "nexrad")
    readonly property bool historySource: layerSource === "cdo" || layerSource === "meteostat"
    readonly property string appMode: store.mode || "radar"
    readonly property bool radarMode: appMode === "radar"
    readonly property bool weatherMode: appMode === "weather"
    readonly property bool aviationMode: appMode === "aviation"
    property int layerAltitude: 0
    function setMode(id, keepLayer) {
        var next = Location.parseMode(id);
        if (store.mode !== next) store.setMode(next);
        if (!keepLayer) applyModeDefaults(next);
    }
    property bool modeApplied: false
    function maybeApplyRememberedMode() {
        if (modeApplied || !state) return;
        modeApplied = true;
        if (appMode !== "radar") applyModeDefaults(appMode);
    }
    function applyModeDefaults(id) {
        if (!state || state.source !== "live") return;
        if (id === "radar") {
            setLayer("REF", 0);
            return;
        }
        if (id === "weather") {
            if (layerSource === "nexrad" || !selectedSource || selectedSource === "nexrad") {
                setSource("now");
                setLayer("TEMP", 0);
            }
            return;
        }
        if (layerSource === "nexrad" || historySource || layerSource === "wrf") {
            setSource("now");
            setLayer("WIND", layerAltitude);
        }
    }
    function setLayer(product, altitude) {
        if (!state) return;
        if (product === "REF") selectedSource = "nexrad";
        layerAltitude = product === "REF" ? 0 : Math.max(0, Number(altitude) || 0);
        engine.send({type: "set_product", product: product, elevationIndex: layerAltitude});
    }
    function wrfSpan() { return Math.round(Math.min(map.maxSpan, Math.max(80, map.span))); }
    function setSource(id) {
        if (!state) return;
        selectedSource = id;
        engine.send({type: "set_source", source: id});
        if (id === "wrf") {
            engine.send({type: "estimate_wrf", lat: map.centerLat, lon: map.centerLon, widthKm: wrfSpan(), heightKm: wrfSpan()});
        }
    }
    function runWrf() {
        if (!state) return;
        engine.send({type: "run_wrf", lat: map.centerLat, lon: map.centerLon, widthKm: wrfSpan(), heightKm: wrfSpan()});
    }
    function stepAltitude(delta) {
        if (!state || !state.layers) return;
        var product = scan && scan.kind === "field" ? scan.product : "";
        if (!product) return;
        var spec = state.layers.products.find(p => p.code === product);
        if (!spec || !spec.altitudes.length) return;
        var next = Math.max(0, Math.min(spec.altitudes.length - 1, layerAltitude + delta));
        setLayer(product, next);
    }
    function nearest() {
        var s = map.nearest();
        if (!state) return;
        if (!s) {
            app.notice = "NO NEXRAD WITHIN RANGE";
            noticeTimer.restart();
            return;
        }
        store.followNearest(s.id);
    }
    function choose(s) {
        if (!state || !s) return;
        store.chooseRadar(s.id, Number(s.lat), Number(s.lon), s.name || s.id);
        applyView();
    }
    // Drives the picker from outside for checks and captures:
    // quickshell ipc --pid <pid> call picker open tul
    IpcHandler {
        target: "picker"
        function open(query: string): void { picker.show(query); }
        function accept(): void { picker.accept(); }
        function close(): void { picker.close(); }
        function move(delta: int): void { picker.move(delta); }
        function matches(): string { return JSON.stringify(picker.rows.map(r => r.site.id)); }
        function status(): string { return JSON.stringify({open: picker.open, query: picker.query, selected: picker.selected, total: picker.ranked.total, focused: picker.fieldFocused}); }
    }
    IpcHandler {
        target: "location"
        function open(query: string): void { locationPicker.show(query); }
        function accept(): void { locationPicker.accept(); }
        function close(): void { locationPicker.close(); }
        function move(delta: int): void { locationPicker.move(delta); }
        function go(lat: string, lon: string, name: string): void { locationPicker.go(Number(lat), Number(lon), name); }
        function setLat(text: string): void { locationPicker.latText = text; }
        function setLon(text: string): void { locationPicker.lonText = text; }
        function matches(): string { return JSON.stringify(locationPicker.rows.map(r => r.where ? r.name + ", " + r.where : r.name)); }
        function status(): string { return JSON.stringify({open: locationPicker.open, query: locationPicker.query, selected: locationPicker.selected, focused: locationPicker.fieldFocused, count: locationPicker.rows.length, lat: locationPicker.latText, lon: locationPicker.lonText, error: locationPicker.coordError}); }
    }
    readonly property var theme: session ? session.theme.snapshot : themeInputs.snapshot
    Theme { id: themeInputs; registerIpc: !app.session }
    // Glyphs is the default; config.toml's `treatment` and the keys change it.
    property string treatment: session ? session.treatment : Quickshell.env("OMASTORM_STYLE") || "GLYPHS"
    onTreatmentChanged: if (session) session.treatment = treatment
    // The weak-return floor (DESIGN.md, weak-return floor): measured returns
    // under this many dBZ draw nothing, null draws them all. `w` toggles
    // between off and the configured floor (the default when config.toml
    // has none or says false); the popover inherits it through the session.
    property var weakFloor: session ? session.weakFloor : KeyMap.envFloor(Quickshell.env("OMASTORM_WEAK")) !== undefined ? KeyMap.envFloor(Quickshell.env("OMASTORM_WEAK")) : KeyMap.DEFAULT_FLOOR
    onWeakFloorChanged: if (session) session.weakFloor = weakFloor
    readonly property real configuredFloor: { var floor = KeyMap.weakFloor(config.weakFloor, []); return floor === null ? KeyMap.DEFAULT_FLOOR : floor; }
    Connections {
        target: app.session
        function onTreatmentChanged() { app.treatment = app.session.treatment; }
        function onWeakFloorChanged() { app.weakFloor = app.session.weakFloor; }
        function onLocationPickerRequested() { if (app.opened) locationPicker.show(""); }
    }
    // Quickshell keeps the process alive after its last window closes, which
    // left 400-600 MB orphans behind every close. Quit with the window; the
    // engine daemon is separate and stays up.
    Connections { target: Quickshell; function onLastWindowClosed() { if (!app.session) Qt.quit(); } }
    FloatingWindow {
        id: win
        title: "Omastorm"
        visible: app.opened
        onVisibleChanged: if (!visible && app.opened) app.dismiss()
        implicitWidth: Number(Quickshell.env("OMASTORM_WIDTH")) || 960
        implicitHeight: Number(Quickshell.env("OMASTORM_HEIGHT")) || 680
        minimumSize: Qt.size(360 * Math.max(1, app.theme.baseSize/12), 360 * Math.max(1, app.theme.baseSize/12))
        color: app.theme.background
        property bool compact: width < 560

        component LabelText: Text {
            color: app.theme.foreground
            font.family: app.theme.font
            font.pixelSize: app.theme.baseSize
            elide: Text.ElideRight
        }
        component Control: Button {
            id: button
            property bool selected: false
            implicitHeight: 30
            implicitWidth: Math.max(30, contentItem.implicitWidth + 18)
            padding: 6
            contentItem: LabelText {
                text: button.text
                color: button.selected ? app.theme.background : app.theme.foreground
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
            background: Rectangle {
                color: button.selected ? app.theme.accent : button.hovered || button.activeFocus ? Qt.alpha(app.theme.accent, .18) : "transparent"
                border.width: 1
                border.color: button.selected || button.activeFocus ? app.theme.accent : Qt.alpha(app.theme.foreground, .22)
            }
        }
        // Glyphs on the canvas's 16 px grid (DESIGN.md): transport,
        // lock, and follow, drawn rather than typed so the monospace font's
        // coverage does not decide their shape.
        component Glyph: Canvas {
            id: glyphCanvas
            property string glyph: "play"
            property color ink: app.theme.foreground
            property real fade: 1
            implicitWidth: 16
            implicitHeight: 16
            onInkChanged: requestPaint()
            onFadeChanged: requestPaint()
            onGlyphChanged: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();
                    ctx.clearRect(0, 0, width, height);
                    ctx.translate(Math.round((width - 16) / 2), Math.round((height - 16) / 2));
                    ctx.fillStyle = ink; ctx.strokeStyle = ink; ctx.lineWidth = 1.5; ctx.globalAlpha = fade;
                    function tri(x1, y1, x2, y2, x3, y3) { ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.lineTo(x3, y3); ctx.closePath(); ctx.fill(); }
                    function bar(x) { ctx.beginPath(); ctx.moveTo(x, 3); ctx.lineTo(x, 13); ctx.stroke(); }
                    function seg(x1, y1, x2, y2) { ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke(); }
                    switch (glyphCanvas.glyph) {
                    case "play": tri(4, 2.5, 4, 13.5, 12.5, 8); break;
                    case "pause": ctx.fillRect(4, 3, 3, 10); ctx.fillRect(9, 3, 3, 10); break;
                    case "back": bar(4); tri(13, 3.5, 13, 12.5, 6, 8); break;
                    case "fwd": bar(12); tri(3, 3.5, 3, 12.5, 10, 8); break;
                    case "first": bar(3); tri(8, 3.5, 8, 12.5, 4.5, 8); tri(14, 3.5, 14, 12.5, 10.5, 8); break;
                    case "last": bar(13); tri(2, 3.5, 2, 12.5, 5.5, 8); tri(8, 3.5, 8, 12.5, 11.5, 8); break;
                    case "lock": ctx.strokeRect(3.5, 7.5, 9, 6); ctx.beginPath(); ctx.moveTo(5.5, 7.5); ctx.lineTo(5.5, 5); ctx.arc(8, 5, 2.5, Math.PI, 0); ctx.lineTo(10.5, 7.5); ctx.stroke(); break;
                    case "follow": ctx.beginPath(); ctx.arc(8, 8, 4, 0, 2 * Math.PI); ctx.stroke(); seg(8, 1, 8, 4); seg(8, 12, 8, 15); seg(1, 8, 4, 8); seg(12, 8, 15, 8); break;
                    case "search": ctx.beginPath(); ctx.arc(6.5, 6.5, 4.5, 0, 2 * Math.PI); ctx.stroke(); seg(10, 10, 14, 14); break;
                    case "keys": ctx.strokeRect(1.5, 4.5, 13, 8); seg(4, 7, 5, 7); seg(7, 7, 8, 7); seg(10, 7, 11, 7); seg(4.5, 10, 11.5, 10); break;
                    case "chevron": seg(5, 6.5, 8, 9.5); seg(8, 9.5, 11, 6.5); break;
                    }
            }
        }
        // A 30 px control showing one glyph. Never takes keyboard focus: the
        // keys stay global.
        component GlyphButton: Button {
            id: transport
            property string glyph: "play"
            property bool selected: false
            implicitHeight: 30
            implicitWidth: 30
            padding: 0
            focusPolicy: Qt.NoFocus
            contentItem: Glyph {
                glyph: transport.glyph
                ink: transport.selected ? app.theme.background : app.theme.foreground
                fade: transport.enabled ? 1 : .35
            }
            background: Rectangle {
                color: transport.selected ? app.theme.accent : transport.hovered ? Qt.alpha(app.theme.accent, .18) : "transparent"
                border.width: 1
                border.color: transport.selected ? app.theme.accent : Qt.alpha(app.theme.foreground, .22)
            }
        }
        Rectangle {
          id: surface
          anchors.fill: parent
          color: app.theme.background
          // One Shortcut per action with the bindings in force (Keys.js has
          // the defaults). While the picker or the sheet is open its card
          // has the keyboard and every window shortcut stands down; while
          // the treatment menu is open the treatment keys still choose and
          // the menu itself takes Escape.
          Instantiator {
            model: KeyMap.ACTIONS.map(a => a.id)
            delegate: Shortcut {
                sequences: app.bindings[modelData] || []
                enabled: app.opened && !app.overlayOpen && (!treatmentMenu.opened || modelData === "pixels" || modelData === "glyphs" || modelData === "stipple")
                onActivated: app.run(modelData)
            }
          }
          ColumnLayout {
            id: layout
            anchors.fill: parent
            anchors.margins: win.compact ? 12 : 20
            spacing: 10
            RowLayout {
                Layout.fillWidth: true
                RadarMark { ink: app.theme.accent; Layout.rightMargin: 6 }
                LabelText { text: "OMASTORM"; font.bold: true; font.letterSpacing: 2; font.pixelSize: app.theme.baseSize + 2 }
                RowLayout {
                    spacing: 10
                    Layout.leftMargin: 12
                    Repeater {
                        model: [
                            {id: "radar", label: win.compact ? "RADAR" : "RADAR"},
                            {id: "weather", label: win.compact ? "WX" : "WEATHER"},
                            {id: "aviation", label: win.compact ? "AV" : "AVIATION"}
                        ]
                        LabelText {
                            required property var modelData
                            text: modelData.label
                            font.pixelSize: 10
                            font.letterSpacing: 1.4
                            color: app.appMode === modelData.id ? app.theme.accent : app.theme.foreground
                            opacity: app.appMode === modelData.id ? 1 : .5
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: app.setMode(modelData.id) }
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                LabelText { text: app.sourceBadge; color: app.theme.accent; font.letterSpacing: 1.5 }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Qt.alpha(app.theme.foreground, .25) }
            RowLayout {
                Layout.fillWidth: true
                LabelText {
                    text: app.radarMode ? (app.siteId || "—")
                        : app.aviationMode && app.aviation && app.aviation.station ? app.aviation.station.id
                        : (app.store.placeName || "WEATHER").toUpperCase()
                    font.pixelSize: app.theme.baseSize + 7; font.bold: true
                }
                LabelText { text: app.siteName; visible: app.radarMode && !win.compact; opacity: .65 }
                // FOLLOWING or LOCKED after the site name (DESIGN.md, window
                // chrome); nothing when following is off and no lock is set.
                RowLayout {
                    id: siteChip
                    spacing: 5
                    visible: app.radarMode && (app.locked || app.following)
                    readonly property color ink: app.locked ? app.theme.accent : Qt.alpha(app.theme.foreground, .55)
                    Glyph { glyph: app.locked ? "lock" : "follow"; ink: siteChip.ink }
                    LabelText {
                        text: app.locked && app.outsideCoverage ? "LOCKED · OUTSIDE COVERAGE" : app.locked ? "LOCKED" : "FOLLOWING"
                        visible: !win.compact; color: siteChip.ink; font.pixelSize: 10; font.letterSpacing: 1
                    }
                }
                LabelText {
                    visible: !win.compact && !!app.resetTarget && Location.distanceKm(map.centerLat, map.centerLon, app.resetTarget.lat, app.resetTarget.lon) < 2
                    text: !app.resetTarget ? ""
                        : app.resetTarget.source === "weather"
                        ? "LOCATION · " + (app.resetTarget.name || "OMARCHY'S LOCATION").toUpperCase()
                        : "LOCATION · CONFIG.TOML"
                    color: Qt.alpha(app.theme.foreground, .55)
                    font.pixelSize: 10; font.letterSpacing: 1
                    Layout.leftMargin: 10
                }
                Item { Layout.fillWidth: true }
                LabelText { text: !app.scan ? "" : app.scan.productName.toUpperCase() + (app.scan.kind === "field"
                    ? (app.scan.altitudeName ? " / " + app.scan.altitudeName.toUpperCase() : "")
                    : (app.scan.scanTime ? " / " + app.scan.elevationDeg.toFixed(1) + "°" : "")) }
            }
            RowLayout {
                Layout.fillWidth: true
                // The scan time keeps its full width; only the status text shrinks.
                LabelText { text: !app.scan || !app.scan.scanTime ? "—" : Qt.formatDateTime(new Date(app.scan.scanTime), "yyyy-MM-dd  HH:mm t"); opacity: .75; Layout.preferredWidth: implicitWidth }
                // The age of the frame on screen; yellow or red with the condition.
                LabelText {
                    text: "· " + app.ageText
                    visible: app.ageText !== ""
                    color: app.condition === "loading" ? app.theme.foreground : app.conditionColor
                    opacity: app.alert && app.condition !== "loading" ? 1 : .75
                    Layout.preferredWidth: implicitWidth
                }
                // The engine's answer to this window's last command takes the
                // status slot while it stands, ahead of any condition; a
                // config.toml mistake stands there the same way until the
                // file is fixed. The radar underneath stays clear.
                LabelText {
                    text: engine.rejection || app.configError || store.persistError || app.notice || app.sourceDetail
                    color: engine.rejection || app.configError || store.persistError || app.notice ? app.theme.accent : app.conditionColor
                    opacity: engine.rejection || app.configError || store.persistError || app.notice || app.alert ? 1 : .5
                    visible: !win.compact || engine.rejection !== "" || app.configError !== "" || store.persistError !== "" || app.notice !== "" || app.alert
                    horizontalAlignment: Text.AlignRight
                    Layout.fillWidth: true
                }
            }
            LabelText {
                visible: app.aviationMode && app.aviationLine !== "" && !win.compact
                text: app.aviationLine
                wrapMode: Text.Wrap
                opacity: .75
                font.pixelSize: 11
                Layout.fillWidth: true
            }
            LabelText {
                visible: app.aviationMode && !win.compact && !!app.aviation && !!app.aviation.taf && !!app.aviation.taf.raw
                text: "TAF · " + app.aviation.taf.raw
                wrapMode: Text.Wrap
                opacity: .55
                font.pixelSize: 10
                Layout.fillWidth: true
            }
            LabelText {
                visible: app.aviationMode && app.grametLine !== "" && !win.compact
                text: app.grametLine
                wrapMode: Text.Wrap
                opacity: .75
                font.pixelSize: 11
                Layout.fillWidth: true
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: app.run("gramet") }
            }
            LabelText {
                visible: app.weatherMode && app.historyLine !== "" && !win.compact
                text: app.historyLine
                wrapMode: Text.Wrap
                opacity: .75
                font.pixelSize: 11
                Layout.fillWidth: true
            }
            RowLayout {
                visible: !win.compact && !!app.state && !app.radarMode
                spacing: 8
                Repeater {
                    model: app.aviationMode ? [
                        {id: "now", label: "NOW", group: "report", key: "source_now"},
                        {id: "gfs", label: "GFS", group: "forecast", key: "source_gfs"},
                        {id: "ecmwf", label: "ECMWF", group: "forecast", key: "source_ecmwf"}
                    ] : [
                        {id: "now", label: "NOW", group: "report", key: "source_now"},
                        {id: "gfs", label: "GFS", group: "forecast", key: "source_gfs"},
                        {id: "ecmwf", label: "ECMWF", group: "forecast", key: "source_ecmwf"},
                        {id: "wrf", label: "WRF", group: "forecast", key: "source_wrf"},
                        {id: "cdo", label: "CDO", group: "report", key: "source_cdo"},
                        {id: "meteostat", label: "METEOSTAT", group: "report", key: "source_meteostat"}
                    ]
                    LabelText {
                        required property var modelData
                        text: modelData.label
                        font.pixelSize: 10
                        font.letterSpacing: 1
                        color: app.layerSource === modelData.id ? app.theme.accent : app.theme.foreground
                        opacity: app.layerSource === modelData.id ? 1 : .55
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: app.run(modelData.key) }
                    }
                }
            }
            LabelText {
                visible: app.weatherMode && !win.compact && !!app.state && !!app.state.wrf && !!app.state.wrf.estimate
                text: (app.state.wrf.status && app.state.wrf.status !== "idle" ? app.state.wrf.status.replace("_", " ").toUpperCase() + " · " : "WRF · ") + app.state.wrf.estimate.summary
                wrapMode: Text.Wrap
                opacity: .55
                font.pixelSize: 10
                Layout.fillWidth: true
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: app.run("run_wrf") }
            }
            RowLayout {
                visible: !win.compact && !!app.state && !app.radarMode
                spacing: 8
                Repeater {
                    model: app.aviationMode ? [
                        {code: "WIND", label: "WIND", key: "layer_wind"},
                        {code: "PRES", label: "PRES", key: "layer_pressure"},
                        {code: "WATER", label: "WATER", key: "layer_water"}
                    ] : [
                        {code: "TEMP", label: "TEMP", key: "layer_temp"},
                        {code: "PRECIP", label: "PRECIP", key: "layer_precip"},
                        {code: "WIND", label: "WIND", key: "layer_wind"},
                        {code: "PRES", label: "PRES", key: "layer_pressure"},
                        {code: "WATER", label: "WATER", key: "layer_water"}
                    ]
                    LabelText {
                        required property var modelData
                        text: modelData.label
                        font.pixelSize: 10
                        font.letterSpacing: 1
                        color: app.scan && app.scan.product === modelData.code ? app.theme.accent : app.theme.foreground
                        opacity: app.scan && app.scan.product === modelData.code ? 1 : .55
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: app.run(modelData.key) }
                    }
                }
                LabelText {
                    visible: !!app.scan && app.scan.kind === "field"
                    text: "ALT − / +"
                    font.pixelSize: 10
                    opacity: .55
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: mouse => app.stepAltitude(mouse.button === Qt.RightButton ? -1 : 1)
                    }
                }
            }
            Rectangle {
                id: mapFrame
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 100
                color: app.theme.background
                border.color: Qt.alpha(app.theme.foreground, .17)
                clip: true
                RadarMap {
                    id: map
                    anchors.fill: parent
                    scan: app.scan
                    texture: engine.texture
                    azimuthLut: engine.azimuthLut
                    siteId: app.siteId
                    sites: engine.sites
                    tileRoot: "file://" + engine.runtime
                    theme: app.theme
                    treatment: app.treatment
                    weakFloor: app.weakFloor
                    radarOpacity: app.condition === "unavailable" ? .6 : 1
                    labelSize: win.compact ? 10 : 12
                    locked: app.locked
                    // A settled pan hands the centre to the engine, which switches
                    // station while following and unlocked; the camera stays.
                    onViewSettled: (lat, lon) => {
                        if (!app.opened || app.store.needsLocation) return;
                        // A pick or restore already set the store; a settle
                        // still queued from the previous camera must not
                        // write that centre back (radar jumps, map stays).
                        if (app.applyingView
                            && (Math.abs(lat - app.store.centerLat) > 0.05
                                || Math.abs(lon - app.store.centerLon) > 0.05))
                            return;
                        engine.send({type: "view_center", lat: lat, lon: lon});
                        app.store.rememberView(lat, lon, map.span);
                    }
                    onResetRequested: app.resetView()
                    Component.onCompleted: app.applyView()
                    // The map asks for tiles when its camera settles and the
                    // engine answers this window alone, tile by tile.
                    hazards: app.aviationMode && app.aviation && app.aviation.hazards ? app.aviation.hazards : []
                    route: app.aviationMode && app.gramet && app.gramet.coords ? app.gramet.coords : []
                    onTilesNeeded: (z, x0, y0, x1, y1) => engine.send({type: "tiles_needed", z: z, x0: x0, y0: y0, x1: x1, y1: y1})
                }
                Connections { target: engine; function onTileReady(tile) { map.tileReady(tile); } }
                LabelText { anchors.top: parent.top; anchors.left: parent.left; anchors.margins: 12; text: "N ↑"; opacity: .75 }
                // The `?` chip in the map's top-right corner (DESIGN.md, window
                // chrome) opens the keys sheet, as does the key itself.
                Rectangle {
                    id: helpChip
                    anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 10
                    width: helpRow.implicitWidth + 12; height: 22
                    color: Qt.alpha(app.theme.background, .9)
                    opacity: helpArea.containsMouse ? 1 : .7
                    visible: !!app.state
                    RowLayout {
                        id: helpRow
                        anchors.centerIn: parent
                        spacing: 5
                        Glyph { glyph: "keys" }
                        LabelText { text: "?"; font.pixelSize: 10 }
                    }
                    MouseArea { id: helpArea; anchors.fill: parent; hoverEnabled: true; onClicked: app.run("help") }
                }
                // The engine's attribution verbatim while an osm tile is on
                // screen (docs/protocol.md, state.basemap); Natural Earth otherwise.
                LabelText {
                    anchors.bottom: parent.bottom; anchors.right: parent.right; anchors.margins: 12
                    anchors.left: parent.horizontalCenter; horizontalAlignment: Text.AlignRight
                    text: {
                        var mapCredit = map.osmOnScreen && app.state && app.state.basemap ? app.state.basemap.osm.attribution : "NATURAL EARTH · OFFLINE";
                        var air = app.aviationMode && app.aviation && app.aviation.status !== "idle" ? " · " + app.aviation.attribution : "";
                        var hist = app.weatherMode && app.history && app.history.status !== "idle" ? " · " + app.history.attribution : "";
                        return mapCredit + air + hist;
                    }
                    visible: !!app.scan
                    font.pixelSize: 10; opacity: .7
                }
                Rectangle {
                    visible: app.radarMode
                    anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.margins: 10
                    width: scaleLabel.implicitWidth+12; height: win.compact ? 32 : 24; color: app.theme.background
                    LabelText { id: scaleLabel; anchors.centerIn: parent; text: win.compact ? "RINGS 50 km\nDASHED ~460 km" : "RINGS 50 km · DASHED: NOMINAL 460 km"; font.pixelSize: 10; opacity: .7 }
                }
                LabelText { anchors.centerIn: parent; width: parent.width-24; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter; text: map.error || engine.error; visible: text.length > 0 }
            }
            // The timeline row: transport, one tick per frame with the newest
            // at the right, the first and newest times under the ends and the
            // shown frame's time between them while it is not the newest.
            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                visible: !!app.scan && (app.radarMode || (app.weatherMode && app.historySource))
                RowLayout {
                    spacing: 5
                    GlyphButton { glyph: "first"; visible: !win.compact; enabled: app.frames.length > 1; onClicked: app.jump(false) }
                    GlyphButton { glyph: "back"; enabled: app.frames.length > 1; onClicked: app.step(-1) }
                    GlyphButton { glyph: app.playing ? "pause" : "play"; selected: app.playing; visible: app.radarMode; enabled: app.frames.length > 1; onClicked: app.togglePlay() }
                    GlyphButton { glyph: "fwd"; enabled: app.frames.length > 1; onClicked: app.step(1) }
                    GlyphButton { glyph: "last"; visible: !win.compact; enabled: app.frames.length > 1; onClicked: app.jump(true) }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Item {
                        id: strip
                        Layout.fillWidth: true
                        implicitHeight: 14
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Qt.alpha(app.theme.foreground, .17) }
                        Repeater {
                            model: app.slots
                            Rectangle {
                                required property var modelData
                                required property int index
                                readonly property bool current: !modelData.stub && index === app.currentSlot
                                readonly property bool tall: current || modelData.partial
                                x: app.slots.length > 1 ? Math.round(index * (strip.width - width) / (app.slots.length - 1)) : Math.round((strip.width - width) / 2)
                                y: modelData.stub ? strip.height - height - 1 : Math.round((strip.height - height) / 2)
                                width: tall ? 3 : 2
                                height: modelData.stub ? 2 : tall ? 14 : 8
                                color: current ? app.theme.accent : modelData.partial ? "transparent"
                                    : Qt.alpha(app.theme.foreground, modelData.stub ? .35 : index > app.currentSlot ? .18 : .42)
                                border.width: modelData.partial && !current ? 1 : 0
                                border.color: app.theme.accent
                            }
                        }
                        // Dragging scrubs: the nearest frame under the pointer is sought
                        // once per frame change; the engine pauses on a seek.
                        MouseArea {
                            anchors.fill: parent
                            anchors.topMargin: -6
                            anchors.bottomMargin: -18
                            enabled: app.frames.length > 1
                            property string target: ""
                            function scrub(mx) {
                                var n = app.slots.length;
                                if (n < 2) return;
                                var i = Math.round(Math.max(0, Math.min(1, mx / strip.width)) * (n - 1)), lo = i, hi = i;
                                while (lo >= 0 && app.slots[lo].stub) lo--;
                                while (hi < n && app.slots[hi].stub) hi++;
                                var pick = lo < 0 ? hi : hi >= n ? lo : i - lo <= hi - i ? lo : hi;
                                var id = app.slots[pick].id;
                                if (id && id !== target) { target = id; engine.send({type: "seek", id: id}); }
                            }
                            onPressed: mouse => { target = ""; scrub(mouse.x); }
                            onPositionChanged: mouse => { if (pressed) scrub(mouse.x); }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        LabelText { text: app.frames.length ? app.clock(app.frames[0].scanTime) : ""; font.pixelSize: 10; opacity: .65 }
                        Item { Layout.fillWidth: true }
                        LabelText { text: app.scan && !app.newestShown ? app.clock(app.scan.scanTime) : ""; font.pixelSize: 10; opacity: .65 }
                        Item { Layout.fillWidth: true }
                        LabelText {
                            text: app.frames.length > 1 ? app.clock(app.frames[app.frames.length - 1].scanTime) + (app.state.source === "live" ? " now" : "") : ""
                            font.pixelSize: 10; opacity: .65
                        }
                    }
                }
                LabelText {
                    visible: !win.compact
                    text: !app.frames.length ? "" : !app.newestShown && app.frameIndex >= 0 ? (app.frameIndex + 1) + " / " + app.frames.length + " · " + app.clock(app.scan.scanTime, true)
                        : app.frames.length + (app.frames.length === 1 ? " FRAME" : " FRAMES") + (app.frames.length > 1 ? " · " + app.span(app.frames[0].scanTime, app.frames[app.frames.length - 1].scanTime) : "")
                    font.pixelSize: 10; opacity: .65
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Item {
                    Layout.fillWidth: true
                    implicitHeight: legendRow.implicitHeight
                    RowLayout {
                        id: legendRow
                        anchors.fill: parent; spacing: 0
                        Repeater {
                            model: app.scan ? app.scan.palette : []
                            ColumnLayout {
                                required property string modelData
                                required property int index
                                Layout.fillWidth: true; Layout.preferredWidth: 1; spacing: 4
                                Rectangle { Layout.fillWidth: true; height: 6; color: modelData }
                                // Number at the swatch's left edge; the unit sits at the far
                                // right of the last column so the strip spans the full width.
                                RowLayout {
                                    id: labelRow
                                    Layout.fillWidth: true; spacing: 0
                                    LabelText {
                                        id: number
                                        text: app.legendLabel(index)
                                        opacity: app.legendShown[index] ? 1 : 0
                                        font.pixelSize: 10
                                    }
                                    Item { Layout.fillWidth: true }
                                    LabelText {
                                        text: app.scan ? app.scan.units : ""
                                        font.pixelSize: 10
                                        visible: index===app.bands-1 && labelRow.width >= number.implicitWidth + implicitWidth + 8
                                    }
                                }
                            }
                        }
                    }
                    // The hidden part of the scale (DESIGN.md, weak-return floor):
                    // the swatches under the floor sink into the background with a
                    // tick at the floor, so the legend shows what the map leaves out.
                    Rectangle {
                        visible: app.floorFraction > 0
                        height: 6
                        width: Math.round(legendRow.width * app.floorFraction)
                        color: Qt.alpha(app.theme.background, .8)
                        Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: app.theme.foreground; opacity: .7 }
                    }
                }
                LabelText {
                    text: !app.scan ? "" : !app.floorActive ? "Blank: no return / outside · " + app.legendLabel(0) + " measured · X: folded"
                        : "Blank: no return / outside / measured <" + app.weakFloor + " " + app.scan.units + " hidden" + (app.weakKey ? " (" + app.weakKey + " shows)" : "") + " · X: folded"
                    font.pixelSize: 10; Layout.fillWidth: true; opacity: .65
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 5
                // SEARCH and the lock control lead the row (DESIGN.md, search
                // placement); the lock is selected while locked.
                Button {
                    id: searchButton
                    implicitHeight: 30
                    implicitWidth: contentItem.implicitWidth + (win.compact ? 14 : 24)
                    padding: 0
                    focusPolicy: Qt.NoFocus
                    enabled: !!app.state
                    onClicked: picker.show("")
                    contentItem: RowLayout {
                        spacing: 6
                        Item { Layout.fillWidth: true }
                        Glyph { glyph: "search"; fade: searchButton.enabled ? 1 : .35 }
                        LabelText { text: "SEARCH"; visible: !win.compact; opacity: searchButton.enabled ? 1 : .35 }
                        Item { Layout.fillWidth: true }
                    }
                    background: Rectangle {
                        color: searchButton.hovered ? Qt.alpha(app.theme.accent, .18) : "transparent"
                        border.width: 1
                        border.color: Qt.alpha(app.theme.foreground, .22)
                    }
                }
                GlyphButton { glyph: app.locked ? "lock" : "follow"; selected: app.locked; visible: app.radarMode; enabled: !!app.state; onClicked: app.toggleLock() }
                Control { text: win.compact ? "⌂" : "⌂ LOCATION"; onClicked: locationPicker.show("") }
                Item { Layout.fillWidth: true }
                // The treatment chip (DESIGN.md, treatment control): one
                // low-emphasis control naming the treatment; click opens the
                // three in the picker's row style above it, 1 2 3 choose.
                Button {
                    id: treatmentChip
                    implicitHeight: 30
                    implicitWidth: contentItem.implicitWidth + 18
                    padding: 0
                    visible: app.radarMode
                    opacity: treatmentMenu.opened || hovered || activeFocus ? 1 : .7
                    onClicked: treatmentMenu.opened ? treatmentMenu.close() : treatmentMenu.show()
                    contentItem: RowLayout {
                        spacing: 6
                        Item { Layout.fillWidth: true }
                        LabelText { text: app.treatment }
                        Glyph { glyph: "chevron"; implicitWidth: 12 }
                        Item { Layout.fillWidth: true }
                    }
                    background: Rectangle {
                        color: treatmentMenu.opened || treatmentChip.hovered || treatmentChip.activeFocus ? Qt.alpha(app.theme.accent, .18) : "transparent"
                        border.width: 1
                        border.color: treatmentMenu.opened || treatmentChip.activeFocus ? app.theme.accent : Qt.alpha(app.theme.foreground, .22)
                    }
                }
                Rectangle { width: 1; height: 18; color: Qt.alpha(app.theme.foreground, .22); Layout.leftMargin: 4; Layout.rightMargin: 4; visible: !win.compact }
                Control { text: "−"; visible: !win.compact; onClicked: map.zoom(Math.min(map.span,map.maxSpan)*1.25) }
                Control { text: "+"; visible: !win.compact; onClicked: map.zoom(Math.min(map.span,map.maxSpan)/1.25) }
                Control { text: "RESET"; onClicked: app.resetView() }
            }
            LabelText {
                Layout.fillWidth: true
                text: "NOAA / NEXRAD · Natural Earth"
                font.pixelSize: 10; opacity: .7
            }
          }
          // The site picker over everything, its card's top on the map's.
          SitePicker {
            id: picker
            anchors.fill: parent
            sites: engine.sites
            theme: app.theme
            centerLat: map.centerLat
            centerLon: map.centerLon
            homeSite: ""
            compact: win.compact
            cardTop: layout.anchors.margins + mapFrame.y
            onChosen: site => app.choose(site)
          }
          GrametPicker {
            id: grametPicker
            anchors.fill: parent
            theme: app.theme
            compact: win.compact
            cardTop: layout.anchors.margins + mapFrame.y
            onSubmitted: (origin, destination, cruiseKt, flightLevel) => {
                engine.send({type: "set_gramet", origin: origin, destination: destination, cruiseKt: cruiseKt, flightLevel: flightLevel});
            }
          }
          LocationPicker {
            id: locationPicker
            anchors.fill: parent
            theme: app.theme
            engine: engine
            closeOnScrim: !app.store.needsLocation
            centerLat: map.centerLat
            centerLon: map.centerLon
            compact: win.compact
            cardTop: layout.anchors.margins + mapFrame.y
            onChosen: (lat, lon, name) => {
                app.store.setPlace(lat, lon, name);
                app.applyView();
                app.notice = name ? "LOCATION · " + name.toUpperCase() : "LOCATION · " + lat.toFixed(4) + ", " + lon.toFixed(4);
                noticeTimer.restart();
            }
          }
          // The treatment menu over the surface (not a Popup, which the
          // window overlay would draw outside the captured surface): a card
          // above the chip's right edge in the picker's row style, the
          // current and the hovered row in accent, each row's key at the
          // right. A click outside, Escape, a treatment key, or a choice
          // closes it; Up, Down, and Return choose from the keyboard.
          Item {
            id: treatmentMenu
            anchors.fill: parent
            property bool opened: false
            property int cursor: -1
            visible: opened
            focus: opened
            function show() { cursor = -1; opened = true; forceActiveFocus(); }
            function close() { opened = false; }
            Keys.onPressed: event => {
                if (!opened) return;
                event.accepted = true;
                if (event.key === Qt.Key_Escape) close();
                else if (event.key === Qt.Key_Up) cursor = Math.max(0, (cursor < 0 ? KeyMap.TREATMENTS.indexOf(app.treatment) : cursor) - 1);
                else if (event.key === Qt.Key_Down) cursor = Math.min(KeyMap.TREATMENTS.length - 1, (cursor < 0 ? KeyMap.TREATMENTS.indexOf(app.treatment) : cursor) + 1);
                else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && cursor >= 0) app.run(KeyMap.TREATMENTS[cursor].toLowerCase());
                else event.accepted = false;
            }
            MouseArea { anchors.fill: parent; onClicked: treatmentMenu.close() }
            Rectangle {
                id: treatmentCard
                readonly property point anchor: treatmentMenu.opened ? treatmentChip.mapToItem(treatmentMenu, treatmentChip.width, 0) : Qt.point(0, 0)
                x: Math.round(anchor.x - width)
                y: Math.round(anchor.y - height - 6)
                width: 168
                height: treatmentRows.implicitHeight + 12
                color: Qt.alpha(app.theme.background, .95)
                border.width: 1
                border.color: app.theme.foreground
                MouseArea { anchors.fill: parent } // a click on the card stays on the card
                ColumnLayout {
                    id: treatmentRows
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 0
                    Repeater {
                        model: KeyMap.TREATMENTS
                        Rectangle {
                            id: treatmentRow
                            required property string modelData
                            required property int index
                            readonly property bool current: app.treatment === modelData
                            readonly property bool hot: rowArea.containsMouse || treatmentMenu.cursor === index
                            readonly property color ink: current || hot ? app.theme.accent : app.theme.foreground
                            Layout.fillWidth: true
                            implicitHeight: 28
                            color: hot ? Qt.alpha(app.theme.foreground, .08) : "transparent"
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                LabelText { text: treatmentRow.modelData; color: treatmentRow.ink; Layout.fillWidth: true }
                                LabelText {
                                    text: (app.bindings[treatmentRow.modelData.toLowerCase()] || []).map(KeyMap.pretty).join(" ")
                                    color: treatmentRow.ink; font.pixelSize: 10; opacity: .6
                                }
                            }
                            MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; onClicked: app.run(treatmentRow.modelData.toLowerCase()) }
                        }
                    }
                }
            }
          }
          // The `?` sheet over everything, below the map's top edge.
          KeysSheet {
            id: sheet
            anchors.fill: parent
            theme: app.theme
            bindings: app.bindings
            mode: app.appMode
            compact: win.compact
            cardTop: layout.anchors.margins + mapFrame.y
          }
        }
        // Opt-in capture uses the actual QML scene at a fixed size, without a compositor.
        Timer {
            interval: Number(Quickshell.env("OMASTORM_CAPTURE_DELAY")) || 2500; running: !app.session && !!Quickshell.env("OMASTORM_CAPTURE"); repeat: false
            onTriggered: surface.grabToImage(result => { result.saveToFile(Quickshell.env("OMASTORM_CAPTURE")); Qt.quit(); })
        }
    }
}
