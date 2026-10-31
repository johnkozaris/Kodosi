#pragma once

#include <QKeySequence>
#include <QList>
#include <QObject>
#include <QSettings>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

#include <memory>

namespace kodosi {

/// What a key can do in the app. The defaults are Ghostty's keys and Kodosi's own.
class ShortcutsModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap keys READ keys NOTIFY keysChanged)
    Q_PROPERTY(QVariantMap hints READ hints NOTIFY keysChanged)
    Q_PROPERTY(QVariantList rows READ rows NOTIFY keysChanged)
    Q_PROPERTY(bool changed READ changed NOTIFY keysChanged)

public:
    explicit ShortcutsModel(std::unique_ptr<QSettings> settings, QObject* parent = nullptr);

    [[nodiscard]] QVariantMap keys() const;
    [[nodiscard]] QVariantMap hints() const;
    [[nodiscard]] QVariantList rows() const;
    [[nodiscard]] bool changed() const;
    [[nodiscard]] QList<QKeySequence> taken() const;

    Q_INVOKABLE QString bind(const QString& action, const QString& sequence, const QString& old = {});
    Q_INVOKABLE void remove(const QString& action, const QString& sequence);
    Q_INVOKABLE void reset();
    Q_INVOKABLE [[nodiscard]] static QString sequence(int key, int modifiers);

signals:
    void keysChanged();

private:
    std::unique_ptr<QSettings> m_settings;

    [[nodiscard]] QStringList keysOf(const QString& action) const;
    void store(const QString& action, const QStringList& keys);
};

}
