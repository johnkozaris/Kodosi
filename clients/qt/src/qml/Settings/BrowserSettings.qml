pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// Copies the sign-ins of another browser of this computer into the shared browser. Each copy is
// a person's own step, and a button takes back what the copies brought.
ColumnLayout {
    id: root

    spacing: 14

    onVisibleChanged: {
        if (visible)
            Models.Canvas.askSignInSources();
    }

    PlainLabel {
        Layout.fillWidth: true
        text: qsTr("Sign-ins")
        font.weight: Font.DemiBold
    }
    PlainLabel {
        Layout.fillWidth: true
        text: qsTr("Copy the sign-ins of a browser of this computer, so that sites know you in your private terminals. Shared terminals never get them.")
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
    Repeater {
        model: Models.Canvas.signInSources

        RowLayout {
            id: source

            required property string modelData

            Layout.fillWidth: true

            PlainLabel {
                Layout.fillWidth: true
                text: source.modelData
                elide: Text.ElideRight
            }
            KButton {
                text: qsTr("Copy")
                enabled: !Models.Canvas.signInWork
                objectName: "settings.browser.copy." + source.modelData
                Accessible.id: objectName
                onClicked: Models.Canvas.copySignIns(source.modelData)
            }
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: Models.Canvas.signInSources.length === 0
        text: qsTr("This computer has no browser to copy from.")
        color: KodosiTheme.inkFaint
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 10

        KButton {
            text: qsTr("Remove copied sign-ins")
            enabled: !Models.Canvas.signInWork
            objectName: "settings.browser.forget"
            Accessible.id: objectName
            onClicked: Models.Canvas.forgetSignIns()
        }
        Spinner {
            size: 14
            visible: Models.Canvas.signInWork
        }
        PlainLabel {
            Layout.fillWidth: true
            text: Models.Canvas.signInNote
            color: KodosiTheme.inkMuted
            wrapMode: Text.WordWrap
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        text: qsTr("Passkeys and sign-ins bound to a device stay in their own browser. On Windows, Chrome keeps its sign-ins locked, so sign in on the canvas.")
        color: KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontCaption
        wrapMode: Text.WordWrap
    }
}
