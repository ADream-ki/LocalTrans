// SettingsPage.qml - 设置页面 (优化版)
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"
import "../components"

Item {
    id: root
    
    property var settingsVM: null
    property var sessionVM: null
    property bool compactForm: width < 980

    function syncInputSelection() {
        if (typeof inputDeviceCombo === "undefined")
            return
        if (!audioDeviceVM || !inputDeviceCombo.model || inputDeviceCombo.model.length === 0)
            return
        var selected = audioDeviceVM.selectedInput
        for (var i = 0; i < inputDeviceCombo.model.length; ++i) {
            if (String(inputDeviceCombo.model[i].id) === String(selected)) {
                inputDeviceCombo.currentIndex = i
                return
            }
        }
        inputDeviceCombo.currentIndex = 0
    }

    function syncOutputSelection() {
        if (typeof outputDeviceCombo === "undefined")
            return
        if (!audioDeviceVM || !outputDeviceCombo.model || outputDeviceCombo.model.length === 0)
            return
        var selected = audioDeviceVM.selectedOutput
        for (var i = 0; i < outputDeviceCombo.model.length; ++i) {
            if (String(outputDeviceCombo.model[i].id) === String(selected)) {
                outputDeviceCombo.currentIndex = i
                return
            }
        }
        outputDeviceCombo.currentIndex = 0
    }

    Component.onCompleted: {
        if (audioDeviceVM) {
            audioDeviceVM.refresh()
            syncInputSelection()
            syncOutputSelection()
        }
    }

    Connections {
        target: audioDeviceVM
        function onInputDevicesChanged() { root.syncInputSelection() }
        function onOutputDevicesChanged() { root.syncOutputSelection() }
        function onInputDeviceChanged(deviceId) { root.syncInputSelection() }
        function onOutputDeviceChanged(deviceId) { root.syncOutputSelection() }
    }
    
    ScrollView {
        id: settingsScroll
        anchors.fill: parent
        anchors.margins: root.width < 1000 ? Theme.spacingM : Theme.spacingXL
        clip: true
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        
        ColumnLayout {
            width: settingsScroll.availableWidth > 0
                   ? settingsScroll.availableWidth
                   : Math.max(320, root.width - settingsScroll.anchors.margins * 2)
            spacing: root.compactForm ? Theme.spacingL : Theme.spacingXL
            
            // 页面标题
            Label {
                text: qsTr("设置")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontXXL
                font.weight: Theme.weightBold
                color: Theme.textPrimary
            }
            
            // 快速预设
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingM
                
                Label {
                    text: qsTr("快速预设")
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontM
                    font.weight: Theme.weightSemibold
                    color: Theme.textSecondary
                }
                
                Item {
                    Layout.fillWidth: true
                    implicitHeight: presetFlow.height

                    Flow {
                        id: presetFlow
                        width: parent.width
                        spacing: Theme.spacingM

                        Repeater {
                            model: [
                                { name: qsTr("极速模式"), desc: qsTr("低延迟，适合对话"), icon: "⚡", color: Theme.accent },
                                { name: qsTr("高准确率"), desc: qsTr("最准确，适合会议"), icon: "🎯", color: Theme.success },
                                { name: qsTr("中文优化"), desc: qsTr("中文体验最佳"), icon: "中", color: Theme.warning },
                                { name: qsTr("Loci增强"), desc: qsTr("本地AI加速"), icon: "✦", color: Theme.primary }
                            ]

                            GlassCard {
                                width: {
                                    var cols = root.width >= 1500 ? 4 : (root.width >= 1200 ? 3 : (root.width >= 900 ? 2 : 1))
                                    return Math.max(220, (presetFlow.width - (cols - 1) * presetFlow.spacing) / cols)
                                }
                                height: 84
                                interactive: true

                                content: RowLayout {
                                    anchors.fill: parent
                                    spacing: Theme.spacingM

                                    Rectangle {
                                        Layout.preferredWidth: 48
                                        Layout.preferredHeight: 48
                                        radius: Theme.radiusMedium
                                        color: modelData.color + "33"

                                        Label {
                                            anchors.centerIn: parent
                                            text: modelData.icon
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontXL
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2

                                        Label {
                                            text: modelData.name
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontM
                                            font.weight: Theme.weightSemibold
                                            color: Theme.textPrimary
                                            elide: Text.ElideRight
                                        }

                                        Label {
                                            text: modelData.desc
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontS
                                            color: Theme.textTertiary
                                            wrapMode: Text.WordWrap
                                            maximumLineCount: 2
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // ASR 设置
            GlassCard {
                Layout.fillWidth: true
                interactive: false
                elevated: true
                
                content: ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spacingM
                    
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingS
                        
                        Label {
                            text: "🎤"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                        }
                        
                        Label {
                            text: qsTr("语音识别 (ASR)")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }
                    }
                    
                    // 模型选择
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("模型")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: asrModelCombo
                            Layout.fillWidth: true
                            model: ["whisper-tiny", "whisper-base", "whisper-small", "whisper-medium"]
                            currentIndex: 1
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: asrModelCombo.displayText
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                    
                    // 语言选择
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("语言")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: asrLangCombo
                            Layout.fillWidth: true
                            model: [qsTr("自动检测"), "English", "中文", "日本語", "한국어"]
                            currentIndex: 0
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: asrLangCombo.displayText
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                    
                    // 输入设备
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("输入设备")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: inputDeviceCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getInputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (audioDeviceVM && currentIndex >= 0 && currentIndex < model.length) {
                                    audioDeviceVM.selectInput(String(model[currentIndex].id))
                                }
                            }
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: inputDeviceCombo.currentIndex >= 0 && inputDeviceCombo.model.length > inputDeviceCombo.currentIndex
                                      ? inputDeviceCombo.model[inputDeviceCombo.currentIndex].name
                                      : qsTr("请选择输入设备")
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                }
            }
            
            // MT 设置
            GlassCard {
                Layout.fillWidth: true
                interactive: false
                elevated: true
                
                content: ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spacingM
                    
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingS
                        
                        Label {
                            text: "🌐"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                        }
                        
                        Label {
                            text: qsTr("机器翻译 (MT)")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }
                    }
                    
                    // 模型选择
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("模型")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: mtModelCombo
                            Layout.fillWidth: true
                            model: ["Qwen2.5-3B", "Qwen2.5-7B", "NLLB-200", "M2M100"]
                            currentIndex: 0
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: mtModelCombo.displayText
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                    
                    // Loci 增强开关
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("Loci 增强")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        RowLayout {
                            spacing: Theme.spacingM
                            
                            Switch {
                                id: lociSwitch
                                checked: true
                                
                                indicator: Rectangle {
                                    implicitWidth: 48
                                    implicitHeight: 28
                                    x: lociSwitch.leftPadding
                                    y: parent.height / 2 - height / 2
                                    radius: Theme.radiusFull
                                    color: lociSwitch.checked ? Theme.primary : Theme.bgTertiary
                                    border.width: 1
                                    border.color: lociSwitch.checked ? Theme.primaryLight : Theme.border
                                    
                                    Rectangle {
                                        x: lociSwitch.checked ? parent.width - width - 4 : 4
                                        y: 4
                                        width: 20
                                        height: 20
                                        radius: 10
                                        color: Theme.textPrimary
                                        
                                        Behavior on x {
                                            NumberAnimation { duration: Theme.durationFast }
                                        }
                                    }
                                }
                            }
                            
                            Label {
                                text: lociSwitch.checked ? qsTr("已启用") : qsTr("已禁用")
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontS
                                font.weight: Theme.weightMedium
                                color: lociSwitch.checked ? Theme.primaryLight : Theme.textTertiary
                            }
                        }
                    }
                }
            }
            
            // TTS 设置
            GlassCard {
                Layout.fillWidth: true
                interactive: false
                elevated: true
                
                content: ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spacingM
                    
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingS
                        
                        Label {
                            text: "🔊"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                        }
                        
                        Label {
                            text: qsTr("语音合成 (TTS)")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }
                    }
                    
                    // 模型选择
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("模型")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: ttsModelCombo
                            Layout.fillWidth: true
                            model: ["Coqui TTS", "Bark", "VITS", "Edge TTS"]
                            currentIndex: 0
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: ttsModelCombo.displayText
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                    
                    // 输出设备
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("输出设备")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        ComboBox {
                            id: outputDeviceCombo
                            Layout.fillWidth: true
                            model: audioDeviceVM ? audioDeviceVM.getOutputDevices() : []
                            textRole: "name"
                            onActivated: {
                                if (audioDeviceVM && currentIndex >= 0 && currentIndex < model.length) {
                                    audioDeviceVM.selectOutput(String(model[currentIndex].id))
                                }
                            }
                            
                            background: Rectangle {
                                color: Theme.bgTertiary
                                radius: Theme.radiusMedium
                                border.width: 1
                                border.color: Theme.border
                            }
                            
                            contentItem: Text {
                                text: outputDeviceCombo.currentIndex >= 0 && outputDeviceCombo.model.length > outputDeviceCombo.currentIndex
                                      ? outputDeviceCombo.model[outputDeviceCombo.currentIndex].name
                                      : qsTr("请选择输出设备")
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontM
                                color: Theme.textPrimary
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 12
                            }
                        }
                    }
                    
                    // 音量
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL
                        
                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("音量")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }
                        
                        Slider {
                            id: volumeSlider
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            value: 80
                            
                            background: Rectangle {
                                x: volumeSlider.leftPadding
                                y: volumeSlider.topPadding + volumeSlider.availableHeight / 2 - height / 2
                                width: volumeSlider.availableWidth
                                height: 4
                                radius: 2
                                color: Theme.bgTertiary
                                
                                Rectangle {
                                    width: volumeSlider.visualPosition * parent.width
                                    height: parent.height
                                    radius: 2
                                    color: Theme.primary
                                }
                            }
                            
                            handle: Rectangle {
                                x: volumeSlider.leftPadding + volumeSlider.visualPosition * (volumeSlider.availableWidth - width)
                                y: volumeSlider.topPadding + volumeSlider.availableHeight / 2 - height / 2
                                width: 18
                                height: 18
                                radius: 9
                                color: Theme.textPrimary
                                border.width: 2
                                border.color: Theme.primary
                            }
                        }
                        
                        Label {
                            Layout.preferredWidth: 40
                            text: Math.round(volumeSlider.value) + "%"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                            horizontalAlignment: Text.AlignRight
                        }
                    }
                }
            }

            // 专业 I/O 控制（DAW 风格）
            GlassCard {
                Layout.fillWidth: true
                interactive: false
                elevated: true

                content: ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spacingM

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacingS

                        Label {
                            text: "🎛"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                        }

                        Label {
                            text: qsTr("音频 I/O 控制")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontL
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("输出模式")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        ComboBox {
                            id: outputModeCombo
                            Layout.fillWidth: true
                            model: [
                                { name: qsTr("虚拟声卡"), value: "virtual" },
                                { name: qsTr("指定设备"), value: "device" },
                                { name: qsTr("系统默认"), value: "system" }
                            ]
                            textRole: "name"
                            onActivated: {
                                if (audioDeviceVM && currentIndex >= 0 && currentIndex < model.length) {
                                    audioDeviceVM.outputMode = model[currentIndex].value
                                }
                            }
                            Component.onCompleted: {
                                if (!audioDeviceVM) return
                                for (var i = 0; i < model.length; ++i) {
                                    if (model[i].value === audioDeviceVM.outputMode) {
                                        currentIndex = i
                                        break
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("I/O 档位")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        ComboBox {
                            id: ioProfileCombo
                            Layout.fillWidth: true
                            model: [
                                { name: qsTr("实时低延迟"), value: "realtime" },
                                { name: qsTr("均衡"), value: "balanced" },
                                { name: qsTr("高稳定"), value: "studio" }
                            ]
                            textRole: "name"
                            onActivated: {
                                if (audioDeviceVM && currentIndex >= 0 && currentIndex < model.length) {
                                    audioDeviceVM.ioProfile = model[currentIndex].value
                                }
                            }
                            Component.onCompleted: {
                                if (!audioDeviceVM) return
                                for (var i = 0; i < model.length; ++i) {
                                    if (model[i].value === audioDeviceVM.ioProfile) {
                                        currentIndex = i
                                        break
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("缓冲")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        Slider {
                            id: ioBufferSlider
                            Layout.fillWidth: true
                            from: 20
                            to: 300
                            stepSize: 5
                            value: audioDeviceVM ? audioDeviceVM.ioBufferMs : 60
                            onMoved: {
                                if (audioDeviceVM) audioDeviceVM.ioBufferMs = Math.round(value)
                            }
                        }

                        Label {
                            Layout.preferredWidth: 52
                            text: Math.round(ioBufferSlider.value) + "ms"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                            horizontalAlignment: Text.AlignRight
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("输入增益")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        Slider {
                            id: inputGainSlider
                            Layout.fillWidth: true
                            from: -24
                            to: 24
                            stepSize: 0.5
                            value: audioDeviceVM ? audioDeviceVM.inputGainDb : 0
                            onMoved: {
                                if (audioDeviceVM) audioDeviceVM.inputGainDb = value
                            }
                        }

                        Label {
                            Layout.preferredWidth: 58
                            text: Number(inputGainSlider.value).toFixed(1) + " dB"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                            horizontalAlignment: Text.AlignRight
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("输出增益")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        Slider {
                            id: outputGainSlider
                            Layout.fillWidth: true
                            from: -24
                            to: 24
                            stepSize: 0.5
                            value: audioDeviceVM ? audioDeviceVM.outputGainDb : 0
                            onMoved: {
                                if (audioDeviceVM) audioDeviceVM.outputGainDb = value
                            }
                        }

                        Label {
                            Layout.preferredWidth: 58
                            text: Number(outputGainSlider.value).toFixed(1) + " dB"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textSecondary
                            horizontalAlignment: Text.AlignRight
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: root.compactForm ? Theme.spacingM : Theme.spacingXL

                        Label {
                            Layout.preferredWidth: root.compactForm ? 76 : 100
                            text: qsTr("监听")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontM
                            color: Theme.textSecondary
                        }

                        Switch {
                            id: monitorSwitch
                            checked: audioDeviceVM ? audioDeviceVM.monitoringEnabled : false
                            onToggled: {
                                if (audioDeviceVM) audioDeviceVM.monitoringEnabled = checked
                            }
                        }

                        Label {
                            text: monitorSwitch.checked ? qsTr("已启用") : qsTr("已禁用")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontS
                            color: Theme.textTertiary
                        }
                    }
                }
            }
            
            // 关于信息
            ColumnLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spacingL
                spacing: Theme.spacingS
                
                Label {
                    text: qsTr("LocalTrans v1.0.0")
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontS
                    color: Theme.textTertiary
                }
                
                Label {
                    text: qsTr("本地 AI 翻译工作台 · Powered by Loci")
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXS
                    color: Theme.textDisabled
                }
            }
            
            Item { Layout.preferredHeight: Theme.spacingXL }
        }
    }
}
