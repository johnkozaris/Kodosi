import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

// The body of a terminal and its canvas: two lobes of one surface in the terminal's color with
// a soft waist at `seam`, a header bar on each lobe, and the neck up to `tab`, the tab that the
// terminal flows out of. With no depth, the waist and the gap between the bars close.
Item {
    id: root

    property real seam: width
    property real depth: 0
    property real radius: KodosiTheme.radiusTile
    property rect tab: Qt.rect(0, 0, 0, 0)
    readonly property real band: 38
    readonly property real waist: 14
    readonly property real part: Math.max(0, Math.min(1, depth / waist))
    readonly property real gap: 10 * part
    readonly property real end: KodosiTheme.radiusXs * part
    readonly property real reach: 20
    readonly property real r: Math.max(0, Math.min(radius, width / 2, height / 2))
    readonly property real dipTop: dip(depth, reach)
    readonly property real dipBottom: dip(depth / waist * 5, 12)
    readonly property bool necked: tab.width > tab.height && tab.y + tab.height < 0
    readonly property real neckMiddle: tab.x + tab.width / 2
    readonly property real neckTop: (tab.width - tab.height) / 2
    readonly property real neckNarrow: Math.min(neckTop, Math.max(10, tab.width * 0.28), Math.max(4, neckMiddle - r),
        Math.max(4, width - r - neckMiddle))
    readonly property real neckFlare: Math.min(10, -(tab.y + tab.height) + 4)
    readonly property real flareLeft: Math.min(neckFlare, Math.max(0, neckMiddle - neckNarrow - r))
    readonly property real flareRight: Math.min(neckFlare, Math.max(0, width - r - neckMiddle - neckNarrow))
    readonly property color face: KodosiTheme.terminal
    readonly property color bandColor: KodosiTheme.terminalBandSelected

    function dip(value: real, wide: real): real {
        const edge = Math.min(seam, width - seam);
        return value * Math.max(0, Math.min(1, (edge - wide) / (r + wide)));
    }
    function n(value: real): string {
        return value.toFixed(2);
    }
    function corner(sx: real, sy: real, ex: real, ey: real, ax: real, ay: real): string {
        const pull = 0.55;
        return " C " + n(sx + (ax - sx) * pull) + " " + n(sy + (ay - sy) * pull) + " " + n(ex + (ax - ex) * pull) + " "
            + n(ey + (ay - ey) * pull) + " " + n(ex) + " " + n(ey);
    }
    function valley(y: real, value: real, wide: real): string {
        const x = seam;
        return " L " + n(x - wide) + " " + n(y) + " C " + n(x - wide / 2) + " " + n(y) + " " + n(x - wide / 2) + " "
            + n(y + value) + " " + n(x) + " " + n(y + value) + " C " + n(x + wide / 2) + " " + n(y + value) + " "
            + n(x + wide / 2) + " " + n(y) + " " + n(x + wide) + " " + n(y);
    }
    // The top edge from `from` to `to` (left to right), with the waist when it lies between them.
    function topEdge(from: real, to: real): string {
        if (dipTop > 0.5 && seam - reach >= from && seam + reach <= to)
            return valley(0, dipTop, reach) + " L " + n(to) + " 0";
        return " L " + n(to) + " 0";
    }
    // From the top edge at `start` around the right side, the bottom and the left side, back to
    // the top edge at `finish`.
    function around(start: real, finish: real): string {
        const w = width, h = height;
        let path = topEdge(start, w - r) + corner(w - r, 0, w, r, w, 0) + " L " + n(w) + " " + n(h - r)
            + corner(w, h - r, w - r, h, w, h);
        if (dipBottom > 0.5)
            path += " L " + n(seam + 12) + " " + n(h) + " C " + n(seam + 6) + " " + n(h) + " " + n(seam + 6) + " "
                + n(h - dipBottom) + " " + n(seam) + " " + n(h - dipBottom) + " C " + n(seam - 6) + " " + n(h - dipBottom) + " "
                + n(seam - 6) + " " + n(h) + " " + n(seam - 12) + " " + n(h);
        path += " L " + n(r) + " " + n(h) + corner(r, h, 0, h - r, 0, h) + " L 0 " + n(r) + corner(0, r, r, 0, 0, 0);
        return path + topEdge(r, finish);
    }
    function lobes(): string {
        return "M " + n(r) + " 0" + around(r, r) + " Z";
    }
    // The point of the left half of the waist's top curve where it reaches `x`.
    function onValley(x: real): var {
        const p0 = [seam - reach, 0], p1 = [seam - reach / 2, 0], p2 = [seam - reach / 2, dipTop], p3 = [seam, dipTop];
        const at = t => {
            const u = 1 - t;
            return [u * u * u * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t * t * t * p3[0],
                u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1]];
        };
        let low = 0, high = 1;
        for (let step = 0; step < 24; ++step) {
            const middle = (low + high) / 2;
            if (at(middle)[0] < x)
                low = middle;
            else
                high = middle;
        }
        const t = (low + high) / 2;
        const lerp = (a, b) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        const q0 = lerp(p0, p1), q1 = lerp(p1, p2), q2 = lerp(p2, p3), s0 = lerp(q0, q1), s1 = lerp(q1, q2);
        return { c1: q0, c2: s0, point: lerp(s0, s1) };
    }
    // The header bar of the left lobe, or of the whole body with no waist.
    function leftBand(): string {
        const w = width;
        if (seam <= 0.5 || seam >= w - 0.5)
            return "M 0 " + n(band) + " L 0 " + n(r) + corner(0, r, r, 0, 0, 0) + " L " + n(w - r) + " 0"
                + corner(w - r, 0, w, r, w, 0) + " L " + n(w) + " " + n(band) + " Z";
        const edge = seam - gap / 2;
        let path = "M 0 " + n(band) + " L 0 " + n(r) + corner(0, r, r, 0, 0, 0);
        if (dipTop > 0.5 && seam - reach < edge) {
            const cut = onValley(edge);
            path += " L " + n(seam - reach) + " 0 C " + n(cut.c1[0]) + " " + n(cut.c1[1]) + " " + n(cut.c2[0]) + " "
                + n(cut.c2[1]) + " " + n(edge) + " " + n(cut.point[1]);
        } else {
            path += " L " + n(edge) + " 0";
        }
        return path + " L " + n(edge) + " " + n(band - end) + " Q " + n(edge) + " " + n(band) + " " + n(edge - end) + " "
            + n(band) + " Z";
    }
    // The header bar of the right lobe, the mirror of the left one at the waist.
    function rightBand(): string {
        const w = width;
        if (seam <= 0.5 || seam >= w - 0.5)
            return "";
        const edge = seam + gap / 2;
        let path = "M " + n(w) + " " + n(band) + " L " + n(w) + " " + n(r) + corner(w, r, w - r, 0, w, 0);
        if (dipTop > 0.5 && seam + reach > edge) {
            const cut = onValley(2 * seam - edge);
            path += " L " + n(seam + reach) + " 0 C " + n(2 * seam - cut.c1[0]) + " " + n(cut.c1[1]) + " "
                + n(2 * seam - cut.c2[0]) + " " + n(cut.c2[1]) + " " + n(edge) + " " + n(cut.point[1]);
        } else {
            path += " L " + n(edge) + " 0";
        }
        return path + " L " + n(edge) + " " + n(band - end) + " Q " + n(edge) + " " + n(band) + " " + n(edge + end) + " "
            + n(band) + " Z";
    }
    // One side of the neck, from the tab down to the body's top edge.
    function neckSide(sign: real): string {
        const m = neckMiddle, t = neckTop, w = neckNarrow, f = sign < 0 ? flareLeft : flareRight;
        const low = tab.y + tab.height, gap = -low, waist = low + gap / 2;
        return "M " + n(m + sign * t) + " " + n(low) + " C " + n(m + sign * (t - (t - w) * 0.6)) + " " + n(low) + " "
            + n(m + sign * w) + " " + n(waist - gap * 0.3) + " " + n(m + sign * w) + " " + n(waist) + " C "
            + n(m + sign * w) + " " + n(waist + gap * 0.3) + " " + n(m + sign * (w + f * 0.4)) + " 0 "
            + n(m + sign * (w + f)) + " 0";
    }
    function neck(): string {
        if (!necked)
            return "";
        const m = neckMiddle, t = neckTop, w = neckNarrow, fl = flareLeft, fr = flareRight;
        const low = tab.y + tab.height, gap = -low, waist = low + gap / 2;
        return "M " + n(m - t) + " " + n(low - 0.5) + " L " + n(m - t) + " " + n(low) + " C " + n(m - t + (t - w) * 0.6) + " "
            + n(low) + " " + n(m - w) + " " + n(waist - gap * 0.3) + " " + n(m - w) + " " + n(waist) + " C " + n(m - w) + " "
            + n(waist + gap * 0.3) + " " + n(m - w - fl * 0.4) + " 0 " + n(m - w - fl) + " 0 L " + n(m - w - fl) + " 1 L "
            + n(m + w + fr) + " 1 L " + n(m + w + fr) + " 0 C " + n(m + w + fr * 0.4) + " 0 " + n(m + w) + " " + n(waist + gap * 0.3)
            + " " + n(m + w) + " " + n(waist) + " C " + n(m + w) + " " + n(waist - gap * 0.3) + " " + n(m + t - (t - w) * 0.6) + " "
            + n(low) + " " + n(m + t) + " " + n(low) + " L " + n(m + t) + " " + n(low - 0.5) + " Z";
    }
    // The hairline around the body and the neck, open where the neck meets the body.
    function outline(): string {
        if (!necked)
            return lobes();
        const left = neckMiddle - neckNarrow - flareLeft, right = neckMiddle + neckNarrow + flareRight;
        return "M " + n(right) + " 0" + around(right, left) + " " + neckSide(-1) + " " + neckSide(1);
    }

    Accessible.ignored: true

    Shape {
        x: 3
        y: 6
        width: root.width
        height: root.height
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: "transparent"
            fillColor: KodosiTheme.alpha(KodosiTheme.shadow, KodosiTheme.isDark ? 0.16 : 0.05)

            PathSvg { path: root.width > 0 ? root.lobes() : "" }
        }
    }
    Shape {
        x: 1.5
        y: 3
        width: root.width
        height: root.height
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: "transparent"
            fillColor: KodosiTheme.alpha(KodosiTheme.shadow, KodosiTheme.isDark ? 0.22 : 0.07)

            PathSvg { path: root.width > 0 ? root.lobes() : "" }
        }
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: "transparent"
            fillColor: root.face

            PathSvg { path: root.width > 0 ? root.lobes() : "" }
        }
        ShapePath {
            strokeColor: "transparent"
            fillColor: root.bandColor

            PathSvg { path: root.width > 0 ? root.leftBand() : "" }
        }
        ShapePath {
            strokeColor: "transparent"
            fillColor: root.bandColor

            PathSvg { path: root.width > 0 ? root.rightBand() : "" }
        }
        ShapePath {
            strokeColor: "transparent"
            fillColor: root.bandColor

            PathSvg { path: root.neck() }
        }
        ShapePath {
            strokeColor: KodosiTheme.alpha(KodosiTheme.terminalHairline, 0.8)
            strokeWidth: 1
            fillColor: "transparent"

            PathSvg { path: root.width > 0 ? root.outline() : "" }
        }
    }
}
