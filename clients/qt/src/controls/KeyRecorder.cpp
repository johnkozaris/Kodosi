#include "controls/KeyRecorder.hpp"

#include "presentation/ShortcutsModel.hpp"

#include <QKeyEvent>

namespace kodosi {

KeyRecorder::KeyRecorder(QQuickItem* parent)
    : QQuickItem(parent)
{
}

bool KeyRecorder::event(QEvent* event)
{
    if (event->type() == QEvent::ShortcutOverride) {
        event->accept();
        return true;
    }
    return QQuickItem::event(event);
}

void KeyRecorder::keyPressEvent(QKeyEvent* event)
{
    event->accept();
    emit pressed(event->key(), ShortcutsModel::sequence(*event));
}

}
