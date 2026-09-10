import QtQuick
import QtQuick.Layouts

// Route GRAMET: origin ICAO, destination ICAO, cruise TAS, optional FL.
Item {
    id: picker
    property var theme
    property bool compact: false
    property real cardTop: 20
    property bool open: false
    property alias query: field.text
    signal submitted(string origin, string destination, int cruiseKt, int flightLevel)
    visible: open
    function show(text) {
        field.text = text || field.text || "";
        hint.text = "SCEL SCFA 420   or   SCEL SCFA 420 350";
        open = true;
        Qt.callLater(() => { field.cursorPosition = field.length; field.forceActiveFocus(); });
    }
    function close() {
        open = false;
        field.focus = false;
    }
    function parse(text) {
        var parts = String(text || "").trim().toUpperCase().split(/\s+/).filter(s => s !== "");
        if (parts.length < 3) return { error: "Need origin, destination, and cruise TAS." };
        var origin = parts[0], dest = parts[1], tas = Number(parts[2]), fl = parts.length > 3 ? Number(parts[3]) : 350;
        if (!/^[A-Z]{4}$/.test(origin) || !/^[A-Z]{4}$/.test(dest)) return { error: "Origin and destination must be ICAO ids." };
        if (origin === dest) return { error: "Origin and destination must differ." };
        if (!isFinite(tas) || tas < 80 || tas > 550) return { error: "Cruise TAS must be 80–550 kt." };
        if (!isFinite(fl) || fl < 50 || fl > 450) return { error: "Flight level must be 50–450." };
        return { origin: origin, destination: dest, cruiseKt: Math.round(tas), flightLevel: Math.round(fl) };
    }
    function accept() {
        var parsed = parse(field.text);
        if (parsed.error) { hint.text = parsed.error; return; }
        close();
        submitted(parsed.origin, parsed.destination, parsed.cruiseKt, parsed.flightLevel);
    }
    Keys.onPressed: event => {
        if (!open) return;
        if (event.key === Qt.Key_Escape) { close(); event.accepted = true; }
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { accept(); event.accepted = true; }
    }
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(picker.theme.background, 0.35)
        MouseArea { anchors.fill: parent; onClicked: picker.close() }
    }
    Rectangle {
        id: card
        anchors.horizontalCenter: parent.horizontalCenter
        y: picker.cardTop
        width: Math.min(parent.width - 24, 420)
        height: col.implicitHeight + 20
        color: Qt.alpha(picker.theme.background, 0.96)
        border.color: picker.theme.foreground
        border.width: 1
        MouseArea { anchors.fill: parent }
        ColumnLayout {
            id: col
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 10
            spacing: 8
            Text { text: "GRAMET"; color: picker.theme.foreground; font.pixelSize: 12; font.letterSpacing: 1 }
            TextInput {
                id: field
                Layout.fillWidth: true
                color: picker.theme.foreground
                font.pixelSize: 14
                selectionColor: picker.theme.accent
                cursorVisible: activeFocus
            }
            Text {
                id: hint
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: picker.theme.foreground
                opacity: 0.6
                font.pixelSize: 10
            }
        }
    }
}
