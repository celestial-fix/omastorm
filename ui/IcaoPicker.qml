import QtQuick
import QtQuick.Layouts

// Aviation ICAO: one four-letter id, or empty to follow the map centre.
Item {
    id: picker
    property var theme
    property bool compact: false
    property real cardTop: 20
    property bool open: false
    property alias query: field.text
    signal submitted(string icao)
    visible: open
    function show(text) {
        field.text = text || "";
        hint.text = "SCEL   or empty to follow the map";
        open = true;
        Qt.callLater(() => { field.cursorPosition = field.length; field.forceActiveFocus(); });
    }
    function close() {
        open = false;
        field.focus = false;
    }
    function parse(text) {
        var value = String(text || "").trim().toUpperCase();
        if (!value) return { icao: "" };
        if (!/^[A-Z]{4}$/.test(value)) return { error: "Need a four-letter ICAO id, or empty to follow the map." };
        return { icao: value };
    }
    function accept() {
        var parsed = parse(field.text);
        if (parsed.error) { hint.text = parsed.error; return; }
        close();
        submitted(parsed.icao);
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
            Text { text: "ICAO"; color: picker.theme.foreground; font.pixelSize: 12; font.letterSpacing: 1 }
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
