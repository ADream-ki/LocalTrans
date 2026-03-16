import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"

Item {
    id: root

    property alias model: listView.model
    property bool showDownload: true

    signal downloadClicked(string name)
    signal deleteClicked(string name)

    ListView {
        id: listView
        anchors.fill: parent
        spacing: Theme.spacingM
        clip: true

        delegate: Rectangle {
            required property var modelData
            width: ListView.view.width
            radius: Theme.radiusLarge
            color: Theme.bgElevated
            border.width: 1
            border.color: Theme.border
            implicitHeight: 76

            RowLayout {
                anchors.fill: parent
                anchors.margins: Theme.spacingL
                spacing: Theme.spacingL

                Rectangle {
                    width: 40
                    height: 40
                    radius: 20
                    color: {
                        if (modelData.status === "ready") return Theme.successBg
                        if (modelData.status === "downloading") return Theme.warningBg
                        if (modelData.status === "error") return Theme.errorBg
                        return Theme.bgTertiary
                    }
                    border.width: 1
                    border.color: Theme.border

                    Label {
                        anchors.centerIn: parent
                        text: {
                            if (modelData.type === "asr") return "🎤"
                            if (modelData.type === "mt") return "🌐"
                            if (modelData.type === "tts") return "🔊"
                            if (modelData.type === "loci") return "🧠"
                            return "📦"
                        }
                        font.pixelSize: Theme.fontL
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Label {
                        text: modelData.name || ""
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontM
                        font.weight: Theme.weightSemibold
                        color: Theme.textPrimary
                        elide: Text.ElideMiddle
                    }

                    RowLayout {
                        spacing: Theme.spacingS

                        Label {
                            text: modelData.size || ""
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                        }

                        Rectangle {
                            radius: Theme.radiusFull
                            implicitHeight: 20
                            implicitWidth: statusLabel.implicitWidth + 14
                            color: {
                                if (modelData.status === "ready") return Theme.successBg
                                if (modelData.status === "downloading") return Theme.warningBg
                                if (modelData.status === "error") return Theme.errorBg
                                return Theme.bgTertiary
                            }
                            border.width: 1
                            border.color: Theme.border

                            Label {
                                id: statusLabel
                                anchors.centerIn: parent
                                text: {
                                    if (modelData.status === "ready") return qsTr("已就绪")
                                    if (modelData.status === "downloading") return qsTr("下载中")
                                    if (modelData.status === "error") return qsTr("错误")
                                    return qsTr("未下载")
                                }
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontXS
                                color: Theme.textSecondary
                            }
                        }
                    }
                }

                RowLayout {
                    spacing: Theme.spacingS

                    Button {
                        visible: root.showDownload && modelData.status !== "ready"
                        text: qsTr("下载")
                        onClicked: root.downloadClicked(modelData.name || "")
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

                    Button {
                        visible: modelData.status === "ready"
                        text: qsTr("删除")
                        onClicked: root.deleteClicked(modelData.name || "")
                        background: Rectangle {
                            radius: Theme.radiusMedium
                            color: Theme.error
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
                }
            }
        }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
        }
    }
}

