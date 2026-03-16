// CustomTitleBar.qml - 自定义标题栏 (优化版)
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"

Rectangle {
    id: root
    height: 52
    color: "transparent"
    property bool compactLayout: width < 1180
    
    // 信号
    signal minimizeClicked()
    signal maximizeClicked()
    signal closeClicked()
    
    // 状态
    property bool isRunning: sessionVM ? sessionVM.isRunning : false
    property string currentState: sessionVM ? sessionVM.state : "idle"
    
    // 背景渐变
    gradient: Gradient {
        orientation: Gradient.Horizontal
        GradientStop { position: 0.0; color: Theme.bgPrimary }
        GradientStop { position: 0.5; color: Theme.bgSecondary }
        GradientStop { position: 1.0; color: Theme.bgPrimary }
    }
    
    // 底部边框
    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: 1
        color: Theme.divider
    }
    
    // 拖拽区域
    MouseArea {
        anchors.fill: parent
        property point lastPos: Qt.point(0, 0)
        
        onPressed: lastPos = Qt.point(mouseX, mouseY)
        onPositionChanged: {
            if (pressed) {
                var dx = mouseX - lastPos.x
                var dy = mouseY - lastPos.y
                var window = root.Window.window
                window.x += dx
                window.y += dy
            }
        }
        onDoubleClicked: root.maximizeClicked()
    }
    
    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: root.compactLayout ? Theme.spacingM : Theme.spacingL
        anchors.rightMargin: root.compactLayout ? Theme.spacingM : Theme.spacingL
        spacing: root.compactLayout ? Theme.spacingM : Theme.spacingL
        
        // Logo 和标题
        RowLayout {
            spacing: Theme.spacingM
            
            // Logo 图标
            Rectangle {
                width: 36
                height: 36
                radius: Theme.radiusMedium
                
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: Theme.primary }
                    GradientStop { position: 1.0; color: Theme.accent }
                }
                
                Label {
                    anchors.centerIn: parent
                    text: "LT"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontS
                    font.weight: Theme.weightBold
                    color: Theme.textInverse
                }
            }
            
            ColumnLayout {
                spacing: 1
                
                Label {
                    text: "LocalTrans"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXL
                    font.weight: Theme.weightBold
                    color: Theme.textPrimary
                }
                
                Label {
                    text: qsTr("本地 AI 翻译工作台")
                    visible: root.width >= 1080
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXS
                    font.weight: Theme.weightNormal
                    color: Theme.textTertiary
                }
            }
        }
        
        Item { Layout.fillWidth: true }
        
        // 语言选择和控制
        RowLayout {
            visible: !root.compactLayout
            spacing: Theme.spacingM
            
            // 源语言
            LanguageSelector {
                id: sourceLangSelector
                currentLang: settingsVM ? settingsVM.sourceLang : "en"
                onLangChanged: function(lang) {
                    if (settingsVM) settingsVM.sourceLang = lang
                }
            }
            
            // 交换按钮
            Rectangle {
                width: 40
                height: 36
                radius: Theme.radiusMedium
                color: swapHover.containsMouse ? Theme.bgHover : "transparent"
                
                Label {
                    anchors.centerIn: parent
                    text: "⇄"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXL
                    font.weight: Theme.weightSemibold
                    color: swapHover.containsMouse ? Theme.primaryLight : Theme.textSecondary
                }
                
                MouseArea {
                    id: swapHover
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (sessionVM) sessionVM.swapLanguages()
                    }
                }
            }
            
            // 目标语言
            LanguageSelector {
                id: targetLangSelector
                currentLang: settingsVM ? settingsVM.targetLang : "zh"
                onLangChanged: function(lang) {
                    if (settingsVM) settingsVM.targetLang = lang
                }
            }
        }

        Rectangle {
            visible: root.compactLayout
            height: 30
            width: compactLang.width + 14
            radius: Theme.radiusFull
            color: Theme.bgTertiary
            border.width: 1
            border.color: Theme.border

            Label {
                id: compactLang
                anchors.centerIn: parent
                text: (settingsVM ? settingsVM.sourceLang.toUpperCase() : "EN") + " → " +
                      (settingsVM ? settingsVM.targetLang.toUpperCase() : "ZH")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontXS
                color: Theme.textSecondary
            }
        }
        
        Item { Layout.preferredWidth: Theme.spacingXL; visible: !root.compactLayout }
        
        // 状态指示器和控制按钮
        RowLayout {
            spacing: Theme.spacingM
            
            // 运行状态指示器
            Rectangle {
                visible: root.width >= 1320
                width: statusRow.width + 16
                height: 30
                radius: Theme.radiusFull
                color: Theme.bgTertiary
                
                Row {
                    id: statusRow
                    anchors.centerIn: parent
                    spacing: 8
                    
                    // 状态点
                    Rectangle {
                        width: 10
                        height: 10
                        radius: 5
                        anchors.verticalCenter: parent.verticalCenter
                        color: Theme.getStateColor(root.currentState)
                        
                        // 运行时闪烁
                        SequentialAnimation on opacity {
                            running: root.isRunning
                            loops: Animation.Infinite
                            NumberAnimation { to: 0.4; duration: 600 }
                            NumberAnimation { to: 1.0; duration: 600 }
                        }
                    }
                    
                    Label {
                        text: root.isRunning ? qsTr("运行中") : qsTr("就绪")
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontS
                        font.weight: Theme.weightMedium
                        color: Theme.textSecondary
                    }
                }
            }
            
            // 开始/停止按钮
            Rectangle {
                width: startBtnRow.width + 24
                height: 36
                radius: Theme.radiusMedium
                
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: root.isRunning ? Theme.error : Theme.primary }
                    GradientStop { position: 1.0; color: root.isRunning ? "#DC2626" : Theme.primaryDark }
                }
                
                // 悬停效果
                Rectangle {
                    anchors.fill: parent
                    radius: parent.radius
                    color: "white"
                    opacity: startBtnHover.containsMouse ? 0.15 : 0
                }
                
                Row {
                    id: startBtnRow
                    anchors.centerIn: parent
                    spacing: 8
                    
                    Label {
                        text: root.isRunning ? "⏹" : "▶"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontS
                        color: Theme.textPrimary
                    }
                    
                    Label {
                        text: root.isRunning ? qsTr("停止") : qsTr("开始")
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontS
                        font.weight: Theme.weightSemibold
                        color: Theme.textPrimary
                    }
                }
                
                MouseArea {
                    id: startBtnHover
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (sessionVM) {
                            root.isRunning ? sessionVM.stopSession() : sessionVM.startSession()
                        }
                    }
                }
            }
        }
        
        Item { Layout.preferredWidth: Theme.spacingL; visible: !root.compactLayout }
        
        // 窗口控制按钮
        RowLayout {
            spacing: 2
            
            // 最小化
            WindowButton {
                icon: "─"
                onClicked: root.minimizeClicked()
            }
            
            // 最大化
            WindowButton {
                icon: "□"
                onClicked: root.maximizeClicked()
            }
            
            // 关闭
            WindowButton {
                icon: "✕"
                isClose: true
                onClicked: root.closeClicked()
            }
        }
    }
    
    // 语言选择器组件
    component LanguageSelector: Rectangle {
        property string currentLang: "en"
        signal langChanged(string lang)
        
        width: langRow.width + 20
        height: 36
        radius: Theme.radiusMedium
        color: langSelectorHover.containsMouse ? Theme.bgHover : Theme.bgTertiary
        border.width: 1
        border.color: langSelectorHover.containsMouse ? Theme.borderLight : Theme.border
        
        Behavior on border.color { ColorAnimation { duration: Theme.durationFast } }
        
        Row {
            id: langRow
            anchors.centerIn: parent
            spacing: 6
            
            Label {
                text: currentLang.toUpperCase()
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontS
                font.weight: Theme.weightSemibold
                color: Theme.textPrimary
            }
            
            Label {
                text: "▼"
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontXS
                color: Theme.textTertiary
            }
        }
        
        MouseArea {
            id: langSelectorHover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: langMenu.open()
        }
        
        // 语言菜单
        Menu {
            id: langMenu
            y: parent.height + 6
            
            background: Rectangle {
                implicitWidth: 140
                color: Theme.bgElevated
                radius: Theme.radiusMedium
                border.width: 1
                border.color: Theme.border
            }
            
            MenuItem {
                text: "English (EN)"
                onTriggered: langChanged("en")
            }
            MenuItem {
                text: "中文 (ZH)"
                onTriggered: langChanged("zh")
            }
            MenuItem {
                text: "日本語 (JA)"
                onTriggered: langChanged("ja")
            }
            MenuItem {
                text: "한국어 (KO)"
                onTriggered: langChanged("ko")
            }
        }
    }
    
    // 窗口按钮组件
    component WindowButton: Rectangle {
        property string icon: ""
        property bool isClose: false
        
        signal clicked()
        
        width: 44
        height: 36
        radius: Theme.radiusSmall
        color: btnHover.containsMouse ? 
               (isClose ? Theme.error : Theme.bgHover) : 
               "transparent"
        
        Label {
            anchors.centerIn: parent
            text: icon
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontM
            font.weight: Theme.weightMedium
            color: isClose && btnHover.containsMouse ? Theme.textPrimary : Theme.textSecondary
        }
        
        MouseArea {
            id: btnHover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }
}
