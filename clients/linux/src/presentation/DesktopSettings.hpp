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
    Q_PROPERTY(QString effectiveWorkingDirectory READ effectiveWorkingDirectory NOTIFY settingsChanged)
    Q_PROPERTY(QString settingsError READ settingsError NOTIFY settingsErrorChanged)
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
    QString settingsError() const { return m_error; }
    Q_INVOKABLE bool apply(
        const QString& family, int size, int cursor, double lineHeight, int scrollback, bool blink);
    Q_INVOKABLE void setWorkingDirectory(const QString& path);
    Q_INVOKABLE void resetTerminal();
    Q_INVOKABLE void clearError();
signals:
    void settingsChanged();
    void settingsErrorChanged();

private:
    std::unique_ptr<QSettings> m_settings;
    QString m_fontFamily = QStringLiteral("monospace"), m_directory, m_error;
    int m_fontSize = 13, m_scrollback = 10000;
    CursorStyle m_cursor = Block;
    double m_lineHeight = 1.0;
    bool m_blink = false;
    bool persist();
};
}
