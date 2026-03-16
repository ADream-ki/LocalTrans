import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"

Item {
    id: root

    property bool compact: width < 1080

    function syncRouteSelections() {
        if (!audioDeviceVM || !peerInputCombo.model || peerInputCombo.model.length === 0)
            return
        var peerIn = String(sessionVM ? sessionVM.peerInputDeviceId : "")
        var peerOut = String(sessionVM ? sessionVM.peerOutputDeviceId : "")
        var selfIn = String(sessionVM ? sessionVM.selfInputDeviceId : "")
        var selfOut = String(sessionVM ? sessionVM.selfOutputDeviceId : "")

        for (var i = 0; i < peerInputCombo.model.length; ++i) {
            if (String(peerInputCombo.model[i].id) === peerIn) {
                peerInputCombo.currentIndex = i
                break
            }
        }
        peerInputCombo.currentIndex = 0
        for (var j = 0; j < peerOutputCombo.model.length; ++j) {
            if (String(peerOutputCombo.model[j].id) === peerOut) {
                peerOutputCombo.currentIndex = j
                break
            }
        }
        for (var k = 0; k < selfInputCombo.model.length; ++k) {
            if (String(selfInputCombo.model[k].id) === selfIn) {
                selfInputCombo.currentIndex = k
                break
            }
        }
        for (var m = 0; m < selfOutputCombo.model.length; ++m) {
            if (String(selfOutputCombo.model[m].id) === selfOut) {
                selfOutputCombo.currentIndex = m
                break
            }
        }
    }

    Component.onCompleted: {
        if (audioDeviceVM) {
            audioDeviceVM.refresh()
            syncRouteSelections()
        }
    }

    Connections {
        target: audioDeviceVM
        function onInputDevicesChanged() { root.syncRouteSelections() }
        function onOutputDevicesChanged() { root.syncRouteSelections() }
    }

    Connections {
        target: sessionVM
        function onRoutingChanged() { root.syncRouteSelections() }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.bgPrimary
    }

    GridLayout {
        anchors.fill: parent
        columns: root.compact ? 1 : 2
        columnSpacing: 0
        rowSpacing: 0

        Rectangle {
            Layout.fillHeight: true
            Layout.fillWidth: root.compact
            Layout.preferredWidth: root.compact ? parent.width : 296
            color: Theme.bgSecondary
            border.width: 1
            border.color: Theme.border

            ScrollView {
                anchors.fill: parent
                anchors.margins: Theme.spacingXL
                clip: true
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                ColumnLayout {
                    width: parent.availableWidth
                    spacing: Theme.spacingXL

                    RowLayout {
                        spacing: Theme.spacingS

                        Rectangle {
                            width: 8
                            height: 8
                            radius: 4
                            color: Theme.success
                        }

                        Label {
                            text: "LOCALTRANS PRO"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightBold
                            color: Theme.primary
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingM

                        Label {
                            text: qsTr("音频驱动路由")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontXS
                            font.weight: Theme.weightBold
                            color: Theme.textSecondary
                        }

                        Label {
                            text: qsTr("对方通道: A(输入) -> B(输出)")
                            color: Theme.textSecondary
                            font.pixelSize: Theme.fontS
                            font.family: Theme.fontFamily
                        }
                        ComboBox {
                            id: peerInputCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getInputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (sessionVM && currentIndex >= 0 && currentIndex < model.length)
                                    sessionVM.setPeerInputDeviceId(String(model[currentIndex].id))
                            }
                        }
                        ComboBox {
                            id: peerOutputCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getOutputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (sessionVM && currentIndex >= 0 && currentIndex < model.length)
                                    sessionVM.setPeerOutputDeviceId(String(model[currentIndex].id))
                            }
                        }

                        Label {
                            text: qsTr("我的通道: C(输入) -> D(输出)")
                            color: Theme.textSecondary
                            font.pixelSize: Theme.fontS
                            font.family: Theme.fontFamily
                        }
                        ComboBox {
                            id: selfInputCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getInputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (sessionVM && currentIndex >= 0 && currentIndex < model.length)
                                    sessionVM.setSelfInputDeviceId(String(model[currentIndex].id))
                            }
                        }
                        ComboBox {
                            id: selfOutputCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getOutputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (sessionVM && currentIndex >= 0 && currentIndex < model.length)
                                    sessionVM.setSelfOutputDeviceId(String(model[currentIndex].id))
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingM

                        Label {
                            text: qsTr("端侧 AI 引擎")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontXS
                            font.weight: Theme.weightBold
                            color: Theme.textSecondary
                        }

                        ComboBox {
                            id: asrBackendCombo
                            Layout.fillWidth: true
                            model: settingsVM ? settingsVM.getASRBackends() : []
                            textRole: "name"
                            Component.onCompleted: {
                                if (!settingsVM || !model) return
                                for (var i = 0; i < model.length; ++i) {
                                    if (String(model[i].value) === String(settingsVM.asrBackend)) {
                                        currentIndex = i
                                        return
                                    }
                                }
                            }
                            onActivated: {
                                if (settingsVM && currentIndex >= 0 && currentIndex < model.length) {
                                    settingsVM.asrBackend = model[currentIndex].value
                                    settingsVM.saveSettings()
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingS

                        Label {
                            text: qsTr("翻译方向")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontXS
                            font.weight: Theme.weightBold
                            color: Theme.textSecondary
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacingS

                            ComboBox {
                                id: sourceLangCombo
                                Layout.fillWidth: true
                                model: settingsVM ? settingsVM.getLanguages() : []
                                textRole: "name"
                                Component.onCompleted: {
                                    if (!settingsVM || !model) return
                                    for (var i = 0; i < model.length; ++i) {
                                        if (String(model[i].code) === String(settingsVM.sourceLang)) {
                                            currentIndex = i
                                            return
                                        }
                                    }
                                }
                                onActivated: {
                                    if (settingsVM && currentIndex >= 0 && currentIndex < model.length) {
                                        settingsVM.sourceLang = model[currentIndex].code
                                        settingsVM.saveSettings()
                                    }
                                }
                            }

                            Label {
                                text: "→"
                                color: Theme.textSecondary
                                font.pixelSize: Theme.fontL
                                font.family: Theme.fontFamily
                            }

                            ComboBox {
                                id: targetLangCombo
                                Layout.fillWidth: true
                                model: settingsVM ? settingsVM.getLanguages() : []
                                textRole: "name"
                                Component.onCompleted: {
                                    if (!settingsVM || !model) return
                                    for (var i = 0; i < model.length; ++i) {
                                        if (String(model[i].code) === String(settingsVM.targetLang)) {
                                            currentIndex = i
                                            return
                                        }
                                    }
                                }
                                onActivated: {
                                    if (settingsVM && currentIndex >= 0 && currentIndex < model.length) {
                                        settingsVM.targetLang = model[currentIndex].code
                                        settingsVM.saveSettings()
                                    }
                                }
                            }
                        }
                    }

                    Item { Layout.fillHeight: true }

                    Rectangle {
                        Layout.fillWidth: true
                        radius: Theme.radiusMedium
                        color: Theme.bgElevated
                        border.width: 1
                        border.color: Theme.border
                        implicitHeight: privacyText.implicitHeight + Theme.spacingL * 2

                        Label {
                            id: privacyText
                            anchors.fill: parent
                            anchors.margins: Theme.spacingL
                            text: qsTr("隐私保护已启用\n所有语音处理在本地完成。")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 66
                    color: Theme.bgPrimary
                    border.width: 1
                    border.color: Theme.border

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacingXL
                        anchors.rightMargin: Theme.spacingXL
                        spacing: Theme.spacingL

                        Label {
                            text: qsTr("实时翻译会话")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }

                        Rectangle {
                            radius: Theme.radiusFull
                            color: sessionVM && sessionVM.isRunning ? Theme.successBg : Theme.bgTertiary
                            border.width: 1
                            border.color: sessionVM && sessionVM.isRunning ? Theme.success : Theme.border
                            implicitHeight: 24
                            implicitWidth: statusLabel.implicitWidth + 16

                            Label {
                                id: statusLabel
                                anchors.centerIn: parent
                                text: sessionVM && sessionVM.isRunning ? qsTr("引擎运行中") : qsTr("引擎就绪")
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontXS
                                font.weight: Theme.weightSemibold
                                color: sessionVM && sessionVM.isRunning ? "#065F46" : Theme.textSecondary
                            }
                        }

                        Item { Layout.fillWidth: true }

                        Label {
                            text: qsTr("推理延迟: ") + (sessionVM ? Math.round(sessionVM.estimatedLatencyMs) : 0) + "ms"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                        }

                        Label {
                            text: qsTr("会话时长: ") + (sessionVM ? sessionVM.sessionDuration : "00:00")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                        }
                    }
                }

                ScrollView {
                    id: flowScroll
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                    Column {
                        id: flowColumn
                        width: flowScroll.availableWidth
                        spacing: Theme.spacingL
                        padding: Theme.spacingXL

                        Rectangle {
                            width: parent.width
                            radius: Theme.radiusLarge
                            color: Theme.bgElevated
                            border.width: 1
                            border.color: Theme.border
                            implicitHeight: sourceText.implicitHeight + transText.implicitHeight + Theme.spacingXL
                            visible: !!(sessionVM && (sessionVM.currentTranscription.length > 0 || sessionVM.currentTranslation.length > 0))

                            Column {
                                anchors.fill: parent
                                anchors.margins: Theme.spacingL
                                spacing: Theme.spacingS

                                Label {
                                    id: sourceText
                                    text: sessionVM ? sessionVM.currentTranscription : ""
                                    color: Theme.textSecondary
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontM
                                    wrapMode: Text.WordWrap
                                }

                                Label {
                                    id: transText
                                    text: sessionVM ? sessionVM.currentTranslation : ""
                                    color: Theme.textPrimary
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontXL
                                    font.weight: Theme.weightMedium
                                    wrapMode: Text.WordWrap
                                }
                            }
                        }

                        Repeater {
                            model: sessionVM ? sessionVM.history : []
                            delegate: Rectangle {
                                required property var modelData
                                width: flowColumn.width
                                radius: Theme.radiusLarge
                                color: Theme.bgElevated
                                border.width: 1
                                border.color: Theme.border
                                implicitHeight: src.implicitHeight + dst.implicitHeight + Theme.spacingXL

                                Column {
                                    anchors.fill: parent
                                    anchors.margins: Theme.spacingL
                                    spacing: Theme.spacingS

                                    Label {
                                        id: src
                                        text: modelData.source
                                        color: Theme.textSecondary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontM
                                        wrapMode: Text.WordWrap
                                    }

                                    Label {
                                        id: dst
                                        text: modelData.translation
                                        color: Theme.textPrimary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontXL
                                        font.weight: Theme.weightMedium
                                        wrapMode: Text.WordWrap
                                    }

                                    Label {
                                        visible: modelData.direction && modelData.direction.length > 0
                                        text: modelData.direction
                                        color: Theme.primary
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontXS
                                    }
                                }
                            }
                        }

                        Label {
                            visible: !(sessionVM && sessionVM.history.length > 0) && !(sessionVM && sessionVM.currentTranslation.length > 0)
                            text: qsTr("点击“开启实时翻译”开始会话")
                            color: Theme.textTertiary
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            horizontalAlignment: Text.AlignHCenter
                            width: parent.width
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 84
                    color: Theme.bgPrimary
                    border.width: 1
                    border.color: Theme.border

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacingXL
                        anchors.rightMargin: Theme.spacingXL
                        spacing: Theme.spacingL

                        Button {
                            id: startButton
                            text: sessionVM && sessionVM.isRunning ? qsTr("停止实时翻译") : qsTr("开启实时翻译")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            font.weight: Theme.weightSemibold
                            leftPadding: 22
                            rightPadding: 22
                            onClicked: {
                                if (!sessionVM) return
                                if (sessionVM.isRunning) sessionVM.stopSession()
                                else sessionVM.startSession()
                            }

                            background: Rectangle {
                                radius: Theme.radiusMedium
                                color: sessionVM && sessionVM.isRunning ? Theme.error : Theme.primary
                            }

                            contentItem: Label {
                                text: startButton.text
                                color: Theme.textInverse
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                font.weight: Theme.weightSemibold
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }

                        Button {
                            text: qsTr("清空记录")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            onClicked: {
                                if (sessionVM) sessionVM.clearHistory()
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

                        Item { Layout.fillWidth: true }

                        Label {
                            text: sessionVM ? sessionVM.pipelineSummary : ""
                            color: Theme.textTertiary
                            font.family: Theme.fontFamilyMono
                            font.pixelSize: Theme.fontXS
                            elide: Text.ElideRight
                            Layout.maximumWidth: 420
                        }
                    }
                }
            }
        }
    }
}
