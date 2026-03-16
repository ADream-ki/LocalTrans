pragma Singleton
import QtQuick

QtObject {
    // Light base
    readonly property color bgPrimary: "#FFFFFF"
    readonly property color bgSecondary: "#F7F9FC"
    readonly property color bgTertiary: "#EEF2F7"
    readonly property color bgElevated: "#FFFFFF"
    readonly property color bgHover: "#E8EEF8"

    // Brand and status
    readonly property color primary: "#2563EB"
    readonly property color primaryLight: "#1D4ED8"
    readonly property color primaryDark: "#1E40AF"
    readonly property color primaryBg: "#EAF1FF"
    readonly property color accent: "#0EA5E9"
    readonly property color accentLight: "#0284C7"
    readonly property color accentBg: "#E0F2FE"
    readonly property color success: "#10B981"
    readonly property color successBg: "#ECFDF5"
    readonly property color warning: "#F59E0B"
    readonly property color warningBg: "#FFFBEB"
    readonly property color error: "#EF4444"
    readonly property color errorBg: "#FEF2F2"

    // Text and border
    readonly property color textPrimary: "#1E293B"
    readonly property color textSecondary: "#64748B"
    readonly property color textTertiary: "#94A3B8"
    readonly property color textDisabled: "#CBD5E1"
    readonly property color textInverse: "#FFFFFF"
    readonly property color border: "#E2E8F0"
    readonly property color borderLight: "#CBD5E1"
    readonly property color borderFocus: "#2563EB"
    readonly property color divider: "#E2E8F0"
    readonly property color glassBg: "#F4FFFFFF"
    readonly property color glassBorder: "#66E2E8F0"

    // Geometry
    readonly property real radiusXS: 2
    readonly property real radiusSmall: 6
    readonly property real radiusMedium: 8
    readonly property real radiusLarge: 12
    readonly property real radiusXLarge: 16
    readonly property real radiusFull: 9999

    readonly property real spacingXS: 4
    readonly property real spacingS: 8
    readonly property real spacingM: 12
    readonly property real spacingL: 16
    readonly property real spacingXL: 24
    readonly property real spacingXXL: 32

    // Typography
    readonly property real fontXS: 11
    readonly property real fontS: 12
    readonly property real fontM: 14
    readonly property real fontL: 16
    readonly property real fontXL: 18
    readonly property real fontXXL: 22
    readonly property int weightNormal: Font.Normal
    readonly property int weightMedium: Font.Medium
    readonly property int weightSemibold: Font.DemiBold
    readonly property int weightBold: Font.Bold
    readonly property real lineHeightNormal: 1.45
    readonly property real lineHeightRelaxed: 1.7
    readonly property string fontFamily: "'Segoe UI Variable', 'Microsoft YaHei UI', 'Noto Sans SC', sans-serif"
    readonly property string fontFamilyMono: "'Cascadia Code', 'JetBrains Mono', Consolas, monospace"

    // Motion
    readonly property int durationInstant: 60
    readonly property int durationFast: 120
    readonly property int durationNormal: 220
    readonly property int durationSlow: 320

    function withAlpha(color, alpha) {
        return Qt.rgba(color.r, color.g, color.b, alpha)
    }
}
