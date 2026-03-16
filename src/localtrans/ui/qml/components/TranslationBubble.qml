// TranslationBubble.qml - 翻译气泡组件 (优化版)
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../theme"

Rectangle {
    id: root
    
    // 数据属性
    property string sourceText: ""
    property string translatedText: ""
    property string sourceLang: "EN"
    property string targetLang: "ZH"
    property bool lociEnhanced: false
    
    implicitWidth: 400
    implicitHeight: layout.implicitHeight + Theme.spacingL * 2
    radius: Theme.radiusLarge
    color: Theme.bgTertiary
    
    // 边框
    border.width: 1
    border.color: Theme.border
    
    ColumnLayout {
        id: layout
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Theme.spacingL
        spacing: Theme.spacingM
        
        // 源文本区域
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingS
            
            // 语言标签 + Loci 增强指示器
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingS
                
                Rectangle {
                    width: langLabel.width + 12
                    height: 20
                    radius: Theme.radiusSmall
                    color: Theme.bgSecondary
                    
                    Label {
                        id: langLabel
                        anchors.centerIn: parent
                        text: sourceLang.toUpperCase()
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontXS
                        font.weight: Theme.weightSemibold
                        color: Theme.textSecondary
                    }
                }
                
                // Loci 增强指示器
                Rectangle {
                    visible: lociEnhanced
                    width: lociBadge.width + 12
                    height: 20
                    radius: Theme.radiusSmall
                    
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: Theme.primary }
                        GradientStop { position: 1.0; color: Theme.accent }
                    }
                    
                    Row {
                        id: lociBadge
                        anchors.centerIn: parent
                        spacing: 4
                        
                        Label {
                            text: "✦"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontXS
                            color: Theme.textPrimary
                        }
                        
                        Label {
                            text: "Loci"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontXS
                            font.weight: Theme.weightSemibold
                            color: Theme.textPrimary
                        }
                    }
                }
                
                Item { Layout.fillWidth: true }
            }
            
            // 源文本
            Label {
                Layout.fillWidth: true
                text: sourceText || qsTr("等待语音输入...")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontL
                font.weight: Theme.weightNormal
                color: sourceText ? Theme.textPrimary : Theme.textTertiary
                wrapMode: Text.WordWrap
                lineHeight: Theme.lineHeightNormal
                lineHeightMode: Text.ProportionalHeight
            }
        }
        
        // 分隔线
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            Layout.topMargin: Theme.spacingXS
            Layout.bottomMargin: Theme.spacingXS
            color: Theme.divider
        }
        
        // 翻译文本区域
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingS
            
            // 目标语言标签
            Rectangle {
                width: targetLangLabel.width + 12
                height: 20
                radius: Theme.radiusSmall
                color: Theme.primaryBg
                
                Label {
                    id: targetLangLabel
                    anchors.centerIn: parent
                    text: targetLang.toUpperCase()
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXS
                    font.weight: Theme.weightSemibold
                    color: Theme.primaryLight
                }
            }
            
            // 翻译文本
            Label {
                Layout.fillWidth: true
                text: translatedText || qsTr("翻译结果将显示在这里")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontXL
                font.weight: Theme.weightMedium
                color: translatedText ? Theme.textPrimary : Theme.textTertiary
                wrapMode: Text.WordWrap
                lineHeight: Theme.lineHeightRelaxed
                lineHeightMode: Text.ProportionalHeight
            }
        }
    }
    
    // 悬停效果
    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: "transparent"
        border.width: 2
        border.color: Theme.accent
        opacity: hoverArea.containsMouse ? 0.2 : 0
        
        Behavior on opacity {
            NumberAnimation { duration: Theme.durationFast }
        }
    }
    
    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
    }
}