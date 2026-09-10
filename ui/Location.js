.pragma library
// Map centre, remembered view, and radar lock (DESIGN.md, location and
// remembered state). Pure functions so a check can drive them without a
// window. Config.toml holds deliberate preferences; state.json holds the
// last camera and the UI radar lock.

var DEFAULT_SPAN = 210;
var MIN_SPAN = 25;

function validLat(value) {
    return typeof value === "number" && isFinite(value) && Math.abs(value) <= 90;
}
function validLon(value) {
    return typeof value === "number" && isFinite(value) && Math.abs(value) <= 180;
}
function validPair(lat, lon) { return validLat(lat) && validLon(lon); }

function asFiniteNumber(value) {
    if (typeof value === "number") return isFinite(value) ? value : NaN;
    if (typeof value === "string" && value.trim()) {
        var n = Number(value);
        return isFinite(n) ? n : NaN;
    }
    return NaN;
}

function firstDefined() {
    for (var i = 0; i < arguments.length; i++) {
        if (arguments[i] !== undefined && arguments[i] !== null && arguments[i] !== "")
            return arguments[i];
    }
    return undefined;
}

function firstElement(value) {
    if (Array.isArray(value)) return value.length ? value[0] : null;
    return value && typeof value === "object" ? value : null;
}

function asName(value) {
    if (typeof value === "string") return value.trim();
    if (Array.isArray(value) && value[0])
        return asName(value[0].value !== undefined ? value[0].value : value[0].name);
    if (value && typeof value === "object")
        return asName(value.value !== undefined ? value.value : value.name);
    return "";
}

// Coordinates from one object: Omarchy / Visual Crossing / WeeWX
// (latitude, longitude), WeatherAPI / OpenWeatherMap One Call / Tomorrow.io
// (lat, lon), OpenWeatherMap current (lat + lon/lng), or a GeoJSON Point.
function coordsFrom(obj) {
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return null;
    var lat = asFiniteNumber(firstDefined(obj.latitude, obj.lat));
    var lon = asFiniteNumber(firstDefined(obj.longitude, obj.lon, obj.lng, obj.long));
    if (validPair(lat, lon)) return { lat: lat, lon: lon };
    var xy = obj.coordinates;
    if ((obj.type === "Point" || obj.type === "point") && Array.isArray(xy) && xy.length >= 2) {
        lon = asFiniteNumber(xy[0]);
        lat = asFiniteNumber(xy[1]);
        if (validPair(lat, lon)) return { lat: lat, lon: lon };
    }
    return null;
}

function nameFrom(obj) {
    if (!obj || typeof obj !== "object") return "";
    var keys = ["name", "resolvedAddress", "formatted_address", "address", "city", "station", "location"];
    for (var i = 0; i < keys.length; i++) {
        var value = obj[keys[i]];
        if (keys[i] === "location" && value && typeof value === "object") continue;
        var name = asName(value);
        if (name) return name;
    }
    var area = asName(obj.areaName);
    return area || "";
}

// weather.json, or a weather-API / WeeWX payload saved as that file.
// A name without coordinates is not a place (DESIGN.md: no fetch at launch).
function parseWeatherLocation(raw) {
    if (raw === undefined || raw === null || raw === "") return null;
    try {
        var json = typeof raw === "string" ? JSON.parse(raw) : raw;
        return weatherPlace(json);
    } catch (e) { return null; }
}

function weatherPlace(json) {
    if (!json || typeof json !== "object") return null;
    if (Array.isArray(json)) {
        for (var i = 0; i < json.length; i++) {
            var row = weatherPlace(json[i]);
            if (row) return row;
        }
        return null;
    }
    var holders = [
        json,
        json.location,
        json.coord,
        json.coords,
        json.station,
        json.stationInfo,
        firstElement(json.nearest_area),
        json.geometry,
        firstElement(json.features)
    ];
    var coords = null, holder = null;
    for (var h = 0; h < holders.length; h++) {
        var obj = holders[h];
        if (!obj || typeof obj !== "object") continue;
        var found = coordsFrom(obj) || coordsFrom(obj.geometry) || coordsFrom(obj.properties);
        if (found) { coords = found; holder = obj; break; }
    }
    if (!coords) return null;
    var name = nameFrom(json) || nameFrom(json.location) || nameFrom(json.station)
        || nameFrom(json.stationInfo) || nameFrom(firstElement(json.nearest_area))
        || nameFrom(holder) || nameFrom(holder && holder.properties) || "";
    return { name: name, lat: coords.lat, lon: coords.lon };
}

var WEATHER_SOURCES = [
    { id: "weewx", label: "WeeWX", needs: "url" },
    { id: "weatherapi", label: "WeatherAPI", needs: "key" },
    { id: "openweathermap", label: "OpenWeatherMap", needs: "key" },
    { id: "tomorrow", label: "Tomorrow.io", needs: "key" },
    { id: "visualcrossing", label: "Visual Crossing", needs: "key" }
];

