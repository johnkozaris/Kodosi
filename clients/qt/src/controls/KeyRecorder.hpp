#pragma once

#include <QQuickItem>
#include <QString>
#include <QtQml/qqmlregistration.h>

namespace kodosi {

class KeyRecorder : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT

public:
    explicit KeyRecorder(QQuickItem* parent = nullptr);

signals:
    void pressed(int key, const QString& sequence);

protected:
    bool event(QEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
};

}
