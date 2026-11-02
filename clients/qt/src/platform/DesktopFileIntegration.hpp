#pragma once
#include <QObject>
#include <QPointer>
#include <QString>
#include <QUrl>
#include <functional>
class QFileDialog;
class QWindow;
namespace kodosi {
class DesktopFileIntegration final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QString errorMessage READ errorMessage NOTIFY errorChanged)
public:
    using UrlOpener = std::function<bool(const QUrl&)>;
    explicit DesktopFileIntegration(QObject* parent = nullptr);
    DesktopFileIntegration(UrlOpener opener, QObject* parent = nullptr);
    ~DesktopFileIntegration() override;
    bool busy() const;
    QString errorMessage() const { return m_error; }
    Q_INVOKABLE bool requestDirectory(const QString& purpose, const QString& initialDirectory = {});
    Q_INVOKABLE void cancelDirectory();
    Q_INVOKABLE bool requestFiles(const QString& purpose);
    Q_INVOKABLE bool requestPageFiles(const QString& sessionId, bool several);
    Q_INVOKABLE bool openPath(const QString& path);
    Q_INVOKABLE bool openWebUrl(const QString& url);
    Q_INVOKABLE void copyText(const QString& text);
    void saveCopy(const QString& name, const QByteArray& content);
    Q_INVOKABLE void clearError();
    void setTransientParent(QWindow* window);
    [[nodiscard]] static QString canonicalLocalPath(const QString& path, bool directoryOnly = false);
signals:
    void busyChanged();
    void errorChanged();
    void directoryPicked(QString purpose, QString path);
    void filesPicked(QString purpose, QList<QUrl> files);
    void pageFilesPicked(QString sessionId, QList<QUrl> files);

private:
    QPointer<QFileDialog> m_dialog;
    QPointer<QWindow> m_window;
    UrlOpener m_opener;
    QString m_error;
    void fail(QString error);
    bool pickFiles(const QString& title, bool several, std::function<void(const QList<QUrl>&)> done);
};
}
