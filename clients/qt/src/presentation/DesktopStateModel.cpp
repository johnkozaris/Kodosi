#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include <QEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QWindow>
#include <algorithm>

namespace kodosi {
DesktopStateModel::DesktopStateModel(std::unique_ptr<QSettings> settings, QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
{
    m_sidebarOpen = m_settings->value(QStringLiteral("window/sidebar"), true).toBool();
    m_dosiOn = m_settings->value(QStringLiteral("dosi/on"), true).toBool();
    m_dosiOpen = m_dosiOn && m_settings->value(QStringLiteral("window/dosi"), false).toBool();
    m_listWidth = std::clamp(m_settings->value(QStringLiteral("window/sidebarWidth"), 252.0).toDouble(), 220.0, 380.0);
    m_dosiWidth = std::clamp(m_settings->value(QStringLiteral("window/dosiWidth"), 420.0).toDouble(), 340.0, 640.0);
}
DesktopStateModel::~DesktopStateModel()
{
    saveWindow();
}
void DesktopStateModel::setActiveView(ActiveView value)
{
    if (m_activeView == value)
        return;
    m_activeView = value;
    emit activeViewChanged();
}
void DesktopStateModel::setSidebarOpen(bool value)
{
    if (m_sidebarOpen == value)
        return;
    m_sidebarOpen = value;
    m_settings->setValue(QStringLiteral("window/sidebar"), value);
    emit sidebarOpenChanged();
    emit sidebarWidthChanged();
}
void DesktopStateModel::setDosiOpen(bool value)
{
    if (m_dosiOpen == value)
        return;
    m_dosiOpen = value;
    m_settings->setValue(QStringLiteral("window/dosi"), value);
    emit dosiOpenChanged();
    emit sidebarWidthChanged();
}
void DesktopStateModel::setDosiOn(bool value)
{
    if (m_dosiOn == value)
        return;
    m_dosiOn = value;
    m_settings->setValue(QStringLiteral("dosi/on"), value);
    if (!value)
        setDosiOpen(false);
    emit dosiOnChanged();
}
void DesktopStateModel::openDosi()
{
    if (!m_dosiOn)
        return;
    setSidebarOpen(true);
    setDosiOpen(true);
}
void DesktopStateModel::toggleDosi()
{
    if (m_dosiOpen && m_sidebarOpen)
        setDosiOpen(false);
    else
        openDosi();
}
qreal DesktopStateModel::sidebarWidth() const
{
    return !m_sidebarOpen ? 78 : m_dosiOpen ? m_dosiWidth : m_listWidth;
}
void DesktopStateModel::resizeSidebar(qreal width)
{
    if (!m_sidebarOpen)
        return;
    auto& stored = m_dosiOpen ? m_dosiWidth : m_listWidth;
    const auto next = m_dosiOpen ? std::clamp(width, 340.0, 640.0) : std::clamp(width, 220.0, 380.0);
    if (qFuzzyCompare(stored, next))
        return;
    stored = next;
    m_settings->setValue(m_dosiOpen ? QStringLiteral("window/dosiWidth") : QStringLiteral("window/sidebarWidth"), next);
    emit sidebarWidthChanged();
}
void DesktopStateModel::attachSessionCatalog(SessionCatalogModel* sessions)
{
    if (m_sessions)
        disconnect(m_sessions, nullptr, this, nullptr);
    m_sessions = sessions;
    if (sessions)
        connect(sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this,
            &DesktopStateModel::reconcile);
}
bool DesktopStateModel::stageSession(const QString& id, bool revealStage)
{
    if (!m_sessions)
        return false;
    const auto session = m_sessions->presentationSession(id);
    if (!session || !session->canRetainPresentation)
        return false;
    if (!m_staged.contains(id)) {
        if (m_staged.size() >= 6) {
            m_error = tr("Six terminals are already on Stage. Close a view before opening another.");
            emit lastErrorChanged();
            return false;
        }
        m_staged.append(id);
        emit stagedSessionIdsChanged();
        saveStage();
    }
    remember(id);
    if (m_selected != id) {
        m_selected = id;
        emit selectedSessionIdChanged();
    }
    if (revealStage) setActiveView(Sessions);
    clearError();
    return true;
}
bool DesktopStateModel::selectSession(const QString& id)
{
    return stageSession(id, false);
}
bool DesktopStateModel::unstageSession(const QString& id)
{
    if (!m_staged.removeOne(id))
        return false;
    m_computers.remove(id);
    emit stagedSessionIdsChanged();
    saveStage();
    if (m_selected == id) {
        m_selected = m_staged.isEmpty() ? QString {} : m_staged.first();
        emit selectedSessionIdChanged();
    }
    if (m_staged.isEmpty())
        exitFocusMode();
    return true;
}
void DesktopStateModel::exitFocusMode()
{
    if (m_mode != Grid) {
        m_mode = Grid;
        emit stageLayoutModeChanged();
    }
}
void DesktopStateModel::toggleFocusForSelectedSession()
{
    if (m_mode == Focus) {
        exitFocusMode();
    } else if (!m_selected.isEmpty()) {
        m_mode = Focus;
        emit stageLayoutModeChanged();
    }
}
void DesktopStateModel::setModalVisible(QObject* modal, bool visible)
{
    if (!modal) return;
    const bool wasOpen = modalOpen();
    if (visible && !m_modals.contains(modal)) {
        m_modals.insert(modal, connect(modal, &QObject::destroyed, this,
            [this, modal] { setModalVisible(modal, false); }));
    } else if (!visible) {
        disconnect(m_modals.take(modal));
    }
    if (wasOpen != modalOpen()) emit modalOpenChanged();
}
void DesktopStateModel::clearError()
{
    if (!m_error.isEmpty()) {
        m_error.clear();
        emit lastErrorChanged();
    }
}
void DesktopStateModel::switchAccount(const QString& userId)
{
    saveStage();
    m_staged.clear();
    m_selected.clear();
    m_mode = Grid;
    emit stagedSessionIdsChanged();
    emit selectedSessionIdChanged();
    emit stageLayoutModeChanged();
    m_stageKey = QStringLiteral("stage/")
        + (userId.isEmpty() ? QStringLiteral("local")
                            : QString::fromLatin1(userId.toUtf8().toBase64(
                                  QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals)));
    m_restoring = m_settings->value(m_stageKey).toStringList();
    m_savedStage = m_restoring;
    m_computers.clear();
    m_departures.clear();
    m_closing.clear();
}
void DesktopStateModel::noteClosing(const QString& id)
{
    m_closing.insert(id);
}
void DesktopStateModel::saveStage()
{
    if (m_stageKey.isEmpty())
        return;
    auto kept = m_staged;
    for (const auto& id : std::as_const(m_restoring)) {
        if (!kept.contains(id))
            kept.append(id);
    }
    m_settings->setValue(m_stageKey, kept.mid(0, 64));
}
void DesktopStateModel::setLauncherPrompt(const QString& value)
{
    if (m_launcherPrompt == value)
        return;
    m_launcherPrompt = value;
    emit launcherDraftChanged();
}
void DesktopStateModel::setLauncherFolder(const QString& value)
{
    if (m_launcherFolder == value)
        return;
    m_launcherFolder = value;
    emit launcherDraftChanged();
}
void DesktopStateModel::setLauncherRemoteFolder(const QString& computer, const QString& path)
{
    if (m_launcherRemoteFolders.value(computer).toString() == path)
        return;
    m_launcherRemoteFolders.insert(computer, path);
    emit launcherDraftChanged();
}
void DesktopStateModel::clearLauncherDraft()
{
    if (m_launcherPrompt.isEmpty() && m_launcherFolder.isEmpty() && m_launcherRemoteFolders.isEmpty())
        return;
    m_launcherPrompt.clear();
    m_launcherFolder.clear();
    m_launcherRemoteFolders.clear();
    emit launcherDraftChanged();
}
void DesktopStateModel::reconcile()
{
    if (!m_sessions || !m_sessions->hasAuthoritativeSnapshot())
        return;
    for (auto place = m_staged.size() - 1; place >= 0; --place) {
        const auto id = m_staged.at(place);
        const auto computer = m_computers.value(id);
        if (!m_sessions->containsSession(id) || m_sessions->restart(id, computer.owner, computer.host)) {
            depart(id, place);
            continue;
        }
        const auto session = m_sessions->presentationSession(id);
        if (!session || !session->canRetainPresentation)
            unstageSession(id);
    }
    m_closing.removeIf([this](const QString& id) { return !m_sessions->containsSession(id); });
    const auto followed = follow();
    const auto rank = [this](const QString& id) {
        const auto found = std::ranges::find_if(
            m_savedStage, [&](const QString& saved) { return m_sessions->current(saved) == id; });
        return found == m_savedStage.end() ? -1 : int(found - m_savedStage.begin());
    };
    QStringList restored;
    for (const auto& saved : QStringList(m_restoring)) {
        const auto id = m_sessions->current(saved);
        const auto session = m_sessions->presentationSession(id);
        if (!session)
            continue;
        m_restoring.removeOne(saved);
        if (session->canRetainPresentation && !m_sessions->session(id)->hidden && !m_staged.contains(id)
            && m_staged.size() < 6) {
            m_staged.insert(std::ranges::find_if(m_staged, [&](const QString& staged) { return rank(staged) > rank(id); }),
                id);
            remember(id);
            restored.append(id);
        }
    }
    if (!restored.isEmpty()) {
        emit stagedSessionIdsChanged();
        if (m_selected.isEmpty()) {
            m_selected = m_staged.first();
            emit selectedSessionIdChanged();
        }
    }
    if (!followed.isEmpty() || !restored.isEmpty())
        emit this->restored(followed + restored);
    saveStage();
}
void DesktopStateModel::remember(const QString& id)
{
    if (const auto session = m_sessions ? m_sessions->session(id) : std::nullopt)
        m_computers.insert(id, { session->ownerUserId, session->hostDeviceId });
}
void DesktopStateModel::depart(const QString& id, qsizetype place)
{
    if (m_closing.remove(id) || !m_computers.contains(id)) {
        unstageSession(id);
        return;
    }
    const bool selected = m_selected == id;
    const bool focused = m_mode == Focus && selected;
    m_departures.append({ id, m_computers.take(id), place, selected, focused });
    if (m_departures.size() > 64)
        m_departures.removeFirst();
    m_staged.removeAt(place);
    emit stagedSessionIdsChanged();
    if (selected) {
        m_selected.clear();
        emit selectedSessionIdChanged();
    }
    if (focused || m_staged.isEmpty())
        exitFocusMode();
}
QStringList DesktopStateModel::follow()
{
    QStringList followed;
    auto departures = m_departures;
    std::ranges::reverse(departures);
    for (const auto& departure : std::as_const(departures)) {
        const auto next = m_sessions->restart(departure.id, departure.computer.owner, departure.computer.host);
        if (!next)
            continue;
        m_departures.removeIf([&](const Departure& kept) { return kept.id == departure.id; });
        const auto session = m_sessions->session(*next);
        const auto presentation = m_sessions->presentationSession(*next);
        if (!session || session->hidden || !presentation || !presentation->canRetainPresentation
            || m_staged.contains(*next) || m_staged.size() >= 6)
            continue;
        m_staged.insert(std::min(departure.place, m_staged.size()), *next);
        remember(*next);
        emit stagedSessionIdsChanged();
        if (departure.selected && m_selected.isEmpty()) {
            m_selected = *next;
            emit selectedSessionIdChanged();
        }
        if (departure.focused && m_mode == Grid && m_selected == *next) {
            m_mode = Focus;
            emit stageLayoutModeChanged();
        }
        followed.append(*next);
    }
    return followed;
}
void DesktopStateModel::attachWindow(QWindow* window, std::optional<QSize> overrideSize)
{
    if (m_window)
        m_window->removeEventFilter(this);
    m_window = window;
    if (!window)
        return;
    const auto* screen = window->screen() ? window->screen() : QGuiApplication::primaryScreen();
    const QRect available = screen ? screen->availableGeometry() : QRect(0, 0, 1280, 820);
    window->setMinimumSize(QSize(std::min(820, available.width()), std::min(560, available.height())));
    QRect geometry = m_settings
                         ->value(QStringLiteral("window/geometry"),
                             QRect(available.topLeft() + QPoint(40, 40), QSize(1280, 820)))
                         .toRect();
    if (overrideSize)
        geometry.setSize(*overrideSize);
    geometry.setSize(geometry.size().boundedTo(available.size()).expandedTo(window->minimumSize()));
    geometry.moveLeft(
        std::clamp(geometry.left(), available.left(), available.right() - geometry.width() + 1));
    geometry.moveTop(std::clamp(geometry.top(), available.top(), available.bottom() - geometry.height() + 1));
    window->setGeometry(geometry);
    window->installEventFilter(this);
    if (!overrideSize && m_settings->value(QStringLiteral("window/maximized"), false).toBool())
        window->setWindowState(Qt::WindowMaximized);
}
void DesktopStateModel::saveWindow()
{
    if (!m_window)
        return;
    m_settings->setValue(QStringLiteral("window/maximized"), m_window->windowState() == Qt::WindowMaximized);
    if (m_window->windowState() == Qt::WindowNoState)
        m_settings->setValue(QStringLiteral("window/geometry"), m_window->geometry());
    m_settings->sync();
    if (m_settings->status() != QSettings::NoError) {
        m_error = tr("Window preferences could not be saved.");
        emit lastErrorChanged();
    }
}
bool DesktopStateModel::eventFilter(QObject* object, QEvent* event)
{
    if (object == m_window
        && (event->type() == QEvent::Move || event->type() == QEvent::Resize
            || event->type() == QEvent::WindowStateChange || event->type() == QEvent::Close))
        saveWindow();
    return QObject::eventFilter(object, event);
}
}
