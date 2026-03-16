// GlassCard.qml - 毛玻璃卡片组件 (优化版)
import QtQuick
import QtQuick.Controls

import "../theme"

Rectangle {
    id: root
    
    // 公开属性
    property alias content: contentLoader.sourceComponent
    property bool elevated: false
    property bool interactive: true
    
    // 内部状态
    property bool hovered: interactive && mouseArea.containsMouse
    
    implicitWidth: 200
    implicitHeight: 100
    radius: Theme.radiusLarge
    
    // 背景渐变
    gradient: Gradient {
        GradientStop { 
            position: 0.0
            color: elevated ? Theme.bgElevated : Theme.bgSecondary
        }
        GradientStop { 
            position: 1.0
            color: elevated ? Theme.bgTertiary : Theme.bgTertiary
        }
    }
    
    // 毛玻璃效果层
    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: Theme.glassBg
        opacity: 0.6
    }
    
    // 边框
    border.width: 1
    border.color: root.hovered ? Theme.borderLight : Theme.border
    
    Behavior on border.color { 
        ColorAnimation { duration: Theme.durationFast } 
    }
    
    // 悬停发光效果
    Rectangle {
        anchors.fill: parent
        anchors.margins: -1
        radius: parent.radius + 1
        color: "transparent"
        border.width: 2
        border.color: Theme.primary
        opacity: root.hovered ? 0.3 : 0
        
        Behavior on opacity {
            NumberAnimation { duration: Theme.durationNormal }
        }
    }
    
    // 轻量外晕（兼容 Qt6，无需 GraphicalEffects）
    Rectangle {
        anchors.fill: parent
        anchors.margins: -6
        radius: parent.radius + 6
        color: Theme.primary
        opacity: root.hovered ? 0.06 : 0
        z: -1

        Behavior on opacity {
            NumberAnimation { duration: Theme.durationNormal }
        }
    }
    
    // 内容加载器
    Loader {
        id: contentLoader
        anchors.fill: parent
        anchors.margins: Theme.spacingL
    }
    
    // 鼠标交互
    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: root.interactive
        cursorShape: root.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
        acceptedButtons: Qt.NoButton
    }
    
    // 状态变化动画
    states: [
        State {
            name: "hovered"
            when: root.hovered
            PropertyChanges {
                target: root
                scale: 1.01
            }
        }
    ]
    
    transitions: [
        Transition {
            NumberAnimation {
                properties: "scale"
                duration: Theme.durationNormal
                easing.type: Easing.OutCubic
            }
        }
    ]
}
