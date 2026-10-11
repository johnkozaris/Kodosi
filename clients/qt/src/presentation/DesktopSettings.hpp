#pragma once
#include <QObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QVector>
#include <memory>
namespace kodosi {
class DesktopSettings final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString fontFamily READ fontFamily NOTIFY settingsChanged)
    Q_PROPERTY(int fontSize READ fontSize NOTIFY settingsChanged)
    Q_PROPERTY(CursorStyle cursorStyle READ cursorStyle NOTIFY settingsChanged)
    Q_PROPERTY(double lineHeight READ lineHeight NOTIFY settingsChanged)
    Q_PROPERTY(int scrollbackLines READ scrollbackLines NOTIFY settingsChanged)
    Q_PROPERTY(bool cursorBlink READ cursorBlink NOTIFY settingsChanged)
    Q_PROPERTY(QString effectiveWorkingDirectory READ effectiveWorkingDirectory NOTIFY settingsChanged)
    Q_PROPERTY(QString branchFolder READ branchFolder NOTIFY settingsChanged)
    Q_PROPERTY(QString settingsError READ settingsError NOTIFY settingsErrorChanged)
    Q_PROPERTY(QVariantList startCommands READ startCommands NOTIFY startCommandsChanged)
    Q_PROPERTY(QStringList startCommandIds READ startCommandIds NOTIFY startCommandIdsChanged)
public:
    enum CursorStyle { Block, Bar, Underline };
    Q_ENUM(CursorStyle)
    explicit DesktopSettings(QObject* parent = nullptr);
    DesktopSettings(std::unique_ptr<QSettings> settings, QObject* parent = nullptr);
    QString fontFamily() const { return m_fontFamily; }
    int fontSize() const { return m_fontSize; }
    CursorStyle cursorStyle() const { return m_cursor; }
    double lineHeight() const { return m_lineHeight; }
    int scrollbackLines() const { return m_scrollback; }
    bool cursorBlink() const { return m_blink; }
    QString effectiveWorkingDirectory() const;
    QString branchFolder() const { return m_branchFolder; }
    QString settingsError() const { return m_error; }
    Q_INVOKABLE bool apply(
        const QString& family, int size, int cursor, double lineHeight, int scrollback, bool blink);
    Q_INVOKABLE void setWorkingDirectory(const QString& path);
    Q_INVOKABLE void setBranchFolder(const QString& path);
    Q_INVOKABLE void resetTerminal();
    Q_INVOKABLE void clearError();
    QVariantList startCommands() const;
    QStringList startCommandIds() const;
    Q_INVOKABLE void learn(const QString& program);
    Q_INVOKABLE void addStartCommand();
    Q_INVOKABLE void setStartCommand(const QString& id, const QString& name, const QString& command);
    Q_INVOKABLE void removeStartCommand(const QString& id);
    [[nodiscard]] static QString agentOf(const QString& command);
signals:
    void settingsChanged();
    void settingsErrorChanged();
    void startCommandsChanged();
    void startCommandIdsChanged();

private:
    std::unique_ptr<QSettings> m_settings;
    QString m_fontFamily = QStringLiteral("monospace"), m_directory, m_branchFolder, m_error;
    int m_fontSize = 13, m_scrollback = 10000;
    CursorStyle m_cursor = Block;
    double m_lineHeight = 1.0;
    bool m_blink = false;
    struct StartCommand {
        QString id, name, command;
    };
    QVector<StartCommand> m_start;
    QStringList m_learned;
    bool persist();
    void persistStart();
};
}
