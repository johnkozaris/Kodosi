#pragma once
#include <QObject>
#include <QSettings>
#include <QString>
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
    Q_PROPERTY(QString homeDirectory READ homeDirectory CONSTANT)
    Q_PROPERTY(QString effectiveWorkingDirectory READ effectiveWorkingDirectory NOTIFY settingsChanged)
    Q_PROPERTY(QString branchFolder READ branchFolder NOTIFY settingsChanged)
    Q_PROPERTY(QString lastComputer READ lastComputer WRITE setLastComputer NOTIFY settingsChanged)
    Q_PROPERTY(QString settingsError READ settingsError NOTIFY settingsErrorChanged)
public:
    enum CursorStyle { Block, Bar, Underline };
    Q_ENUM(CursorStyle)
    explicit DesktopSettings(std::unique_ptr<QSettings> settings, QObject* parent = nullptr);
    QString fontFamily() const { return m_fontFamily; }
    int fontSize() const { return m_fontSize; }
    CursorStyle cursorStyle() const { return m_cursor; }
    double lineHeight() const { return m_lineHeight; }
    int scrollbackLines() const { return m_scrollback; }
    bool cursorBlink() const { return m_blink; }
    QString homeDirectory() const;
    QString effectiveWorkingDirectory() const;
    QString branchFolder() const { return m_branchFolder; }
    QString lastComputer() const { return m_computer; }
    QString settingsError() const { return m_error; }
    Q_INVOKABLE bool apply(
        const QString& family, int size, int cursor, double lineHeight, int scrollback, bool blink);
    void setWorkingDirectory(const QString& path);
    void setLastComputer(const QString& computer);
    Q_INVOKABLE void setBranchFolder(const QString& path);
    Q_INVOKABLE void resetTerminal();
    Q_INVOKABLE [[nodiscard]] static QString agentOf(const QString& text);
    Q_INVOKABLE [[nodiscard]] static QString agentLabel(const QString& text);
signals:
    void settingsChanged();
    void settingsErrorChanged();

private:
    std::unique_ptr<QSettings> m_settings;
    QString m_fontFamily = QStringLiteral("monospace"), m_directory, m_branchFolder, m_computer, m_error;
    int m_fontSize = 13, m_scrollback = 10000;
    CursorStyle m_cursor = Block;
    double m_lineHeight = 1.0;
    bool m_blink = false;
    bool persist();
    void clearError();
};
}
