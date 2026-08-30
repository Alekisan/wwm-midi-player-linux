import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.wwm.player

ApplicationWindow {
    id: window

    width: 760
    height: 680
    minimumWidth: 620
    minimumHeight: 600
    visible: true
    title: qsTr("Where Winds Meet — MIDI Player")

    PlayerBridge {
        id: bridge
    }

    // Drives the player's event stream on the GUI thread.
    Timer {
        interval: 16
        running: true
        repeat: true
        onTriggered: bridge.poll()
    }

    FileDialog {
        id: fileDialog
        title: qsTr("Choose a MIDI file")
        nameFilters: [qsTr("MIDI files (*.mid *.midi)"), qsTr("All files (*)")]
        onAccepted: bridge.loadFile(selectedFile)
    }

    function formatTime(seconds) {
        if (isNaN(seconds) || seconds < 0)
            seconds = 0;
        var total = Math.floor(seconds);
        var mins = Math.floor(total / 60);
        var secs = total % 60;
        return mins + ":" + (secs < 10 ? "0" : "") + secs;
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 14

        // ---- File ----------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                text: qsTr("Load MIDI…")
                icon.name: "document-open"
                onClicked: fileDialog.open()
            }

            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                text: bridge.fileName === "" ? qsTr("No file loaded") : bridge.fileName
                font.bold: bridge.fileName !== ""
            }
        }

        // ---- Song info ----------------------------------------------
        Frame {
            Layout.fillWidth: true

            GridLayout {
                anchors.fill: parent
                columns: 4
                columnSpacing: 20
                rowSpacing: 4

                Label { text: qsTr("Notes:"); opacity: 0.7 }
                Label { text: bridge.noteCount }
                Label { text: qsTr("Tempo:"); opacity: 0.7 }
                Label { text: bridge.bpm + qsTr(" BPM") }

                Label { text: qsTr("Length:"); opacity: 0.7 }
                Label { text: window.formatTime(bridge.duration) }
                Label { text: qsTr("Transpose:"); opacity: 0.7 }
                Label { text: (bridge.transpose > 0 ? "+" : "") + bridge.transpose + qsTr(" semitones") }
            }
        }

        // ---- Timeline ------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: window.formatTime(bridge.position)
                font.family: "monospace"
            }

            Slider {
                id: timeline
                Layout.fillWidth: true
                enabled: bridge.loaded
                from: 0
                to: Math.max(bridge.duration, 0.001)
                onMoved: bridge.seekTo(value)

                Binding on value {
                    when: !timeline.pressed
                    value: bridge.position
                }
            }

            Label {
                text: window.formatTime(bridge.duration)
                font.family: "monospace"
            }
        }

        // ---- Transport -----------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                text: bridge.playing && !bridge.paused ? qsTr("Pause") : qsTr("Play")
                icon.name: bridge.playing && !bridge.paused ? "media-playback-pause" : "media-playback-start"
                enabled: bridge.loaded
                onClicked: bridge.togglePlayPause()
            }

            Button {
                text: qsTr("Stop")
                icon.name: "media-playback-stop"
                enabled: bridge.loaded && bridge.playing
                onClicked: bridge.stop()
            }

            Item { Layout.fillWidth: true }

            Label { text: qsTr("Speed:"); opacity: 0.7 }

            Slider {
                id: speedSlider
                Layout.preferredWidth: 160
                from: 0.25
                to: 2.0
                stepSize: 0.05
                value: 1.0
                onMoved: bridge.applySpeed(value)
            }

            Label {
                text: speedSlider.value.toFixed(2) + "×"
                font.family: "monospace"
            }
        }

        // ---- Go Live -------------------------------------------------
        Button {
            id: liveButton
            Layout.fillWidth: true
            Layout.preferredHeight: 58
            onClicked: bridge.goLive(!bridge.live)

            background: Rectangle {
                radius: 6
                color: bridge.live
                    ? (liveButton.pressed ? "#a5281b" : "#c0392b")
                    : (liveButton.pressed ? "#1e8449" : "#27ae60")
                border.color: Qt.darker(color, 1.3)
                border.width: 1
            }

            contentItem: ColumnLayout {
                spacing: 1
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: bridge.live ? qsTr("● LIVE") : qsTr("GO LIVE")
                    color: "white"
                    font.bold: true
                    font.pointSize: 13
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: bridge.live
                        ? qsTr("Input is being sent to the game — click to stop")
                        : qsTr("Click to start sending input to the game")
                    color: "white"
                    opacity: 0.9
                    font.pointSize: 9
                }
            }
        }

        // ---- Key visualizer ------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 8

                Label {
                    text: qsTr("Keys")
                    opacity: 0.7
                }

                GridLayout {
                    id: keyGrid
                    Layout.alignment: Qt.AlignHCenter
                    columns: 7
                    columnSpacing: 6
                    rowSpacing: 6

                    // High row (q–u), mid row (a–j), low row (z–m).
                    readonly property var keys: ["q", "w", "e", "r", "t", "y", "u",
                                                 "a", "s", "d", "f", "g", "h", "j",
                                                 "z", "x", "c", "v", "b", "n", "m"]

                    Repeater {
                        id: keyRepeater
                        model: keyGrid.keys

                        delegate: Rectangle {
                            id: keyCell

                            property bool lit: false
                            property string modifier: ""

                            function flash(chord) {
                                var parts = chord.split("+");
                                keyCell.modifier = parts.length > 1 ? parts[0] : "";
                                keyCell.lit = true;
                                offTimer.restart();
                            }

                            implicitWidth: 62
                            implicitHeight: 52
                            radius: 5
                            color: lit
                                ? (modifier === "shift" ? "#2980b9"
                                    : modifier === "ctrl" ? "#8e44ad" : "#16a085")
                                : Qt.rgba(0.5, 0.5, 0.5, 0.15)
                            border.width: 1
                            border.color: Qt.rgba(0.5, 0.5, 0.5, 0.35)

                            Behavior on color {
                                ColorAnimation { duration: 90 }
                            }

                            ColumnLayout {
                                anchors.centerIn: parent
                                spacing: 0

                                Label {
                                    Layout.alignment: Qt.AlignHCenter
                                    text: modelData.toUpperCase()
                                    font.bold: true
                                    font.pointSize: 13
                                    color: keyCell.lit ? "white" : palette.text
                                }
                                Label {
                                    Layout.alignment: Qt.AlignHCenter
                                    text: keyCell.lit ? keyCell.modifier : ""
                                    visible: keyCell.lit && keyCell.modifier !== ""
                                    color: "white"
                                    font.pointSize: 8
                                }
                            }

                            Timer {
                                id: offTimer
                                interval: 140
                                onTriggered: {
                                    keyCell.lit = false;
                                    keyCell.modifier = "";
                                }
                            }
                        }
                    }
                }

                Item { Layout.fillHeight: true }
            }
        }

        // ---- Status --------------------------------------------------
        Label {
            Layout.fillWidth: true
            text: bridge.status
            elide: Text.ElideRight
            opacity: 0.8
        }
    }

    Connections {
        target: bridge

        function onNoteFired(note, chord) {
            var base = chord.split("+").pop();
            var index = keyGrid.keys.indexOf(base);
            if (index >= 0) {
                var item = keyRepeater.itemAt(index);
                if (item)
                    item.flash(chord);
            }
        }
    }
}
