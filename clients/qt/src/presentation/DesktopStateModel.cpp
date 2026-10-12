#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include <QEvent>
#include <QGuiApplication>
#include <QScreen>
#include <QWindow>
#include <algorithm>

namespace kodosi {
DesktopStateModel::DesktopStateModel(QObject* parent)
    : DesktopStateModel(std::make_unique<QSettings>(), true, parent)
{
}
DesktopStateModel::DesktopStateModel(std::unique_ptr<QSettings> settings, bool persistence, QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
    , m_persistenceEnabled(persistence)
{
    m_sidebarOpen = m_settings->value(QStringLiteral("window/sidebar"), true).toBool();
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
    if (m_persistenceEnabled)
        m_settings->setValue(QStringLiteral("window/sidebar"), value);
    emit sidebarOpenChanged();
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
    }
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
    emit stagedSessionIdsChanged();
    if (m_selected == id) {
        m_selected = m_staged.isEmpty() ? QString {} : m_staged.first();
        emit selectedSessionIdChanged();
    }
    if (m_staged.isEmpty())
        exitFocusMode();
    return true;
}
void DesktopStateModel::enterFocusMode(const QString& id)
{
    if (!id.isEmpty() && !stageSession(id))
        return;
    if (m_selected.isEmpty() || m_mode == Focus)
        return;
    m_mode = Focus;
    emit stageLayoutModeChanged();
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
    if (m_mode == Focus)
        exitFocusMode();
    else
        enterFocusMode();
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
void DesktopStateModel::clearSessions()
{
    m_staged.clear();
    m_selected.clear();
    m_mode = Grid;
    emit stagedSessionIdsChanged();
    emit selectedSessionIdChanged();
    emit stageLayoutModeChanged();
}
void DesktopStateModel::reconcile()
{
    if (!m_sessions || !m_sessions->hasAuthoritativeSnapshot())
        return;
    for (const auto& id : QStringList(m_staged)) {
        const auto session = m_sessions->presentationSession(id);
        if (!session || !session->canRetainPresentation)
            unstageSession(id);
    }
}
void DesktopStateModel::attachWindow(QWindow* window, std::optional<QSize> overrideSize)
{
    if (m_window)
        m_window->removeEventFilter(this);
    m_window = window;
    if (!window)
        return;
    const auto* screen = window->screen() ? window->screen() : QGuiApplication::primaryScreen();
    const QRect available = screen ? screen->availableGeometry() : QRect(0, 0, 1240, 800);
    window->setMinimumSize(QSize(std::min(820, available.width()), std::min(560, available.height())));
    QRect geometry = m_settings
                         ->value(QStringLiteral("window/geometry"),
                             QRect(available.topLeft() + QPoint(40, 40), QSize(1240, 800)))
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
    if (!m_persistenceEnabled || !m_window)
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
