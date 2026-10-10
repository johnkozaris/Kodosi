#include "presentation/DesktopSettings.hpp"
#include <QDir>
#include <QFileInfo>
#include <QUuid>
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
    m_branchFolder = m_settings->value(QStringLiteral("terminal/branchFolder")).toString();
    m_learned = m_settings->value(QStringLiteral("agents/learned")).toStringList();
    const int rows = std::min(m_settings->beginReadArray(QStringLiteral("agents/start")), 64);
    for (int row = 0; row < rows; ++row) {
        m_settings->setArrayIndex(row);
        const StartCommand entry { m_settings->value(QStringLiteral("id")).toString(),
            m_settings->value(QStringLiteral("name")).toString(), m_settings->value(QStringLiteral("command")).toString() };
        if (!entry.id.isEmpty() && entry.name.size() <= 128 && entry.command.size() <= 1024)
            m_start.append(entry);
    }
    m_settings->endArray();
}
QString DesktopSettings::agentOf(const QString& command)
{
    for (const auto& word : command.split(QChar::Space, Qt::SkipEmptyParts)) {
        if (word == QStringLiteral("env") || word.contains(QLatin1Char('=')))
            continue;
        const auto program = QFileInfo(word).fileName();
        if (program == QStringLiteral("cursor-agent"))
            return QStringLiteral("cursor");
        return QStringList { QStringLiteral("claude"), QStringLiteral("codex"), QStringLiteral("copilot") }.contains(program)
            ? program
            : QStringLiteral("shell");
    }
    return QStringLiteral("shell");
}
QVariantList DesktopSettings::startCommands() const
{
    QVariantList result;
    QStringList seen;
    for (const auto& entry : m_start) {
        const auto agent = agentOf(entry.command);
        const auto last = entry.name.split(QChar::Space, Qt::SkipEmptyParts).value(
            entry.name.split(QChar::Space, Qt::SkipEmptyParts).size() - 1);
        result.append(QVariantMap { { QStringLiteral("id"), entry.id }, { QStringLiteral("name"), entry.name },
            { QStringLiteral("command"), entry.command }, { QStringLiteral("agent"), agent },
            { QStringLiteral("repeated"), seen.contains(agent) }, { QStringLiteral("initial"), last.left(1).toUpper() } });
        seen.append(agent);
    }
    return result;
}
QStringList DesktopSettings::startCommandIds() const
{
    QStringList ids;
    for (const auto& entry : m_start)
        ids.append(entry.id);
    return ids;
}
void DesktopSettings::learn(const QString& program)
{
    static const QStringList agents { QStringLiteral("claude"), QStringLiteral("codex"), QStringLiteral("copilot"),
        QStringLiteral("cursor") };
    if (!agents.contains(program) || m_learned.contains(program))
        return;
    m_learned.append(program);
    const bool known = std::ranges::any_of(m_start, [&](const StartCommand& entry) { return agentOf(entry.command) == program; });
    if (!known) {
        static const QStringList labels { QStringLiteral("Claude Code"), QStringLiteral("Codex"), QStringLiteral("Copilot"),
            QStringLiteral("Cursor") };
        m_start.append({ QUuid::createUuid().toString(QUuid::WithoutBraces), labels.value(agents.indexOf(program)),
            program == QStringLiteral("cursor") ? QStringLiteral("cursor-agent") : program });
    }
    persistStart();
    if (!known) {
        emit startCommandIdsChanged();
        emit startCommandsChanged();
    }
}
void DesktopSettings::addStartCommand()
{
    if (m_start.size() >= 64)
        return;
    m_start.append({ QUuid::createUuid().toString(QUuid::WithoutBraces), {}, {} });
    persistStart();
    emit startCommandIdsChanged();
    emit startCommandsChanged();
}
void DesktopSettings::setStartCommand(const QString& id, const QString& name, const QString& command)
{
    const auto entry = std::ranges::find_if(m_start, [&](const StartCommand& value) { return value.id == id; });
    if (entry == m_start.end() || name.size() > 128 || command.size() > 1024
        || (entry->name == name && entry->command == command))
        return;
    entry->name = name;
    entry->command = command;
    persistStart();
    emit startCommandsChanged();
}
void DesktopSettings::removeStartCommand(const QString& id)
{
    if (m_start.removeIf([&](const StartCommand& value) { return value.id == id; }) == 0)
        return;
    persistStart();
    emit startCommandIdsChanged();
    emit startCommandsChanged();
}
void DesktopSettings::persistStart()
{
    m_settings->setValue(QStringLiteral("agents/learned"), m_learned);
    m_settings->beginWriteArray(QStringLiteral("agents/start"), static_cast<int>(m_start.size()));
    for (int row = 0; row < m_start.size(); ++row) {
        m_settings->setArrayIndex(row);
        m_settings->setValue(QStringLiteral("id"), m_start.at(row).id);
        m_settings->setValue(QStringLiteral("name"), m_start.at(row).name);
        m_settings->setValue(QStringLiteral("command"), m_start.at(row).command);
    }
    m_settings->endArray();
    m_settings->sync();
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
