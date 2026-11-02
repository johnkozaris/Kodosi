import Kodosi 1.0
import QtQuick
import QtWebChannel
import QtWebEngine
import Kodosi.Models 1.0 as Models

// A live Word, Excel, PowerPoint or PDF file. The view bundle of the app draws it in a closed
// frame, and its only way out is the pipe of the document.
WebEngineView {
    id: root

    required property string sessionId
    required property string key
    required property string view
    property string pipe: ""
    property bool shown: false
    property int wait: 1000

    readonly property string host: `
        new QWebChannel(qt.webChannelTransport, ({ objects: { page } }) => {
          const host = window.documentView.hosted(
            { kind: page.view, theme: page.look, ready: () => page.ready() },
            (frame) => page.sent(frame),
          );
          page.received.connect(host.deliver);
          page.attached.connect(host.attach);
          page.detached.connect(host.detach);
          page.lookChanged.connect(() => host.restyle(page.look));
          page.started();
        });`

    function join() {
        root.pipe = Models.CanvasDocuments.open(root.sessionId, root.key);
        if (root.pipe.length === 0)
            again.restart();
    }

    url: "kodosi-document://view/index.html"
    profile: Models.CanvasWeb.profile
    backgroundColor: KodosiTheme.surface
    webChannel: channel
    userScripts.collection: [{
        name: "channel",
        sourceUrl: "qrc:///qtwebchannel/qwebchannel.js",
        injectionPoint: WebEngineScript.DocumentCreation,
        worldId: WebEngineScript.MainWorld
    }]
    settings.javascriptCanOpenWindows: false
    settings.localContentCanAccessFileUrls: false
    settings.localContentCanAccessRemoteUrls: false
    settings.errorPageEnabled: false
    settings.focusOnNavigationEnabled: false
    settings.navigateOnDropEnabled: false
    settings.screenCaptureEnabled: false
    settings.webRTCPublicInterfacesOnly: true

    Component.onDestruction: Models.CanvasDocuments.close(root.pipe)
    onLoadingChanged: info => {
        if (info.status === WebEngineLoadingInfo.LoadSucceededStatus)
            runJavaScript(root.host, WebEngineScript.MainWorld);
    }
    onNavigationRequested: request => {
        if (request.url.toString() === root.url.toString())
            return;
        request.reject();
        if (request.navigationType === WebEngineNavigationRequest.LinkClickedNavigation)
            Models.Canvas.follow(root.sessionId, request.url);
    }
    onNewWindowRequested: request => {
        if (request.userInitiated)
            Models.Canvas.follow(root.sessionId, request.requestedUrl);
    }
    onRenderProcessTerminated: {
        root.shown = false;
        Models.CanvasDocuments.close(root.pipe);
        root.pipe = "";
        root.reload();
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

    WebChannel {
        id: channel

        registeredObjects: [bridge]
    }
    QtObject {
        id: bridge

        readonly property string view: root.view
        readonly property var look: ({
                "surface": String(KodosiTheme.surface),
                "surface-behind": String(KodosiTheme.well),
                "raised": String(KodosiTheme.raised),
                "lifted": String(KodosiTheme.lifted),
                "well": String(KodosiTheme.well),
                "ink": String(KodosiTheme.ink),
                "ink-soft": String(KodosiTheme.inkMuted),
                "ink-faint": String(KodosiTheme.inkFaint),
                "line": String(KodosiTheme.hairline),
                "accent": String(KodosiTheme.accentStrong),
                "scheme": KodosiTheme.isDark ? "dark" : "light"
            })

        signal received(string frame)
        signal attached()
        signal detached()

        function sent(frame) {
            Models.CanvasDocuments.send(root.pipe, frame);
        }
        function ready() {
            root.shown = true;
        }
        function started() {
            root.join();
        }

        WebChannel.id: "page"
    }
    Connections {
        function onOpened(pipe) {
            if (pipe === root.pipe) {
                root.wait = 1000;
                bridge.attached();
            }
        }
        function onReceived(pipe, frame) {
            if (pipe === root.pipe)
                bridge.received(frame);
        }
        function onLost(pipe) {
            if (pipe === root.pipe) {
                root.pipe = "";
                bridge.detached();
                again.restart();
            }
        }

        target: Models.CanvasDocuments
    }
    Timer {
        id: again

        interval: root.wait
        onTriggered: {
            root.wait = Math.min(root.wait * 2, 30000);
            root.join();
        }
    }
    Spinner {
        anchors.centerIn: parent
        size: 18
        visible: !root.shown
    }
}
