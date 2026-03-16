import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

import "pages"
import "theme"

Window {
    id: root
    visible: true
    width: 1320
    height: 840
    minimumWidth: 960
    minimumHeight: 620
    title: qsTr("LocalTrans Pro")
    color: Theme.bgPrimary

    Rectangle {
        anchors.fill: parent
        color: Theme.bgPrimary
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            color: Theme.bgPrimary
            border.width: 1
            border.color: Theme.border

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacingXL
                anchors.rightMargin: Theme.spacingXL
                spacing: Theme.spacingL

                Label {
                    text: "LOCALTRANS PRO"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontL
                    font.weight: Theme.weightBold
                    color: Theme.primary
                }

                Item { Layout.fillWidth: true }

                TabBar {
                    id: topTabs
                    Layout.preferredHeight: 38
                    currentIndex: 0
                    spacing: Theme.spacingS

                    background: Rectangle {
                        color: "transparent"
                    }

                    TabButton { text: qsTr("会话") }
                    TabButton { text: qsTr("设置") }
                    TabButton { text: qsTr("模型") }
                    TabButton { text: qsTr("诊断") }
                }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: topTabs.currentIndex

            SessionPage {}
            SettingsPage {}
            ModelPage {}
            DiagnosticsPage {}
        }
    }

    Dialog {
        id: errorDialog
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        background: Rectangle {
            radius: Theme.radiusLarge
            color: Theme.bgElevated
            border.width: 1
            border.color: Theme.error
        }

        contentItem: Label {
            id: errorText
            text: ""
            color: Theme.error
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontM
            wrapMode: Text.WordWrap
            padding: Theme.spacingL
        }

        property alias message: errorText.text
    }

    Connections {
        target: sessionVM
        function onErrorOccurred(error) {
            errorDialog.message = error
            errorDialog.open()
        }
    }
}

