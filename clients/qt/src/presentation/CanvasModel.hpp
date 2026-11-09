#pragma once

#include <QByteArray>
#include <QHash>
#include <QImage>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QPoint>
#include <QSet>
#include <QSizeF>
#include <QStringList>
#include <QUrl>
#include <QVariantMap>

#include <optional>
#include <utility>

namespace kodosi {
class CommandDispatcher;
class SessionCatalogModel;

// What this device knows of the canvases of the terminals: what a person saw and picked, and
// the content of the items that it showed.
class CanvasModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(int revision READ revision NOTIFY changed)
    Q_PROPERTY(bool browserKnown READ browserKnown NOTIFY browserChanged)
    Q_PROPERTY(QStringList browserSources READ browserSources NOTIFY browserChanged)
    Q_PROPERTY(int browserSites READ browserSites NOTIFY browserChanged)
    Q_PROPERTY(QString browserEngine READ browserEngine NOTIFY browserChanged)
    Q_PROPERTY(int browserGetting READ browserGetting NOTIFY browserChanged)
    Q_PROPERTY(QString browserWork READ browserWork NOTIFY browserChanged)
    Q_PROPERTY(QVariantMap browserDone READ browserDone NOTIFY browserChanged)
    Q_PROPERTY(QString browserProblem READ browserProblem NOTIFY browserChanged)

public:
    struct BrowserKey {
        QString code;
        QString named;
        int windows = 0;
    };

    CanvasModel(CommandDispatcher& commands, SessionCatalogModel& sessions, QObject* parent = nullptr);

