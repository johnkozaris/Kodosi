#include "presentation/DesktopSettings.hpp"
#include <QDir>
#include <QFileInfo>
#include <QList>
#include <algorithm>
#include <cmath>
namespace kodosi {
namespace {
struct KnownAgent {
    QString id, label;
};
const QList<KnownAgent>& knownAgents()
{
    static const QList<KnownAgent> agents { { QStringLiteral("claude"), QStringLiteral("Claude Code") },
        { QStringLiteral("codex"), QStringLiteral("Codex") }, { QStringLiteral("copilot"), QStringLiteral("Copilot") },
        { QStringLiteral("cursor"), QStringLiteral("Cursor") }, { QStringLiteral("gemini"), QStringLiteral("Gemini CLI") },
        { QStringLiteral("opencode"), QStringLiteral("OpenCode") } };
    return agents;
}
}
DesktopSettings::DesktopSettings(std::unique_ptr<QSettings> settings, bool keepRunningByDefault, QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
    , m_keepRunningByDefault(keepRunningByDefault)
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
    m_branchFolder = m_settings->value(QStringLiteral("terminal/branchFolder")).toString();
    m_computer = m_settings->value(QStringLiteral("terminal/computer")).toString();
    if (const auto keep = m_settings->value(QStringLiteral("app/keepRunning")); keep.isValid())
        m_keepRunning = keep.toBool();
    m_closeHintShown = m_settings->value(QStringLiteral("app/closeHintShown"), false).toBool();
}
QString DesktopSettings::agentOf(const QString& text)
{
    for (const auto& word : text.split(QChar::Space, Qt::SkipEmptyParts)) {
        if (word == QStringLiteral("env") || word.contains(QLatin1Char('=')))
            continue;
        for (const auto& agent : knownAgents()) {
            if (word.contains(agent.id, Qt::CaseInsensitive))
                return agent.id;
        }
    }
    return QStringLiteral("shell");
}
QString DesktopSettings::agentLabel(const QString& text)
{
    const auto agent = std::ranges::find(knownAgents(), agentOf(text), &KnownAgent::id);
    return agent == knownAgents().end() ? tr("Shell") : agent->label;
}
QString DesktopSettings::homeDirectory() const
{
    return QDir::homePath();
}
QString DesktopSettings::effectiveWorkingDirectory() const
{
    const QFileInfo info(m_directory);
    return !m_directory.isEmpty() && info.isDir() && info.isReadable() ? info.canonicalFilePath()
                                                                       : homeDirectory();
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
void DesktopSettings::setLastComputer(const QString& computer)
{
    if (computer == m_computer)
        return;
    m_computer = computer;
    persist();
    emit settingsChanged();
}
void DesktopSettings::setKeepRunning(bool keep)
{
    if (keep == keepRunning())
        return;
    m_keepRunning = keep;
    persist();
    emit settingsChanged();
}
void DesktopSettings::setCloseHintShown()
{
    if (m_closeHintShown)
        return;
    m_closeHintShown = true;
    persist();
}
void DesktopSettings::setBranchFolder(const QString& path)
{
    const QFileInfo info(path);
    if (!path.isEmpty() && (!QDir::isAbsolutePath(path) || !info.isDir() || !info.isWritable()))
        return;
    m_branchFolder = path.isEmpty() ? QString {} : info.canonicalFilePath();
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
    m_settings->setValue(QStringLiteral("terminal/branchFolder"), m_branchFolder);
    m_settings->setValue(QStringLiteral("terminal/computer"), m_computer);
    if (m_keepRunning)
        m_settings->setValue(QStringLiteral("app/keepRunning"), *m_keepRunning);
    m_settings->setValue(QStringLiteral("app/closeHintShown"), m_closeHintShown);
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
