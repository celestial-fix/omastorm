.pragma library
// Window chrome: modes, sources, time steps, and map variables.
// Pure functions so the sheet and the window share one list.

var MODES = [
    { id: "weather", label: "WEATHER" },
    { id: "aviation", label: "AVIATION" },
    { id: "radar", label: "RADAR" }
];

var SOURCES = [
    { id: "nexrad", label: "NEXRAD", key: "source_nexrad" },
    { id: "now", label: "NOW", key: "source_now" },
    { id: "gfs", label: "GFS", key: "source_gfs" },
    { id: "ecmwf", label: "ECMWF", key: "source_ecmwf" },
    { id: "wrf", label: "WRF", key: "source_wrf" },
    { id: "dmc", label: "DMC", key: "source_dmc" },
    { id: "dmc_wrf_gfs", label: "WRF·GFS", key: "source_dmc_wrf_gfs" },
    { id: "dmc_wrf_ecmwf", label: "WRF·IFS", key: "source_dmc_wrf_ecmwf" },
    { id: "cdo", label: "CDO", key: "source_cdo" },
    { id: "meteostat", label: "METEOSTAT", key: "source_meteostat" }
];

var PRODUCTS = [
    { code: "REF", label: "RADAR", key: "layer_radar" },
    { code: "WIND", label: "WIND", key: "layer_wind" },
    { code: "PRES", label: "PRES", key: "layer_pressure" },
    { code: "WATER", label: "WATER", key: "layer_water" },
    { code: "TEMP", label: "TEMP", key: "layer_temp" },
    { code: "PRECIP", label: "PRECIP", key: "layer_precip" }
];

function isWrfProducer(source) {
    return source === "wrf" || source === "dmc_wrf_gfs" || source === "dmc_wrf_ecmwf";
}

function isFieldProduct(code) {
    return code === "WIND" || code === "PRES" || code === "WATER" || code === "TEMP" || code === "PRECIP";
}

function anyField(selected) {
    return Array.isArray(selected) && selected.some(isFieldProduct);
}

// Hours the time chips may step, given the selected source.
function timeSteps(source) {
    if (source === "nexrad" || source === "dmc") return [];
    if (source === "cdo") return [24];
    if (source === "now") return [1, 3];
    return [1, 3, 6, 12];
}

function stepLabel(hours) {
    if (hours >= 24 && hours % 24 === 0) return (hours / 24) + "d";
    return hours + "h";
}

// RADAR is exclusive with field products. Field products may be combined;
// the engine still draws one raster (the last toggled on).
function toggleProducts(selected, code) {
    var list = Array.isArray(selected) ? selected.slice() : [];
    if (code === "REF") return { selected: ["REF"], active: "REF" };
    list = list.filter(function (p) { return p !== "REF"; });
    var i = list.indexOf(code);
    var off = i >= 0;
    if (off) list.splice(i, 1);
    else list.push(code);
    if (!list.length) list = [code];
    return { selected: list, active: off && list.indexOf(code) < 0 ? list[list.length - 1] : code };
}
