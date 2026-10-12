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
    Q_INVOKABLE bool openPath(const QString& path);
    Q_INVOKABLE bool openWebUrl(const QString& url);
    Q_INVOKABLE void clearError();
    void setTransientParent(QWindow* window);
    [[nodiscard]] static QString canonicalLocalPath(const QString& path, bool directoryOnly = false);
signals:
    void busyChanged();
    void errorChanged();
    void directoryPicked(QString purpose, QString path);
    void directoryPickCancelled(QString purpose);

private:
    QPointer<QFileDialog> m_dialog;
    QPointer<QWindow> m_window;
    UrlOpener m_opener;
    QString m_error;
    void fail(QString error);
};
}
