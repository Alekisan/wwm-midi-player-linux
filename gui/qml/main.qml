import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.wwm.player

ApplicationWindow {
    id: window

    width: 820
    height: 640
    minimumWidth: 640
    minimumHeight: 560
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

    // First-run check: if input injection can't reach /dev/uinput yet, offer to
    // install the udev rule up front.
    Component.onCompleted: {
        if (!bridge.uinput_ready)
            uinputDialog.open();
    }

    Dialog {
        id: uinputDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Input injection needs /dev/uinput access")
        padding: 20

        ColumnLayout {
            spacing: 12
            Layout.maximumWidth: 440

            Label {
                wrapMode: Text.WordWrap
                text: qsTr("Playing MIDI into the game sends keystrokes through a virtual input device (/dev/uinput).\n\nYour account needs write access to it. This player can install a udev rule that grants access to the logged-in user — no reboot needed. You'll be asked to authenticate.")
            }

            Label {
                wrapMode: Text.WordWrap
                opacity: 0.7
                text: qsTr("You can also skip this and keep using audio preview only.")
            }
        }

        footer: DialogButtonBox {
            Button {
                text: qsTr("Install…")
                icon.name: "dialog-password"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
            Button {
                text: qsTr("Not now")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }

        onAccepted: bridge.install_uinput_rule()
    }

    FileDialog {
        id: fileDialog
        title: qsTr("Choose a MIDI file")
        nameFilters: [qsTr("MIDI files (*.mid *.midi)"), qsTr("All files (*)")]
        onAccepted: bridge.load_file(selectedFile)
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Choose a folder of MIDI files")
        onAccepted: bridge.add_folder(selectedFolder)
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
        spacing: 12

        // ---- File -----------------------------------------------------
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
                text: bridge.file_name === "" ? qsTr("No file loaded") : bridge.file_name
                font.bold: bridge.file_name !== ""
            }
        }

        // ---- Song info ------------------------------------------------
        Frame {
            Layout.fillWidth: true

            GridLayout {
                anchors.fill: parent
                columns: 4
                columnSpacing: 6
                rowSpacing: 4

                // Label / value pairs: labels right-aligned in a fixed column,
                // values stretch, so the four pairs line up evenly.
                Label { text: qsTr("Notes:"); opacity: 0.7; horizontalAlignment: Text.AlignRight }
                Label { text: bridge.note_count; Layout.fillWidth: true }
                Label { text: qsTr("Tempo:"); opacity: 0.7; horizontalAlignment: Text.AlignRight }
                Label { text: bridge.bpm + qsTr(" BPM"); Layout.fillWidth: true }

                Label { text: qsTr("Length:"); opacity: 0.7; horizontalAlignment: Text.AlignRight }
                Label { text: window.formatTime(bridge.duration); Layout.fillWidth: true }
                Label { text: qsTr("Transpose:"); opacity: 0.7; horizontalAlignment: Text.AlignRight }
                Label { text: (bridge.transpose > 0 ? "+" : "") + bridge.transpose + qsTr(" semitones"); Layout.fillWidth: true }
            }
        }

        // ---- Timeline -------------------------------------------------
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
                onMoved: bridge.seek_to(value)

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

        // ---- Transport ------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                text: bridge.playing && !bridge.paused ? qsTr("Pause") : qsTr("Play")
                icon.name: bridge.playing && !bridge.paused ? "media-playback-pause" : "media-playback-start"
                enabled: bridge.loaded
                onClicked: bridge.toggle_play_pause()
            }

            Button {
                text: qsTr("Stop")
                icon.name: "media-playback-stop"
                enabled: bridge.loaded && bridge.playing
                onClicked: bridge.stop()
            }

            CheckBox {
                id: previewToggle
                text: qsTr("Preview")
                checked: bridge.preview
                onToggled: bridge.toggle_preview(checked)

                ToolTip.visible: hovered
                ToolTip.text: qsTr("Play audio locally")
            }

            ComboBox {
                id: instrumentPicker
                model: [
                    "Guqin (古琴)",
                    "Pipa (琵琶)",
                    "Erhu (二胡)",
                    "Konghou (箜篌)",
                    "Fangxiang (方響)",
                ]
                currentIndex: bridge.instrument
                enabled: previewToggle.checked
                onActivated: bridge.choose_instrument(index)

                ToolTip.visible: hovered
                ToolTip.text: qsTr("Preview instrument")
            }

            Item { Layout.fillWidth: true }

            Label { text: qsTr("Speed:"); opacity: 0.7 }

            Slider {
                id: speedSlider
                Layout.preferredWidth: 170
                from: 0.25
                to: 2.0
                stepSize: 0.25
                value: 1.0
                onMoved: bridge.apply_speed(value)
            }

            Label {
                text: speedSlider.value.toFixed(2) + "×"
                font.family: "monospace"
            }
        }

        // ---- Go Live --------------------------------------------------
        Button {
            id: liveButton
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            enabled: bridge.game_running
            onClicked: {
                if (!bridge.live && !bridge.uinput_ready)
                    uinputDialog.open();
                else
                    bridge.go_live(!bridge.live);
            }

            background: Rectangle {
                radius: 6
                color: {
                    if (!liveButton.enabled)
                        return Qt.rgba(0.5, 0.5, 0.5, 0.25);
                    if (bridge.live)
                        return liveButton.pressed ? "#a5281b" : "#c0392b";
                    return liveButton.pressed ? "#1e8449" : "#27ae60";
                }
                border.color: Qt.darker(color, 1.3)
                border.width: 1
            }

            contentItem: ColumnLayout {
                spacing: 1

                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: bridge.live ? qsTr("● LIVE") : qsTr("GO LIVE")
                    color: liveButton.enabled ? "white" : palette.text
                    opacity: liveButton.enabled ? 1.0 : 0.5
                    font.bold: true
                    font.pointSize: 13
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: {
                        if (!bridge.game_running)
                            return qsTr("Waiting for the game…");
                        if (bridge.live)
                            return qsTr("Input is being sent to the game — click to stop");
                        return qsTr("Click to start sending input to the game");
                    }
                    color: liveButton.enabled ? "white" : palette.text
                    opacity: liveButton.enabled ? 0.9 : 0.5
                    font.pointSize: 9
                }
            }
        }

        // ---- Playlist -------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 6

                RowLayout {
                    Layout.fillWidth: true

                    Label {
                        text: qsTr("Playlist")
                        font.bold: true
                    }

                    Item { Layout.fillWidth: true }

                    Button {
                        text: qsTr("Add folder…")
                        icon.name: "folder-open"
                        flat: true
                        onClicked: folderDialog.open()
                    }
                }

                ListView {
                    id: playlist
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: bridge.songs

                    ScrollBar.vertical: ScrollBar {}

                    delegate: ItemDelegate {
                        width: playlist.width
                        highlighted: index === bridge.current_index
                        onClicked: bridge.select_song(index)

                        contentItem: RowLayout {
                            spacing: 8

                            Label {
                                text: index + 1
                                opacity: 0.5
                                font.family: "monospace"
                                Layout.preferredWidth: 28
                                horizontalAlignment: Text.AlignRight
                            }

                            Label {
                                text: modelData
                                elide: Text.ElideMiddle
                                Layout.fillWidth: true
                            }

                            ToolButton {
                                icon.name: "edit-delete"
                                onClicked: bridge.remove_song(index)

                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Remove from playlist")
                            }
                        }
                    }
                }

                Label {
                    visible: bridge.songs.length === 0
                    text: qsTr("No MIDI files yet — use “Load MIDI…” or “Add folder…”")
                    opacity: 0.6
                    Layout.alignment: Qt.AlignHCenter
                }
            }
        }

        // ---- Status ---------------------------------------------------
        Label {
            Layout.fillWidth: true
            text: bridge.status
            elide: Text.ElideRight
            opacity: 0.8
        }
    }
}
