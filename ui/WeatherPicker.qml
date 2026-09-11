import QtQuick
import QtQuick.Layouts
import "Location.js" as Location

// Weather source and API key (DESIGN.md, current conditions). Writes
// weather.toml, never state.json. The engine fetches; this form only
// chooses the source.
Item {
    id: picker
    property var theme
    property var config
    property bool compact: false
    property real cardTop: 20
    property bool open: false
    property string source: ""
    property alias keyText: keyField.text
    property alias urlText: urlField.text
    property string error: ""
    readonly property var selected: Location.WEATHER_SOURCES.find(s => s.id === picker.source) || null
    readonly property bool needsKey: selected && selected.needs === "key"
    readonly property bool needsUrl: selected && selected.needs === "url"
    component Word: Text {
        color: picker.theme.foreground
        font.family: picker.theme.font
        font.pixelSize: 12
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
    visible: open
    focus: open
    function load() {
        var w = Location.weatherSettings(config.values, config.weatherValues);
        source = w.source;
        keyText = w.apiKey;
        urlText = w.url;
        error = "";
    }
    function show() {
        load();
        open = true;
        Qt.callLater(() => { if (needsUrl) urlField.forceActiveFocus(); else keyField.forceActiveFocus(); });
    }
    function close() {
        open = false;
        error = "";
        keyField.focus = false;
        urlField.focus = false;
    }
    function save() {
        if (!source) { config.writeWeather("", "", ""); close(); return; }
        if (needsKey && !keyText.trim()) { error = "API key is required"; return; }
        if (needsUrl && !urlText.trim()) { error = "WeeWX URL is required"; return; }
        if (needsUrl && !/^https?:\/\//i.test(urlText.trim())) { error = "URL must be http or https"; return; }
        if (/["\n]/.test(keyText)) { error = "API key must not contain quotes or newlines"; return; }
        config.writeWeather(source, needsKey ? keyText.trim() : "", needsUrl ? urlText.trim() : "");
        close();
    }
    function clear() {
        source = "";
        keyText = "";
        urlText = "";
        config.writeWeather("", "", "");
        close();
    }
    Keys.onPressed: event => {
        if (!open) return;
        event.accepted = true;
        if (event.key === Qt.Key_Escape) close();
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) save();
        else event.accepted = false;
    }
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(picker.theme.background, .55)
        MouseArea { anchors.fill: parent; onClicked: picker.close() }
    }
    Rectangle {
        id: card
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: picker.cardTop
        anchors.margins: picker.compact ? 12 : 20
        implicitHeight: body.implicitHeight + 24
        color: Qt.alpha(picker.theme.background, .96)
        border.width: 1
        border.color: picker.theme.foreground
        MouseArea { anchors.fill: parent }
        ColumnLayout {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            spacing: 8
            Word { text: "WEATHER SOURCE"; font.bold: true; font.letterSpacing: 1.5 }
            Word {
                text: "Current conditions at the map centre. Your key stays in weather.toml and is never written to state.json."
                wrapMode: Text.Wrap
                font.pixelSize: 11
                opacity: .65
                Layout.fillWidth: true
            }
            Repeater {
                model: Location.WEATHER_SOURCES
                Rectangle {
                    id: row
                    required property var modelData
                    readonly property bool current: picker.source === modelData.id
                    Layout.fillWidth: true
                    implicitHeight: 28
                    color: current || area.containsMouse ? Qt.alpha(picker.theme.foreground, .08) : "transparent"
                    Word {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        text: modelData.label
                        color: row.current ? picker.theme.accent : picker.theme.foreground
                    }
                    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; onClicked: picker.source = row.modelData.id }
                }
            }
            RowLayout {
                visible: picker.needsKey
                Layout.fillWidth: true
                Word { text: "KEY"; font.pixelSize: 10; opacity: .55; Layout.preferredWidth: 36 }
                TextInput {
                    id: keyField
                    Layout.fillWidth: true
                    color: picker.theme.foreground
                    font.family: picker.theme.font
                    font.pixelSize: 12
                    echoMode: TextInput.Password
                    selectByMouse: true
                    clip: true
                }
            }
            RowLayout {
                visible: picker.needsUrl
                Layout.fillWidth: true
                Word { text: "URL"; font.pixelSize: 10; opacity: .55; Layout.preferredWidth: 36 }
                TextInput {
                    id: urlField
                    Layout.fillWidth: true
                    color: picker.theme.foreground
                    font.family: picker.theme.font
                    font.pixelSize: 12
                    selectByMouse: true
                    clip: true
                }
            }
            Word {
                visible: picker.error !== ""
                text: picker.error
                color: picker.theme.accent
                Layout.fillWidth: true
            }
            RowLayout {
                Layout.fillWidth: true
                Word { text: "↵ save"; font.pixelSize: 10; opacity: .55 }
                Word { text: "esc close"; font.pixelSize: 10; opacity: .55 }
                Item { Layout.fillWidth: true }
                Rectangle {
                    implicitWidth: clearLabel.implicitWidth + 16
                    implicitHeight: 26
                    color: "transparent"
                    border.width: 1
                    border.color: Qt.alpha(picker.theme.foreground, .22)
                    Word { id: clearLabel; anchors.centerIn: parent; text: "CLEAR" }
                    MouseArea { anchors.fill: parent; onClicked: picker.clear() }
                }
                Rectangle {
                    implicitWidth: saveLabel.implicitWidth + 16
                    implicitHeight: 26
                    color: "transparent"
                    border.width: 1
                    border.color: Qt.alpha(picker.theme.foreground, .22)
                    Word { id: saveLabel; anchors.centerIn: parent; text: "SAVE" }
                    MouseArea { anchors.fill: parent; onClicked: picker.save() }
                }
            }
        }
    }
}
