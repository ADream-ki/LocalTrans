import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"

Rectangle {
    id: root
    color: Theme.bgElevated
    radius: Theme.radiusLarge
    border.width: 1
    border.color: Theme.border
    implicitHeight: content.implicitHeight + Theme.spacingXL

    property string title: ""
    property var items: []

    ColumnLayout {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Theme.spacingL
        spacing: Theme.spacingM

        Label {
            text: root.title
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontM
            font.weight: Theme.weightBold
            color: Theme.primary
        }

        Repeater {
            model: root.items

            Rectangle {
                required property var modelData
                Layout.fillWidth: true
                implicitHeight: row.implicitHeight + 10
                radius: Theme.radiusSmall
                color: Theme.bgTertiary

                RowLayout {
                    id: row
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: Theme.spacingM

                    Label {
                        text: modelData.label || ""
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontS
                        color: Theme.textSecondary
                        Layout.preferredWidth: 140
                    }

                    Label {
                        text: modelData.value || ""
                        font.family: Theme.fontFamilyMono
                        font.pixelSize: Theme.fontS
                        color: Theme.textPrimary
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }
}