    [[nodiscard]] int revision() const { return m_revision; }
    // What Settings › Browser shows: the last status of this computer, the part of a download of
    // Kodosi Browser from 0 to 100 or -1, the step that runs, by the source's name, "engine",
    // "forget" or "clear", and what each step did.
    [[nodiscard]] bool browserKnown() const { return m_browserKnown; }
    [[nodiscard]] QStringList browserSources() const { return m_browserSources; }
    [[nodiscard]] int browserSites() const { return m_browserSites; }
    [[nodiscard]] QString browserEngine() const { return m_browserEngine; }
    [[nodiscard]] int browserGetting() const { return m_browserGetting; }
    [[nodiscard]] QString browserWork() const { return m_browserWork; }
    [[nodiscard]] QVariantMap browserDone() const { return m_browserDone; }
    [[nodiscard]] QString browserProblem() const { return m_browserProblem; }
    // Starts the page afresh, with no notes from before and the status of now.
    Q_INVOKABLE void openBrowserSettings();
    // Asks for the status of now again, as while Kodosi Browser comes.
    Q_INVOKABLE void refreshBrowser();
    // Gets Kodosi Browser now, or its newest version.
    Q_INVOKABLE void getBrowser();
    Q_INVOKABLE void copySignIns(const QString& source);
    Q_INVOKABLE void forgetSignIns();
    Q_INVOKABLE void removeBrowsingData();
    void applyBrowser(const QJsonObject& event);
    Q_INVOKABLE [[nodiscard]] QString current(const QString& sessionId) const;
    Q_INVOKABLE void pick(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QStringList sections(const QString& sessionId) const;
    Q_INVOKABLE [[nodiscard]] QStringList closedSections(const QString& sessionId) const;
    Q_INVOKABLE void close(const QString& sessionId, const QString& section);
    Q_INVOKABLE void show(const QString& sessionId, const QString& section);
    Q_INVOKABLE [[nodiscard]] bool showsShelf(const QString& sessionId) const;
    Q_INVOKABLE void showShelf(const QString& sessionId, bool shown);
    Q_INVOKABLE void openBrowser(const QString& sessionId);
    Q_INVOKABLE [[nodiscard]] bool opensBrowser(const QString& sessionId) const;
    Q_INVOKABLE [[nodiscard]] bool hasNews(const QString& sessionId) const;
    Q_INVOKABLE [[nodiscard]] bool isNew(const QString& sessionId, const QString& key) const;
    Q_INVOKABLE void saw(const QString& sessionId, const QString& key);
    Q_INVOKABLE void remove(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString question(const QString& sessionId, const QString& key, const QString& words) const;
    Q_INVOKABLE void follow(const QString& sessionId, const QUrl& link);

    Q_INVOKABLE bool load(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString pageAddress(const QString& sessionId, const QString& key);
    Q_INVOKABLE [[nodiscard]] QString pictureAddress(const QString& sessionId, const QString& key);
    Q_INVOKABLE void saveCopy(const QString& sessionId, const QString& key);
    Q_INVOKABLE void attach(const QString& sessionId, const QUrl& file);
    void giveFiles(const QString& sessionId, const QList<QUrl>& files);
    [[nodiscard]] std::optional<std::pair<QByteArray, QByteArray>> published(const QUrl& address) const;
    [[nodiscard]] std::optional<QByteArray> published(int number) const;
    [[nodiscard]] std::optional<QByteArray> stored(const QString& sessionId, const QString& key) const;

    Q_INVOKABLE void act(const QString& sessionId, const QVariantMap& action);
    Q_INVOKABLE void wake(const QString& sessionId);

    void apply(const QJsonObject& event);
    void reset();

    [[nodiscard]] static std::optional<QPointF> browserPoint(QPointF point, QSizeF view, QSizeF browser);
    [[nodiscard]] static std::optional<QPointF> pointInView(QPointF point, QSizeF view, QSizeF browser);
    [[nodiscard]] static std::optional<BrowserKey> browserKey(int key);
    [[nodiscard]] static QString about(const QString& place, const QString& words);

signals:
    void changed();
    void browserChanged();
    void loaded(const QString& sessionId, const QString& key);
    void failed(const QString& sessionId, const QString& key, const QString& message);
    void copyReady(const QString& name, const QByteArray& content);
    void filesGiven(const QString& sessionId, const QStringList& paths);
    void problem(const QString& message);
    void linkQuestionRequested(const QString& sessionId, const QUrl& link, const QString& target);
    void linkRefused(const QString& sessionId, const QString& target, const QString& reason);

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
    struct Published {
        int number = 0;
        QString source;
        quint64 revision = 0;
        QByteArray type;
        QByteArray data;
    };
    struct Upload {
        QString sessionId;
        QString name;
        QByteArray data;
        qsizetype offset = 0;
        bool forPage = false;
    };
    struct Giving {
        qsizetype waiting = 0;
        QStringList paths;
    };
    enum class Asked { Read, Act, Remove, Attach };
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
    QHash<QString, QString> m_lastFiles;
    QHash<QString, QHash<QString, quint64>> m_closed;
    QSet<QString> m_opening;
    QSet<QString> m_shelves;
    QHash<QString, Stored> m_stored;
    QStringList m_storedOrder;
    QHash<QString, Reading> m_readings;
    QHash<QString, Request> m_requests;
    QHash<QString, Upload> m_uploads;
    QHash<QString, Giving> m_giving;
    bool m_browserKnown = false;
    QStringList m_browserSources;
    int m_browserSites = 0;
    QString m_browserEngine;
    int m_browserGetting = -1;
    // The engine before a person's check for a newer one, until the status after it.
    std::optional<QString> m_browserChecked;
    QString m_browserStatusRequest;
    QString m_browserStepRequest;
    QString m_browserWork;
    QVariantMap m_browserDone;
    QString m_browserProblem;
    QList<Published> m_published;
    int m_publishedCount = 0;

    [[nodiscard]] QList<Shown> shown(const QString& sessionId) const;
    [[nodiscard]] QStringList present(const QString& sessionId) const;
    [[nodiscard]] quint64 changedOf(const QString& sessionId, const QString& section) const;
    [[nodiscard]] static QString sectionOf(const QString& key);
    [[nodiscard]] QVariantMap item(const QString& sessionId, const QString& key) const;
    [[nodiscard]] int publish(const QString& sessionId, const QString& key);
    void observe();
    void touch();
    bool upload(const QString& sessionId, const QUrl& file, bool forPage);
    bool sendPart(const QString& upload);
    void gave(const QString& sessionId, const QString& path);
    void askBrowserStatus();
    void browserStep(const QString& key, const QString& type, QJsonObject fields = {});
    void read(const QString& sessionId, const QString& key, bool copy);
    void askPart(const QString& id);
    void receivePart(const Request& request, const QJsonObject& answer);
    void endReading(const QString& id, const QString& failure);
    void store(const QString& id, quint64 revision, QByteArray data);
    [[nodiscard]] QString ask(Asked asked, const QString& sessionId, QJsonObject request, const QString& key = {});
    [[nodiscard]] static QString storedId(const QString& sessionId, const QString& key);
};
}
