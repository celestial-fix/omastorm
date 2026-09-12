import QtQuick
import Quickshell

ShellRoot {
    FloatingWindow {
        implicitWidth: 600; implicitHeight: 420; color: "#101820"
        RadarMap {
            id: map
            anchors.fill: parent
            theme: ({font: "monospace", foreground: "#eeeeee", accent: "#5599ee", background: "#101820", red: "#cc4444"})
            scan: ({site: {lat: -33.45, lon: -70.67}, palette: [], rays: 1, gates: 1, firstGateM: 0, gateSpacingM: 250, elevationDeg: 0})
            siteId: "SCEL"
        }
    }
    function check(ok, message) { if (!ok) throw new Error(message); }
    readonly property var santiago: ({
        kind: "sigmet",
        hazard: "TURB",
        raw: "SIGMET 3 VALID 101200/101600 SCFA-\nSANTIAGO FIR SEV TURB FCST",
        coords: [
            {lat: -32.5, lon: -71.5},
            {lat: -32.5, lon: -69.5},
            {lat: -34.5, lon: -69.5},
            {lat: -34.5, lon: -71.5}
        ]
    })
    readonly property var airmet: ({
        kind: "airmet",
        hazard: "IFR",
        raw: "AIRMET IFR...",
        coords: [
            {lat: -30.0, lon: -74.0},
            {lat: -30.0, lon: -67.0},
            {lat: -37.0, lon: -67.0},
            {lat: -37.0, lon: -74.0}
        ]
    })
    property int stage: 0
    Timer {
        interval: 250; repeat: true; running: true
        onTriggered: {
            try {
                if (stage === 0) {
                    map.lookAt(-33.45, -70.67);
                    map.span = 400;
                    map.hazards = [airmet, santiago];
                    map.airports = [{id: "SCVM", lat: -32.95, lon: -71.48}];
                    map.aviationIcao = "";
                }
                if (stage === 1) {
                    var hit = map.hazardAt(map.width / 2, map.height / 2);
                    check(hit && hit.kind === "sigmet" && hit.hazard === "TURB",
                          "Centre of Santiago did not pick the SIGMET: " + (hit ? hit.kind : "null"));
                    check(map.hazardTitle(hit) === "SIGMET · TURB", "Tooltip title drifted: " + map.hazardTitle(hit));
                    var miss = map.hazardAt(8, 8);
                    check(!miss, "Corner of the view was inside a hazard");
                    map.hoverAt(map.width / 2, map.height / 2);
                    check(map.hoverHazard && map.hoverHazard.kind === "sigmet", "hoverAt did not keep the SIGMET");
                    check(!map.hoverAirport, "View centre should not sit on the ICAO marker");
                    check(map.hoverTip.visible, "Themed tooltip stayed hidden over a SIGMET");
                }
                if (stage === 2) {
                    var marker = map.airportAt(map.sx(map.mercatorX(-32.95)), map.sy(map.mercatorY(-71.48)));
                    check(marker && marker.id === "SCVM", "Viña ICAO marker was not hit: " + (marker ? marker.id : "null"));
                    map.hoverAt(map.sx(map.mercatorX(-32.95)), map.sy(map.mercatorY(-71.48)));
                    check(map.hoverAirport && map.hoverAirport.id === "SCVM", "hovering the ICAO marker did not pick SCVM");
                    check(!map.hoverHazard, "ICAO hover should win over a polygon under the marker");
                    check(map.airportTitle(map.hoverAirport) === "SCVM", "Airport tooltip title drifted: " + map.airportTitle(map.hoverAirport));
                    check(map.airportHint(map.hoverAirport) === "Click to pin METAR / TAF",
                          "Airport click hint drifted: " + map.airportHint(map.hoverAirport));
                    check(!map.airportPinned(map.hoverAirport), "Unpinned SCVM was treated as pinned");
                    check(map.hoverTip.visible, "Themed tooltip stayed hidden over an ICAO marker");
                    var border = String(map.hoverTip.border.color);
                    check(border.indexOf("eeeeee") >= 0 || border.indexOf("EEEEEE") >= 0 || border === String(map.theme.foreground),
                          "Tooltip border is not theme foreground: " + border);
                    map.grabToImage(r => {
                        check(r.saveToFile(Quickshell.env("OMASTORM_REVIEW") + "/hazard-tooltip.png"), "Capture failed");
                        console.log("MAP_HAZARDS_PASSED");
                        Qt.quit();
                    });
                }
                stage++;
            } catch (e) { console.error(e); Qt.quit(); }
        }
    }
    Timer { interval: 8000; running: true; onTriggered: Qt.quit() }
}
