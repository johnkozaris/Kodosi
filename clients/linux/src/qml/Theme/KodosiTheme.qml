pragma Singleton

import QtQuick
import Kodosi.Models 1.0 as Models

QtObject {
    readonly property bool isDark: Models.Appearance.dark
    readonly property bool reduceMotion: Models.Appearance.reduceMotion

    readonly property color darkCanvas: "#100d0b"
    readonly property color darkSurface: "#1d1714"
    readonly property color darkSurfaceRaised: "#241c18"
    readonly property color darkSurfaceElevated: "#2b211c"
    readonly property color darkSurfaceSelected: "#3a281f"
    readonly property color darkTerminal: "#0a0807"
    readonly property color darkSeam: "#40332c"
    readonly property color darkSeamStrong: "#57443a"
    readonly property color darkTextPrimary: "#f1e9e3"
    readonly property color darkTextSecondary: "#ad9a8e"
    readonly property color darkTextTertiary: "#9c887e"
    readonly property color darkAccent: "#db8a62"
    readonly property color darkAccentHover: "#e69a72"
    readonly property color darkAccentPressed: "#c97752"
    readonly property color darkAccentForeground: "#160e0a"
    readonly property color darkDanger: "#e47773"
    readonly property color darkFocusRing: "#ef9d75"
    readonly property color darkOverlayDim: "#100d0b"
    readonly property color darkShadow: "#b8000000"
    readonly property color darkInput: "#17120f"
    readonly property color darkPlaceholderText: "#948178"
    readonly property color darkDisabled: "#5f5149"

    readonly property color lightCanvas: "#f9f4eb"
    readonly property color lightSurface: "#efe3d2"
    readonly property color lightSurfaceRaised: "#ead8c3"
    readonly property color lightSurfaceElevated: "#f2e8d9"
    readonly property color lightSurfaceSelected: "#dec9b5"
    readonly property color lightTerminal: "#e7e3de"
    readonly property color lightSeam: "#d6bea4"
    readonly property color lightSeamStrong: "#b99a79"
    readonly property color lightTextPrimary: "#25160e"
    readonly property color lightTextSecondary: "#5b4233"
    readonly property color lightTextTertiary: "#705445"
    readonly property color lightAccent: "#943a10"
    readonly property color lightAccentHover: "#9b3f12"
    readonly property color lightAccentPressed: "#87350e"
    readonly property color lightAccentForeground: "#fcf9f3"
    readonly property color lightDanger: "#a52525"
    readonly property color lightFocusRing: "#944014"
    readonly property color lightOverlayDim: "#efe6dc"
    readonly property color lightShadow: "#421f1208"
    readonly property color lightInput: "#fffaf2"
    readonly property color lightPlaceholderText: "#6f5141"
    readonly property color lightDisabled: "#8c7668"

    readonly property color canvas: isDark ? darkCanvas : lightCanvas
    readonly property color surface: isDark ? darkSurface : lightSurface
    readonly property color surfaceRaised: isDark
        ? darkSurfaceRaised
        : lightSurfaceRaised
    readonly property color surfaceElevated: isDark
        ? darkSurfaceElevated
        : lightSurfaceElevated
    readonly property color surfaceSelected: isDark
        ? darkSurfaceSelected
        : lightSurfaceSelected
    readonly property color terminal: isDark ? darkTerminal : lightTerminal
    readonly property color seam: isDark ? darkSeam : lightSeam
    readonly property color seamStrong: isDark
        ? darkSeamStrong
        : lightSeamStrong
    readonly property color textPrimary: isDark
        ? darkTextPrimary
        : lightTextPrimary
    readonly property color textSecondary: isDark
        ? darkTextSecondary
        : lightTextSecondary
    readonly property color textTertiary: isDark
        ? darkTextTertiary
        : lightTextTertiary
    readonly property color accent: isDark ? darkAccent : lightAccent
    readonly property color accentHover: isDark
        ? darkAccentHover
        : lightAccentHover
    readonly property color accentPressed: isDark
        ? darkAccentPressed
        : lightAccentPressed
    readonly property color accentForeground: isDark
        ? darkAccentForeground
        : lightAccentForeground
    readonly property color danger: isDark ? darkDanger : lightDanger
    readonly property color focusRing: isDark
        ? darkFocusRing
        : lightFocusRing
    readonly property color overlayDim: isDark
        ? darkOverlayDim
        : lightOverlayDim
    readonly property color shadow: isDark ? darkShadow : lightShadow
    readonly property color input: isDark ? darkInput : lightInput
    readonly property color placeholderText: isDark
        ? darkPlaceholderText
        : lightPlaceholderText
    readonly property color disabled: isDark ? darkDisabled : lightDisabled

    readonly property int fontBody: 13

    readonly property int spacing2: 6
    readonly property int spacing3: 8
    readonly property int spacing4: 10
    readonly property int spacing5: 12

    readonly property real radiusSmall: 8
    readonly property real radiusLarge: 12
    readonly property real radiusModal: 16
    readonly property real controlHeight: 34
    readonly property real compactControlHeight: 30
    readonly property real iconButtonSize: 32

    readonly property int motionFastAuthored: 150
    readonly property int motionSpinnerAuthored: 900
    readonly property int motionFast: reduceMotion ? 0 : motionFastAuthored
    readonly property int motionSpinner: reduceMotion
        ? 0
        : motionSpinnerAuthored
}
