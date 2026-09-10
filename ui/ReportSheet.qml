import QtQuick
import QtQuick.Layouts

// The export sheet: the current view as a Lambert conformal conic chart
// with the layers this build can draw. `e` or Escape, or a click on the
// scrim, closes it; Return writes the PNG.
Item {
    id: sheet
    property var theme
    property var engine
    property var box: ({ west: 0, south: 0, east: 0, north: 0 })
    property bool compact: false
    property real cardTop: 20
    property bool open: false
    property bool storms: true
    property bool geography: true
    property bool rings: true
    property string status: ""
    component Word: Text {
        color: sheet.theme.foreground
        font.family: sheet.theme.font
        font.pixelSize: 12
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
    visible: open
    focus: open
    function show() {
        status = "";
        open = true;
        forceActiveFocus();
    }
    function close() { open = false; }
    function layers() {
        var list = [];
        if (storms) list.push("ref");
        if (geography) list.push("basemap");
        if (rings) list.push("rings");
        return list;
    }
    function exportChart() {
        var list = layers();
        if (!list.length) {
            status = "Pick at least one layer.";
            return;
        }
        status = "EXPORTING…";
        engine.send({
            type: "export_report",
            west: box.west,
            south: box.south,
            east: box.east,
            north: box.north,
            layers: list,
            width: 1280
        });
    }
    Connections {
        target: sheet.engine
        function onReportReady(message) {
            sheet.status = "SAVED · ~/.local/share/omastorm/" + message.path;
        }
    }
    Keys.onPressed: event => {
        if (!open) return;
        if (event.key === Qt.Key_Escape || event.text === "e" || event.text === "E") { close(); event.accepted = true; }
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { exportChart(); event.accepted = true; }
    }
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(sheet.theme.background, .5)
        MouseArea { anchors.fill: parent; onClicked: sheet.close() }
    }
    Rectangle {
        id: card
        width: Math.min(520, sheet.width - 40)
        x: Math.round((sheet.width - width) / 2)
        y: Math.round(Math.max(20, Math.min(sheet.cardTop + 20, sheet.height - height - 20)))
        height: column.implicitHeight + 36
        color: Qt.alpha(sheet.theme.background, .97)
        border.width: 1
        border.color: sheet.theme.foreground
        MouseArea { anchors.fill: parent }
        ColumnLayout {
            id: column
            anchors.fill: parent
            anchors.margins: 18
            spacing: 10
            RowLayout {
                Layout.fillWidth: true
                Word { text: "EXPORT"; font.bold: true; font.pixelSize: 14; font.letterSpacing: 2 }
                Item { Layout.fillWidth: true }
                Word { text: "Lambert conformal conic"; font.pixelSize: 10; opacity: .55 }
            }
            Word {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                font.pixelSize: 11
                opacity: .75
                lineHeight: 1.35
                text: "A chart of the view on screen. This build draws the measured lowest-cut reflectivity, Natural Earth geography, and range rings — not pressure, winds, or other altitudes."
            }
            Word {
                Layout.fillWidth: true
                font.pixelSize: 11
                text: box.south.toFixed(2) + "°N " + box.west.toFixed(2) + "°  →  " + box.north.toFixed(2) + "°N " + box.east.toFixed(2) + "°"
            }
            Repeater {
                model: [
                    { id: "storms", label: "Storms · reflectivity" },
                    { id: "geography", label: "Geography · Natural Earth" },
                    { id: "rings", label: "Range rings · 50 / 100 / 150 km" }
                ]
                RowLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 8
                    Rectangle {
                        width: 14; height: 14
                        color: sheet[parent.modelData.id] ? sheet.theme.accent : "transparent"
                        border.width: 1
                        border.color: sheet.theme.foreground
                        MouseArea { anchors.fill: parent; onClicked: sheet[parent.modelData.id] = !sheet[parent.modelData.id] }
                    }
                    Word {
                        text: parent.modelData.label
                        Layout.fillWidth: true
                        MouseArea { anchors.fill: parent; onClicked: sheet[parent.modelData.id] = !sheet[parent.modelData.id] }
                    }
                }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Qt.alpha(sheet.theme.foreground, .17) }
            RowLayout {
                Layout.fillWidth: true
                Word {
                    text: sheet.engine && sheet.engine.rejection ? sheet.engine.rejection : sheet.status
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    font.pixelSize: 10
                    opacity: .7
                }
                Rectangle {
                    width: exportLabel.implicitWidth + 16
                    height: 26
                    color: "transparent"
                    border.width: 1
                    border.color: sheet.theme.foreground
                    Word { id: exportLabel; anchors.centerIn: parent; text: "EXPORT"; font.pixelSize: 11; font.letterSpacing: 1 }
                    MouseArea { anchors.fill: parent; onClicked: sheet.exportChart() }
                }
            }
        }
    }
}
