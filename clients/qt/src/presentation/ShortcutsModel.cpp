#include "presentation/ShortcutsModel.hpp"

#include <algorithm>
#include <ranges>
#include <utility>

namespace kodosi {
namespace {

struct Action {
    QString id;
    int group = 0;
    const char* title = nullptr;
    QStringList keys;
};

const auto numbered = QStringLiteral("numberedTerminal");

const QList<Action>& actions()
{
    using namespace Qt::StringLiterals;
    static const QList<Action> list {
        { u"newTerminal"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "New terminal"), { u"Ctrl+Shift+N"_s } },
        { u"quickTerminal"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Quick terminal"),
            { u"Ctrl+Shift+T"_s, u"Ctrl+Shift+O"_s, u"Ctrl+Shift+E"_s } },
        { u"resume"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Resume a conversation"), { u"Ctrl+Shift+R"_s } },
        { u"close"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Close terminal"), { u"Ctrl+Shift+W"_s } },
        { u"minimize"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Minimize"), { u"Ctrl+Shift+M"_s } },
        { u"zoom"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Zoom"),
            { u"Ctrl+Shift+F"_s, u"Ctrl+Shift+Return"_s } },
        { u"details"_s, 0, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Details"), { u"Ctrl+Shift+I"_s } },
        { u"previousTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Previous terminal"),
            { u"Ctrl+Shift+["_s, u"Ctrl+Shift+Left"_s, u"Ctrl+PgUp"_s, u"Ctrl+Shift+Tab"_s } },
        { u"nextTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Next terminal"),
            { u"Ctrl+Shift+]"_s, u"Ctrl+Shift+Right"_s, u"Ctrl+PgDown"_s, u"Ctrl+Tab"_s } },
        { u"leftTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Terminal on the left"), { u"Ctrl+Alt+Left"_s } },
        { u"rightTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Terminal on the right"), { u"Ctrl+Alt+Right"_s } },
        { u"upTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Terminal above"), { u"Ctrl+Alt+Up"_s } },
        { u"downTerminal"_s, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Terminal below"), { u"Ctrl+Alt+Down"_s } },
        { numbered, 1, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Terminal 1 to 9"), { u"Alt+1"_s } },
        { u"goTo"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Go to"), { u"Ctrl+Shift+K"_s } },
        { u"sidebar"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Show or hide the sidebar"), { u"Ctrl+Shift+B"_s } },
        { u"dosi"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Dosi"), { u"Ctrl+Shift+J"_s } },
        { u"stopDosi"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Stop Dosi"), { u"Ctrl+Shift+."_s } },
        { u"settings"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Settings"), { u"Ctrl+Shift+,"_s } },
        { u"fullScreen"_s, 2, QT_TRANSLATE_NOOP("kodosi::ShortcutsModel", "Full screen"), { u"F11"_s } },
    };
    return list;
}

const Action* find(const QString& id)
{
    const auto found = std::ranges::find(actions(), id, &Action::id);
    return found == actions().end() ? nullptr : &*found;
}

QString settingsKey(const QString& action)
{
    return QStringLiteral("shortcuts/") + action;
}

QString nativeText(const QString& sequence)
{
    return QKeySequence(sequence).toString(QKeySequence::NativeText);
}

}

ShortcutsModel::ShortcutsModel(std::unique_ptr<QSettings> settings, QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
{
}

QStringList ShortcutsModel::keysOf(const QString& action) const
{
    const auto key = settingsKey(action);
    if (m_settings->contains(key))
        return m_settings->value(key).toStringList();
    const auto* known = find(action);
    return known == nullptr ? QStringList() : known->keys;
}

void ShortcutsModel::store(const QString& action, const QStringList& keys)
{
    const auto* known = find(action);
    if (known != nullptr && known->keys == keys)
        m_settings->remove(settingsKey(action));
    else
        m_settings->setValue(settingsKey(action), keys);
}

/// Many layouts send Ctrl+{ for Ctrl+Shift+[, so a shortcut lists both.
QVariantMap ShortcutsModel::keys() const
{
    QVariantMap map;
    for (const auto& action : actions()) {
        auto keys = keysOf(action.id);
        if (keys.contains(QStringLiteral("Ctrl+Shift+[")))
            keys.append(QStringLiteral("Ctrl+{"));
        if (keys.contains(QStringLiteral("Ctrl+Shift+]")))
            keys.append(QStringLiteral("Ctrl+}"));
        map.insert(action.id, keys);
    }
    return map;
}

QVariantMap ShortcutsModel::hints() const
{
    QVariantMap map;
    for (const auto& action : actions()) {
        const auto keys = keysOf(action.id);
        map.insert(action.id, keys.isEmpty() ? QString() : nativeText(keys.first()));
    }
    return map;
}

QVariantList ShortcutsModel::rows() const
{
    QVariantList rows;
    for (const auto& action : actions()) {
        QVariantList keys;
        for (const auto& key : keysOf(action.id)) {
            const auto label = action.id == numbered ? nativeText(key) + QStringLiteral("–9") : nativeText(key);
            keys.append(QVariantMap { { QStringLiteral("sequence"), key }, { QStringLiteral("label"), label } });
        }
        rows.append(QVariantMap { { QStringLiteral("action"), action.id }, { QStringLiteral("group"), action.group },
            { QStringLiteral("title"), tr(action.title) }, { QStringLiteral("keys"), keys } });
    }
    return rows;
}

bool ShortcutsModel::changed() const
{
    return std::ranges::any_of(actions(), [this](const Action& action) { return m_settings->contains(settingsKey(action.id)); });
}

/// Every key that the programs in terminals do not get.
QList<QKeySequence> ShortcutsModel::taken() const
{
    QList<QKeySequence> taken;
    for (const auto& action : actions()) {
        for (const auto& key : keysOf(action.id)) {
            if (action.id != numbered) {
                taken.append(QKeySequence(key));
                continue;
            }
            for (auto digit = 1; digit <= 9; ++digit)
                taken.append(QKeySequence(key.chopped(1) + QString::number(digit)));
        }
    }
    return taken;
}

/// Gives a key to an action, in place of `old`. Returns the title of the action that had it.
QString ShortcutsModel::bind(const QString& action, const QString& sequence, const QString& old)
{
    if (find(action) == nullptr || sequence.isEmpty())
        return {};
    const auto chord = action == numbered ? sequence.chopped(1) + QLatin1Char('1') : sequence;
    QString holder;
    for (const auto& other : actions()) {
        auto keys = keysOf(other.id);
        if (other.id != action && keys.removeAll(chord) > 0) {
            store(other.id, keys);
            holder = tr(other.title);
        }
    }
    auto keys = keysOf(action);
    if (const auto at = keys.indexOf(old); !old.isEmpty() && at >= 0)
        keys[at] = chord;
    else
        keys.append(chord);
    keys.removeDuplicates();
    store(action, keys);
    emit keysChanged();
    return holder;
}

void ShortcutsModel::remove(const QString& action, const QString& sequence)
{
    auto keys = keysOf(action);
    if (keys.removeAll(sequence) > 0) {
        store(action, keys);
        emit keysChanged();
    }
}

void ShortcutsModel::reset()
{
    for (const auto& action : actions())
        m_settings->remove(settingsKey(action.id));
    emit keysChanged();
}

/// The text of a pressed key, or nothing for a key that a program should get.
QString ShortcutsModel::sequence(const int key, const int modifiers)
{
    switch (key) {
    case Qt::Key_Control:
    case Qt::Key_Shift:
    case Qt::Key_Alt:
    case Qt::Key_AltGr:
    case Qt::Key_Meta:
    case Qt::Key_Super_L:
    case Qt::Key_Super_R:
    case Qt::Key_unknown:
        return {};
    default:
        break;
    }
    const auto flags = Qt::KeyboardModifiers(modifiers)
        & (Qt::ControlModifier | Qt::AltModifier | Qt::ShiftModifier | Qt::MetaModifier);
    const auto function = key >= Qt::Key_F1 && key <= Qt::Key_F35;
    if (!function && (flags & ~Qt::KeyboardModifiers(Qt::ShiftModifier)) == Qt::NoModifier)
        return {};
    return QKeySequence(QKeyCombination(flags, Qt::Key(key))).toString(QKeySequence::PortableText);
}

}
