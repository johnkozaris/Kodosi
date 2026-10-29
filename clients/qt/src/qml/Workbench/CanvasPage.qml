import Kodosi 1.0
import QtQuick
import QtWebEngine
import Kodosi.Models 1.0 as Models

// A page of a canvas in a closed frame. It runs its own scripts, but it reaches no network,
// no files and no storage that stays. A web or mail link opens on this device.
WebEngineView {
    id: root

    required property string address
    property bool otherComputer: false
    property var place: null
    property string shown: ""

    // Calls `done` with the words that the person selected on the page.
    function selection(done) {
        runJavaScript("getSelection().toString()", WebEngineScript.ApplicationWorld, result => done(String(result || "")));
    }
    function show() {
        if (root.address.length === 0 || root.address === root.shown)
            return;
        if (root.shown.length === 0) {
            root.shown = root.address;
            root.url = root.address;
            return;
        }
        runJavaScript("[scrollX, scrollY]", WebEngineScript.ApplicationWorld, position => {
            root.place = position;
            root.shown = root.address;
            root.url = root.address;
        });
    }

    profile: Models.CanvasWeb.profile
    backgroundColor: KodosiTheme.sheet
    settings.javascriptCanOpenWindows: false
    settings.javascriptCanAccessClipboard: false
    settings.localContentCanAccessFileUrls: false
    settings.localContentCanAccessRemoteUrls: false
    settings.pluginsEnabled: true
    settings.pdfViewerEnabled: true
    settings.errorPageEnabled: false
    settings.focusOnNavigationEnabled: false
    settings.navigateOnDropEnabled: false
    settings.screenCaptureEnabled: false
    settings.playbackRequiresUserGesture: true
    settings.webRTCPublicInterfacesOnly: true

    Component.onCompleted: show()
    onAddressChanged: show()
    onNavigationRequested: request => {
        const target = request.url.toString();
        const page = target.split("#")[0] === root.shown;
        const empty = target === "about:blank" || target === "about:srcdoc";
        if (page || (empty && !request.isMainFrame))
            return;
        request.reject();
        if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation)
            Models.Canvas.follow(request.url, root.otherComputer);
    }
    onNewWindowRequested: request => {
        if (request.userInitiated)
            Models.Canvas.follow(request.requestedUrl, root.otherComputer);
    }
    onLoadingChanged: info => {
        if (info.status !== WebEngineLoadingInfo.LoadSucceededStatus || !root.place)
            return;
        runJavaScript("scrollTo(" + Number(root.place[0]) + ", " + Number(root.place[1]) + ")", WebEngineScript.ApplicationWorld);
        root.place = null;
    }
    onContextMenuRequested: request => request.accepted = true
    onJavaScriptConsoleMessage: () => {}
    onPermissionRequested: permission => permission.deny()
    onFileDialogRequested: request => {
        request.accepted = true;
        request.dialogReject();
    }
    onJavaScriptDialogRequested: request => {
        request.accepted = true;
        request.dialogReject();
    }
    onColorDialogRequested: request => {
        request.accepted = true;
        request.dialogReject();
    }
}
