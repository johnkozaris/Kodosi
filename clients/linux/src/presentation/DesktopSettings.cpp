#include "presentation/DesktopSettings.hpp"
#include <QDir>
#include <QFileInfo>
#include <algorithm>
#include <cmath>
namespace kodosi {
DesktopSettings::DesktopSettings(QObject* parent)
    : DesktopSettings(std::make_unique<QSettings>(), parent)
{
}
DesktopSettings::DesktopSettings(std::unique_ptr<QSettings> settings, QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
{
    const auto family
        = m_settings->value(QStringLiteral("terminal/fontFamily"), QStringLiteral("monospace")).toString();
    if (!family.trimmed().isEmpty() && family.size() <= 256 && !family.contains(QChar::Null))
        m_fontFamily = family;
    m_fontSize = std::clamp(m_settings->value(QStringLiteral("terminal/fontSize"), 13).toInt(), 8, 32);
    m_scrollback
        = std::clamp(m_settings->value(QStringLiteral("terminal/scrollback"), 10000).toInt(), 100, 100000);
    m_cursor = static_cast<CursorStyle>(
        std::clamp(m_settings->value(QStringLiteral("terminal/cursor"), 0).toInt(), 0, 2));
    const auto line = m_settings->value(QStringLiteral("terminal/lineHeight"), 1.0).toDouble();
    m_lineHeight = std::isfinite(line) ? std::clamp(line, 0.8, 2.0) : 1.0;
    m_blink = m_settings->value(QStringLiteral("terminal/blink"), false).toBool();
    m_directory = m_settings->value(QStringLiteral("terminal/directory")).toString();
}
QString DesktopSettings::effectiveWorkingDirectory() const
{
    const QFileInfo info(m_directory);
    return !m_directory.isEmpty() && info.isDir() && info.isReadable() ? info.canonicalFilePath()
                                                                       : QDir::homePath();
}
bool DesktopSettings::apply(
    const QString& family, int size, int cursor, double line, int scrollback, bool blink)
{
    if (family.trimmed().isEmpty() || family.size() > 256 || family.contains(QChar::Null) || size < 8
        || size > 32 || cursor < 0 || cursor > 2 || !std::isfinite(line) || line < 0.8 || line > 2.0
        || scrollback < 100 || scrollback > 100000) {
        m_error = tr("Choose valid terminal settings.");
        emit settingsErrorChanged();
        return false;
    }
    m_fontFamily = family.trimmed();
    m_fontSize = size;
    m_cursor = static_cast<CursorStyle>(cursor);
    m_lineHeight = line;
    m_scrollback = scrollback;
    m_blink = blink;
    const bool saved = persist();
    emit settingsChanged();
    return saved;
}
void DesktopSettings::setWorkingDirectory(const QString& path)
{
    const QFileInfo info(path);
    if (!QDir::isAbsolutePath(path) || !info.isDir() || !info.isReadable())
        return;
    m_directory = info.canonicalFilePath();
    persist();
    emit settingsChanged();
}
void DesktopSettings::resetTerminal()
{
    apply(QStringLiteral("monospace"), 13, Block, 1.0, 10000, false);
}
void DesktopSettings::clearError()
{
    if (!m_error.isEmpty()) {
        m_error.clear();
        emit settingsErrorChanged();
    }
}
bool DesktopSettings::persist()
{
    m_settings->setValue(QStringLiteral("terminal/fontFamily"), m_fontFamily);
    m_settings->setValue(QStringLiteral("terminal/fontSize"), m_fontSize);
    m_settings->setValue(QStringLiteral("terminal/cursor"), static_cast<int>(m_cursor));
    m_settings->setValue(QStringLiteral("terminal/lineHeight"), m_lineHeight);
    m_settings->setValue(QStringLiteral("terminal/scrollback"), m_scrollback);
    m_settings->setValue(QStringLiteral("terminal/blink"), m_blink);
    m_settings->setValue(QStringLiteral("terminal/directory"), m_directory);
    m_settings->sync();
    if (m_settings->status() != QSettings::NoError) {
        m_error = tr("Terminal settings could not be saved.");
        emit settingsErrorChanged();
        return false;
    }
    clearError();
    return true;
}
}
