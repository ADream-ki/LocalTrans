import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../components"
import "../theme"

Item {
    id: root

    property var asrModelList: modelVM ? modelVM.asrModels : []
    property var mtModelList: modelVM ? modelVM.mtModels : []
    property var ttsModelList: modelVM ? modelVM.ttsModels : []
    property var lociModelList: modelVM ? modelVM.lociModels : []
    property string modelDirText: ""

    Component.onCompleted: {
        if (modelVM) {
            modelDirText = String(modelVM.modelDir || "")
            modelVM.refreshModels()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.bgPrimary
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.width < 1000 ? Theme.spacingM : Theme.spacingXL
        spacing: Theme.spacingL

        RowLayout {
            Layout.fillWidth: true

            Label {
                text: qsTr("模型管理")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontXXL
                font.weight: Theme.weightBold
                color: Theme.textPrimary
            }

            Item { Layout.fillWidth: true }

            Label {
                text: qsTr("模型目录: ") + root.modelDirText
                font.family: Theme.fontFamilyMono
                font.pixelSize: Theme.fontS
                color: Theme.textTertiary
                Layout.preferredWidth: Math.max(200, root.width * 0.34)
                Layout.maximumWidth: Math.max(200, root.width * 0.42)
                elide: Text.ElideMiddle
            }

            Button {
                text: qsTr("刷新")
                onClicked: {
                    if (!modelVM) return
                    modelVM.refreshModels()
                    root.modelDirText = String(modelVM.modelDir || "")
                }
                background: Rectangle {
                    radius: Theme.radiusMedium
                    color: Theme.bgElevated
                    border.width: 1
                    border.color: Theme.border
                }
                contentItem: Label {
                    text: parent.text
                    color: Theme.textSecondary
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontS
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }

        TabBar {
            id: modelTabBar
            Layout.fillWidth: true
            spacing: Theme.spacingS
            background: Rectangle {
                color: "transparent"
            }

            TabButton { text: qsTr("ASR 模型") }
            TabButton { text: qsTr("MT 模型") }
            TabButton { text: qsTr("TTS 模型") }
            TabButton { text: qsTr("Loci 模型") }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: modelTabBar.currentIndex

            ModelListView {
                model: root.asrModelList
                onDownloadClicked: function(name) { if (modelVM) modelVM.downloadModel(name) }
                onDeleteClicked: function(name) { if (modelVM) modelVM.deleteModel(name) }
            }

            ModelListView {
                model: root.mtModelList
                onDownloadClicked: function(name) { if (modelVM) modelVM.downloadModel(name) }
                onDeleteClicked: function(name) { if (modelVM) modelVM.deleteModel(name) }
            }

            ModelListView {
                model: root.ttsModelList
                onDownloadClicked: function(name) { if (modelVM) modelVM.downloadModel(name) }
                onDeleteClicked: function(name) { if (modelVM) modelVM.deleteModel(name) }
            }

            ColumnLayout {
                spacing: Theme.spacingM

                Button {
                    text: qsTr("添加 Loci 模型")
                    onClicked: {
                        if (modelVM)
                            modelVM.selectLociModel()
                    }
                    background: Rectangle {
                        radius: Theme.radiusMedium
                        color: Theme.primary
                    }
                    contentItem: Label {
                        text: parent.text
                        color: Theme.textInverse
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontS
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }

                ModelListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    model: root.lociModelList
                    showDownload: false
                    onDeleteClicked: function(name) { if (modelVM) modelVM.deleteModel(name) }
                }
            }
        }
    }
}

