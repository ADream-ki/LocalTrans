import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import "../components"
import "../theme"

Item {
    id: root

    property var diagnosticsData: ({
        os: "Windows",
        pythonVersion: "3.11",
        appVersion: "1.0.0",
        lociStatus: "未加载",
        lociVersion: "-",
        lociGpuSupport: "-",
        cpuDevice: "-",
        cudaDevice: "-",
        metalDevice: "-",
        inputDevice: "-",
        outputDevice: "-",
        virtualDevice: "-"
    })

    function refreshDiagnostics() {
        if (platformVM) {
            var data = platformVM.getDiagnostics()
            if (data)
                diagnosticsData = data
        }
    }

    Component.onCompleted: refreshDiagnostics()

    Rectangle {
        anchors.fill: parent
        color: Theme.bgPrimary
    }

    ScrollView {
        anchors.fill: parent
        anchors.margins: root.width < 1000 ? Theme.spacingM : Theme.spacingXL
        clip: true
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

        ColumnLayout {
            width: parent.availableWidth
            spacing: Theme.spacingL

            RowLayout {
                Layout.fillWidth: true

                Label {
                    text: qsTr("系统诊断")
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontXXL
                    font.weight: Theme.weightBold
                    color: Theme.textPrimary
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: qsTr("刷新")
                    onClicked: refreshDiagnostics()
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

            DiagnosticsGroup {
                Layout.fillWidth: true
                title: qsTr("环境信息")
                items: [
                    { label: qsTr("操作系统"), value: diagnosticsData.os },
                    { label: qsTr("Python 版本"), value: diagnosticsData.pythonVersion },
                    { label: qsTr("应用版本"), value: diagnosticsData.appVersion }
                ]
            }

            DiagnosticsGroup {
                Layout.fillWidth: true
                title: qsTr("Loci 引擎")
                items: [
                    { label: qsTr("状态"), value: diagnosticsData.lociStatus },
                    { label: qsTr("版本"), value: diagnosticsData.lociVersion },
                    { label: qsTr("GPU 支持"), value: diagnosticsData.lociGpuSupport }
                ]
            }

            DiagnosticsGroup {
                Layout.fillWidth: true
                title: qsTr("计算设备")
                items: [
                    { label: qsTr("CPU"), value: diagnosticsData.cpuDevice },
                    { label: qsTr("CUDA"), value: diagnosticsData.cudaDevice },
                    { label: qsTr("Metal"), value: diagnosticsData.metalDevice }
                ]
            }

            DiagnosticsGroup {
                Layout.fillWidth: true
                title: qsTr("音频设备")
                items: [
                    { label: qsTr("输入设备"), value: diagnosticsData.inputDevice },
                    { label: qsTr("输出设备"), value: diagnosticsData.outputDevice },
                    { label: qsTr("虚拟设备"), value: diagnosticsData.virtualDevice }
                ]
            }
        }
    }
}

