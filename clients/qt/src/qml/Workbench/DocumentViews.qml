pragma Singleton
pragma ComponentBehavior: Bound

import QtQuick

// The live documents of the app, each in its own web view. A view belongs to this object, so no
// canvas takes it along when the canvas goes. A view off the screen waits in the parking place
// with its place in the file and its undo, and it closes after five minutes there.
QtObject {
    id: root

    // An item of the window that never shows.
    property Item parking: null
    property var pages: ({})

    readonly property Component page: Component {
        CanvasDocument {
            id: document

            Timer {
                interval: 300000
                running: document.parent === root.parking
                onTriggered: root.close(document.sessionId, document.key)
            }
        }
    }

    function nameOf(sessionId, key) {
        return sessionId + "\n" + key;
    }
    // Shows the view of a file in `place`, and opens the file when no view has it.
    function show(sessionId, key, view, place) {
        const name = nameOf(sessionId, key);
        if (!pages[name])
            pages[name] = page.createObject(root, { sessionId: sessionId, key: key, view: view });
        const shown = pages[name];
        shown.parent = place;
        shown.anchors.fill = place;
    }
    // Takes the view of a file off the screen. It keeps its size there.
    function hide(sessionId, key) {
        const shown = pages[nameOf(sessionId, key)];
        if (!shown)
            return;
        shown.anchors.fill = undefined;
        shown.parent = parking;
    }
    // Gives `done` the words that a person selected in the view of a file, or none.
    function selection(sessionId, key, done) {
        const shown = pages[nameOf(sessionId, key)];
        if (shown)
            shown.selection(done);
        else
            done("");
    }
    function close(sessionId, key) {
        const name = nameOf(sessionId, key);
        const shown = pages[name];
        delete pages[name];
        if (shown)
            shown.destroy();
    }
}
