#pragma once

#include <QByteArray>
#include <QHash>
#include <QImage>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QPoint>
#include <QSizeF>
#include <QStringList>
#include <QUrl>
#include <QVariantMap>

#include <optional>
#include <utility>

namespace kodosi {
class CommandDispatcher;
class SessionCatalogModel;

// What this device knows of the canvases of the terminals: what a person saw and picked,
// the content of the items that it showed, and the pictures of and input to each shared
// browser.
class CanvasModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(int revision READ revision NOTIFY changed)

public:
    struct BrowserKey {
        QString code;
        QString named;
        int windows = 0;
    };

    CanvasModel(CommandDispatcher& commands, SessionCatalogModel& sessions, QObject* parent = nullptr);

    [[nodiscard]] int revision() const { return m_revision; }
    Q_INVOKABLE [[nodiscard]] QString current(const QString& sessionId) const;
    Q_INVOKABLE void pick(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] bool hasNews(const QString& sessionId) const;
    Q_INVOKABLE [[nodiscard]] bool isNew(const QString& sessionId, const QString& key) const;
    Q_INVOKABLE void saw(const QString& sessionId, const QString& key);
    Q_INVOKABLE void remove(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString question(const QString& sessionId, const QString& key, const QString& words) const;
    Q_INVOKABLE bool follow(const QUrl& link, bool otherComputer);

    Q_INVOKABLE bool load(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString pageAddress(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString pictureAddress(const QString& sessionId, const QString& key);
    Q_INVOKABLE void saveCopy(const QString& sessionId, const QString& key);
    [[nodiscard]] std::optional<std::pair<QByteArray, QByteArray>> published(const QUrl& address) const;
    [[nodiscard]] std::optional<QByteArray> published(int number) const;
    [[nodiscard]] std::optional<QByteArray> stored(const QString& sessionId, const QString& key) const;

    Q_INVOKABLE void watch(const QString& sessionId);
    Q_INVOKABLE void unwatch(const QString& sessionId);
    Q_INVOKABLE void setBrowserView(const QString& sessionId, qreal width, qreal height);
    Q_INVOKABLE void act(const QString& sessionId, const QVariantMap& action);
    Q_INVOKABLE void wake(const QString& sessionId);
    Q_INVOKABLE [[nodiscard]] QVariantMap picture(const QString& sessionId) const;
    Q_INVOKABLE void pointer(const QString& sessionId, const QString& type, qreal x, qreal y, int button, int clicks,
        int modifiers);
    Q_INVOKABLE void wheel(const QString& sessionId, qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers);
    Q_INVOKABLE bool key(const QString& sessionId, int key, const QString& text, int modifiers, bool down);
    Q_INVOKABLE void type(const QString& sessionId, const QString& text);
    Q_INVOKABLE void paste(const QString& sessionId);
    [[nodiscard]] QImage browserImage(const QString& sessionId) const;

    void apply(const QJsonObject& event);
    void reset();

    [[nodiscard]] static std::optional<QPoint> browserPoint(QPointF point, QSizeF view, QSizeF picture, QSizeF browser);
    [[nodiscard]] static std::optional<BrowserKey> browserKey(int key);
    [[nodiscard]] static QString about(const QString& place, const QString& words);
    [[nodiscard]] static bool followable(const QUrl& link, bool otherComputer);

signals:
    void changed();
    void loaded(const QString& sessionId, const QString& key);
    void failed(const QString& sessionId, const QString& key, const QString& message);
    void pictureChanged(const QString& sessionId);
    void copyReady(const QString& name, const QByteArray& content);
    void problem(const QString& message);

private:
    struct Shown {
        QString key;
        quint64 changed = 0;
    };
    struct Reading {
        QString sessionId;
        QString key;
        quint64 revision = 0;
        QByteArray data;
        std::optional<quint64> total;
        int rounds = 0;
        bool copy = false;
    };
    struct Stored {
        quint64 revision = 0;
        QByteArray data;
    };
    struct Feed {
        int watchers = 0;
        bool asking = false;
        bool sending = false;
        quint64 sequence = 0;
        QImage image;
        QSizeF browser;
        QSizeF view;
        QString failure;
        QList<QJsonObject> pending;
    };
    struct Published {
        int number = 0;
        QString source;
        quint64 revision = 0;
        QByteArray type;
        QByteArray data;
    };
    enum class Asked { Read, Picture, Input, Act, Remove };
    struct Request {
        Asked asked;
        QString sessionId;
        QString key;
        qint64 sent = 0;
    };

    CommandDispatcher& m_commands;
    SessionCatalogModel& m_sessions;
    int m_revision = 0;
    QHash<QString, QHash<QString, quint64>> m_marks;
    QHash<QString, quint64> m_firstSeen;
    QHash<QString, quint64> m_sequences;
    QHash<QString, QString> m_picks;
    QHash<QString, Stored> m_stored;
    QStringList m_storedOrder;
    QHash<QString, Reading> m_readings;
    QHash<QString, Request> m_requests;
    QHash<QString, Feed> m_feeds;
    QList<Published> m_published;
    int m_publishedCount = 0;

    [[nodiscard]] QList<Shown> shown(const QString& sessionId) const;
    [[nodiscard]] QVariantMap item(const QString& sessionId, const QString& key) const;
    [[nodiscard]] int publish(const QString& sessionId, const QString& key);
    void observe();
    void touch();
    void read(const QString& sessionId, const QString& key, bool copy);
    void askPart(const QString& id);
    void receivePart(const Request& request, const QJsonObject& answer);
    void endReading(const QString& id, const QString& failure);
    void store(const QString& id, quint64 revision, QByteArray data);
    void askPicture(const QString& sessionId);
    void receivePicture(const QString& sessionId, const QJsonObject& answer, qint64 sent);
    void retryPicture(const QString& sessionId, qint64 sent);
    void send(const QString& sessionId, QJsonObject input);
    void flush(const QString& sessionId);
    void press(const QString& sessionId, const BrowserKey& key, const QString& typed, bool down, int modifiers);
    [[nodiscard]] QString ask(Asked asked, const QString& sessionId, QJsonObject request, const QString& key = {});
    void failBrowser(const QString& sessionId, const QString& message);
    [[nodiscard]] std::optional<QPoint> point(const QString& sessionId, qreal x, qreal y) const;
    [[nodiscard]] static QString storedId(const QString& sessionId, const QString& key);
    [[nodiscard]] static int browserModifiers(int modifiers);
};
}
