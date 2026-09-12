import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// Credentials for sources that need them (DESIGN.md). Not a weather
// location control. Writes ~/.config/omastorm/sources.json; the engine
// rereads it on the next MeteoChile fetch. Env vars still win.
Item {
    id: sheet
    property var theme
    property bool compact: false
    property real cardTop: 20
    property bool open: false
    property string userText: ""
    property string tokenText: ""
    property string notice: ""
    readonly property string path: Quickshell.env("OMASTORM_SOURCES")
        || (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/omastorm/sources.json"
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
        notice = "";
        reader.reload();
        open = true;
        Qt.callLater(() => { userField.forceActiveFocus(); });
    }
    function close() { open = false; }
    function applyText(text) {
        try {
            var json = text ? JSON.parse(text) : {};
            userText = json.meteochileUser || "";
            tokenText = json.meteochileToken || "";
        } catch (e) {
            userText = "";
            tokenText = "";
        }
    }
    function save() {
        var text = JSON.stringify({ meteochileUser: userText.trim(), meteochileToken: tokenText.trim() });
        var slash = path.lastIndexOf("/");
        var dir = slash >= 0 ? path.slice(0, slash) : ".";
        writer.command = ["sh", "-c",
            "mkdir -p -- \"$1\" && printf '%s\\n' \"$3\" > \"$2\" && mv -f -- \"$2\" \"$4\"",
            "omastorm-sources", dir, path + ".tmp", text, path];
        writer.running = true;
    }
    FileView {
        id: reader
        path: sheet.path
        watchChanges: true
        printErrors: false
        onLoaded: sheet.applyText(text())
        onLoadFailed: sheet.applyText("")
    }
    Process {
        id: writer
        command: ["true"]
        onExited: function (exitCode) {
            sheet.notice = exitCode === 0 ? "SAVED · NEXT DMC FETCH USES THESE" : "COULD NOT WRITE SOURCES.JSON";
        }
    }
    Keys.onPressed: event => {
        if (!open) return;
        if (event.key === Qt.Key_Escape) { close(); event.accepted = true; }
    }
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(sheet.theme.background, .5)
        MouseArea { anchors.fill: parent; onClicked: sheet.close() }
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: sheet.cardTop
        anchors.margins: 16
        height: Math.min(parent.height - sheet.cardTop - 24, card.implicitHeight + 24)
        color: Qt.alpha(sheet.theme.background, .96)
        border.width: 1
        border.color: sheet.theme.foreground
        MouseArea { anchors.fill: parent }
        ColumnLayout {
            id: card
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            Word { text: "API SOURCES"; font.bold: true; font.letterSpacing: 1.4; font.pixelSize: 13 }
            Word {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: .7
                text: "NEXRAD, Open-Meteo (NOW / GFS / ECMWF), AWC briefings, CDO, and Meteostat need no key. WRF-DMC from MeteoChile needs a Servicios Climáticos user and token."
            }
            Word { text: "METEOCHILE USER"; font.pixelSize: 10; opacity: .55 }
            TextInput {
                id: userField
                Layout.fillWidth: true
                color: sheet.theme.foreground
                font.family: sheet.theme.font
                font.pixelSize: 13
                text: sheet.userText
                onTextChanged: sheet.userText = text
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Escape) { sheet.close(); event.accepted = true; }
                    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { tokenField.forceActiveFocus(); event.accepted = true; }
                }
            }
            Word { text: "METEOCHILE TOKEN"; font.pixelSize: 10; opacity: .55 }
            TextInput {
                id: tokenField
                Layout.fillWidth: true
                color: sheet.theme.foreground
                font.family: sheet.theme.font
                font.pixelSize: 13
                echoMode: TextInput.Password
                text: sheet.tokenText
                onTextChanged: sheet.tokenText = text
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Escape) { sheet.close(); event.accepted = true; }
                    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { sheet.save(); event.accepted = true; }
                }
            }
            Word {
                visible: sheet.notice !== ""
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: sheet.theme.accent
                text: sheet.notice
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                Text {
                    text: "SAVE"
                    color: sheet.theme.foreground
                    font.family: sheet.theme.font
                    font.pixelSize: 12
                    MouseArea { anchors.fill: parent; anchors.margins: -6; cursorShape: Qt.PointingHandCursor; onClicked: sheet.save() }
                }
            }
        }
    }
}