function normalizeWeatherSource(value) {
    var s = String(value || "").trim().toLowerCase().replace(/[\s_.-]+/g, "");
    if (!s || s === "off" || s === "none") return "";
    if (s === "owm" || s === "openweather") return "openweathermap";
    if (s === "tomorrowio") return "tomorrow";
    if (s === "weatherapi") return "weatherapi";
    if (s === "visualcrossing") return "visualcrossing";
    if (s === "weewx" || s === "openweathermap" || s === "tomorrow") return s;
    return "";
}

function takeWeatherKeys(into, obj) {
    if (!obj) return;
    if (obj["weather.source"] !== undefined) into.source = obj["weather.source"];
    if (obj["weather.api_key"] !== undefined) into.api_key = obj["weather.api_key"];
    if (obj["weather.url"] !== undefined) into.url = obj["weather.url"];
    if (obj.source !== undefined) into.source = obj.source;
    if (obj.api_key !== undefined) into.api_key = obj.api_key;
    if (obj.url !== undefined) into.url = obj.url;
}

// Deliberate weather source: [weather] in config.toml, then weather.toml.
// An empty source is off. A bad source or a missing key/URL is reported
// and does not fetch.
function weatherSettings(configValues, fileValues) {
    var raw = {};
    takeWeatherKeys(raw, configValues);
    takeWeatherKeys(raw, fileValues);
    var source = normalizeWeatherSource(raw.source);
    var apiKey = typeof raw.api_key === "string" ? raw.api_key.trim() : (raw.api_key ? String(raw.api_key) : "");
    var url = typeof raw.url === "string" ? raw.url.trim() : "";
    var errors = [];
    if (raw.source !== undefined && String(raw.source).trim() && !source)
        errors.push("weather.source is not weewx, weatherapi, openweathermap, tomorrow, or visualcrossing");
    else if (source === "weewx" && !url)
        errors.push("weather.url is required for WeeWX");
    else if (source && source !== "weewx" && !apiKey)
        errors.push("weather.api_key is required for " + source);
    if (url && !/^https?:\/\//i.test(url))
        errors.push("weather.url must be an http or https URL");
    if (typeof raw.api_key === "string" && /["\n]/.test(raw.api_key))
        errors.push("weather.api_key must not contain quotes or newlines");
    return {
        source: errors.length ? "" : source,
        apiKey: apiKey,
        url: url,
        errors: errors
    };
}

function weatherToml(source, apiKey, url) {
    var quote = function (s) { return "\"" + String(s).replace(/["\n]/g, "") + "\""; };
    var lines = ["# Current-conditions source. The key never enters state.json."];
    if (source) lines.push("source = " + quote(source));
    if (apiKey) lines.push("api_key = " + quote(apiKey));
    if (url) lines.push("url = " + quote(url));
    return lines.join("\n") + "\n";
}

function formatWeather(obs) {
    if (!obs) return "";
    var bits = [];
    if (obs.status && obs.status !== "ok") bits.push(String(obs.status).toUpperCase());
    if (typeof obs.temperatureC === "number" && isFinite(obs.temperatureC)) {
        bits.push(Math.round(obs.temperatureC) + "°C");
        bits.push(Math.round(obs.temperatureC * 9 / 5 + 32) + "°F");
    }
    if (obs.condition) bits.push(String(obs.condition).toUpperCase());
    if (obs.sourceName) bits.push(String(obs.sourceName).toUpperCase());
    if (obs.observedAt) {
        var when = new Date(obs.observedAt);
        if (!isNaN(when.getTime())) bits.push(when.toISOString().slice(11, 16) + "Z");
    }
    return bits.join("  ·  ");
}

function clampSpan(value) {
    var n = typeof value === "number" && isFinite(value) ? value : DEFAULT_SPAN;
    return Math.max(MIN_SPAN, n);
}

function parseLatitude(text) {
    var t = String(text).trim();
    if (!t) return { empty: true };
    var n = Number(t);
    if (!isFinite(n)) return { error: "latitude is not a number" };
    if (Math.abs(n) > 90) return { error: "latitude must be in [-90, 90]" };
    return { value: n };
}
function parseLongitude(text) {
    var t = String(text).trim();
    if (!t) return { empty: true };
    var n = Number(t);
    if (!isFinite(n)) return { error: "longitude is not a number" };
    if (Math.abs(n) > 180) return { error: "longitude must be in [-180, 180]" };
    return { value: n };
}
function parseCoordFields(latText, lonText) {
    var lat = parseLatitude(latText), lon = parseLongitude(lonText);
    if (lat.empty && lon.empty) return { empty: true };
    if (lat.empty || lon.empty) return { error: "latitude and longitude must both be set" };
    if (lat.error) return { error: lat.error };
    if (lon.error) return { error: lon.error };
    return { lat: lat.value, lon: lon.value };
}

function distanceKm(lat1, lon1, lat2, lon2) {
    var r = Math.PI / 180, dp = (lat2 - lat1) * r, dl = (lon2 - lon1) * r;
    var h = Math.sin(dp / 2) ** 2 + Math.cos(lat1 * r) * Math.cos(lat2 * r) * Math.sin(dl / 2) ** 2;
    return 2 * 6371 * Math.asin(Math.sqrt(Math.max(0, Math.min(1, h))));
}

function nearestSite(sites, lat, lon) {
    var best = null, bestKm = Infinity;
    if (!validPair(lat, lon) || !sites) return null;
    for (var s of sites) {
        var km = distanceKm(lat, lon, s.lat, s.lon);
        if (km < bestKm) { bestKm = km; best = s; }
    }
    return best;
}

function envView(text) {
    if (!text) return null;
    var parts = String(text).split(",");
    if (parts.length !== 3) return null;
    var lat = Number(parts[0]), lon = Number(parts[1]), span = Number(parts[2]);
    return validPair(lat, lon) && isFinite(span) && span > 0 ? { lat: lat, lon: lon, span: span } : null;
}

// Explicit centre from config.toml: both keys, both in range, or null.
function configCenter(values) {
    if (!values || values.center_lat === undefined && values.center_lon === undefined) return null;
    if (values.center_lat === undefined || values.center_lon === undefined) return null;
    return validPair(values.center_lat, values.center_lon)
        ? { lat: values.center_lat, lon: values.center_lon } : null;
}

function configLock(values) {
    if (!values || typeof values.locked_radar !== "string") return "";
    return values.locked_radar.trim().toUpperCase();
}

function configErrors(values) {
    var errors = [];
    if (!values) return errors;
    var hasLat = values.center_lat !== undefined, hasLon = values.center_lon !== undefined;
    if (hasLat !== hasLon)
        errors.push("center_lat and center_lon must both be set");
    else if (hasLat && !validPair(values.center_lat, values.center_lon))
        errors.push("center_lat/center_lon must be latitudes in [-90, 90] and longitudes in [-180, 180]");
    if (values.locked_radar !== undefined) {
        if (typeof values.locked_radar !== "string")
            errors.push("locked_radar must be a quoted station id");
        else if (!values.locked_radar.trim())
            errors.push("locked_radar must be a quoted station id");
    }
    if (values.home_site !== undefined)
        errors.push("home_site is unused; location is a place (center_lat/center_lon or the location picker)");
    if (values.follow !== undefined)
        errors.push("follow is unused; the map follows the nearest radar unless locked");
    return errors;
}

// Remembered view from state.json. Invalid fields are dropped, not fatal.
function parseState(raw) {
    var empty = { lat: undefined, lon: undefined, span: undefined, lock: "", name: "" };
    if (raw === undefined || raw === null || raw === "") return empty;
    try {
        var json = typeof raw === "string" ? JSON.parse(raw) : raw;
        if (!json || typeof json !== "object") return empty;
        var lat = json.lat, lon = json.lon, span = json.span;
        return {
            lat: validLat(lat) ? lat : undefined,
            lon: validLon(lon) ? lon : undefined,
            span: typeof span === "number" && isFinite(span) && span > 0 ? span : undefined,
            lock: typeof json.lock === "string" ? json.lock.trim().toUpperCase() : "",
            name: typeof json.name === "string" ? json.name : ""
        };
    } catch (e) { return empty; }
}

function stateObject(viewLat, viewLon, span, lock, name) {
    var o = {};
    if (validPair(viewLat, viewLon)) { o.lat = viewLat; o.lon = viewLon; }
    if (typeof span === "number" && isFinite(span) && span > 0) o.span = span;
    if (lock) o.lock = lock;
    if (name) o.name = name;
    return o;
}

// Map centre at launch: explicit config, remembered view, Omarchy weather,
// else nothing (the location picker). Captures may pass env as the first
// argument to outrank the rest for that process.
function resolvePlace(explicit, remembered, weather, env) {
    if (env && validPair(env.lat, env.lon))
        return { lat: env.lat, lon: env.lon, name: "", source: "view", span: env.span };
    if (explicit && validPair(explicit.lat, explicit.lon))
        return { lat: explicit.lat, lon: explicit.lon, name: "", source: "config", span: remembered && remembered.span };
    if (remembered && validPair(remembered.lat, remembered.lon))
        return { lat: remembered.lat, lon: remembered.lon, name: remembered.name || "", source: "state", span: remembered.span };
    if (weather && validPair(weather.lat, weather.lon))
        return { lat: weather.lat, lon: weather.lon, name: weather.name || "", source: "weather", span: remembered && remembered.span };
    return null;
}

// RESET target: configured centre, else weather, else none (keep the camera,
// restore the default span).
function resolveReset(explicit, weather) {
    if (explicit && validPair(explicit.lat, explicit.lon))
        return { lat: explicit.lat, lon: explicit.lon, name: "", source: "config" };
    if (weather && validPair(weather.lat, weather.lon))
        return { lat: weather.lat, lon: weather.lon, name: weather.name || "", source: "weather" };
    return null;
}
