#include "platform/linux/LinuxPresenceSource.hpp"

#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusReply>
#include <QDBusVariant>
#include <QGuiApplication>
#include <QtWaylandClient/QWaylandClientExtensionTemplate>

#include "qwayland-ext-idle-notify-v1.h"

#include <optional>
#include <utility>

namespace kodosi {
namespace {

const auto login = QStringLiteral("org.freedesktop.login1");
const auto loginSession = QStringLiteral("org.freedesktop.login1.Session");
const auto lockedHint = QStringLiteral("LockedHint");
const auto active = QStringLiteral("Active");
const auto properties = QStringLiteral("org.freedesktop.DBus.Properties");
const auto mutter = QStringLiteral("org.gnome.Mutter.IdleMonitor");
const auto mutterPath = QStringLiteral("/org/gnome/Mutter/IdleMonitor/Core");
constexpr int answerTime = 1000;

class IdleNotifier final : public QWaylandClientExtensionTemplate<IdleNotifier>, public QtWayland::ext_idle_notifier_v1 {
public:
    IdleNotifier()
        : QWaylandClientExtensionTemplate<IdleNotifier>(2)
    {
        initialize();
    }

    ~IdleNotifier() override
    {
        if (isActive())
            destroy();
    }
};

class IdleNotification final : public QtWayland::ext_idle_notification_v1 {
public:
    IdleNotification(::ext_idle_notification_v1* notification, IdleWatch::Quiet quiet)
        : QtWayland::ext_idle_notification_v1(notification)
        , m_quiet(std::move(quiet))
    {
    }

    ~IdleNotification() override { destroy(); }

protected:
    void ext_idle_notification_v1_idled() override { m_quiet(true); }
    void ext_idle_notification_v1_resumed() override { m_quiet(false); }

private:
    IdleWatch::Quiet m_quiet;
};

class WaylandIdleWatch final : public IdleWatch {
public:
    WaylandIdleWatch(std::unique_ptr<IdleNotifier> notifier, wl_seat* seat, Quiet quiet)
        : m_notifier(std::move(notifier))
        , m_seat(seat)
        , m_quiet(std::move(quiet))
    {
    }

    // v2 tracks user input only; with v1, an app that inhibits idle (such as a video player) also
    // counts as activity.
    void watch(const std::chrono::milliseconds quietTime) override
    {
        m_notification.reset();
        m_quiet(false);
        const auto timeout = static_cast<uint32_t>(quietTime.count());
        m_notification = std::make_unique<IdleNotification>(
            m_notifier->QtWayland::ext_idle_notifier_v1::version() >= 2
                ? m_notifier->get_input_idle_notification(timeout, m_seat)
                : m_notifier->get_idle_notification(timeout, m_seat),
            m_quiet);
    }

private:
    std::unique_ptr<IdleNotifier> m_notifier;
    std::unique_ptr<IdleNotification> m_notification;
    wl_seat* m_seat;
    Quiet m_quiet;
};

class MutterIdleWatch final : public QObject, public IdleWatch {
    Q_OBJECT

public:
    MutterIdleWatch(const QDBusConnection& bus, Quiet quiet)
        : m_bus(bus)
        , m_quiet(std::move(quiet))
    {
        (void)m_bus.connect(mutter, mutterPath, mutter, QStringLiteral("WatchFired"), this, SLOT(fired(uint)));
    }

    ~MutterIdleWatch() override
    {
        remove(m_idleWatch);
        remove(m_activeWatch);
    }

    void watch(const std::chrono::milliseconds quietTime) override
    {
        remove(std::exchange(m_idleWatch, 0));
        remove(std::exchange(m_activeWatch, 0));
        m_idleWatch = call<uint>(QStringLiteral("AddIdleWatch"), { QVariant::fromValue(quint64(quietTime.count())) })
                          .value_or(0);
        const auto idle = call<quint64>(QStringLiteral("GetIdletime"));
        setQuiet(idle && *idle >= quint64(quietTime.count()));
    }

private slots:
    void fired(const uint watch)
    {
        if (watch != 0 && watch == m_idleWatch) {
            setQuiet(true);
        } else if (watch != 0 && watch == m_activeWatch) {
            m_activeWatch = 0;
            setQuiet(false);
        }
    }

private:
    QDBusConnection m_bus;
    Quiet m_quiet;
    uint m_idleWatch = 0;
    uint m_activeWatch = 0;

    template <typename Answer>
    std::optional<Answer> call(const QString& name, const QVariantList& arguments = {})
    {
        auto request = QDBusMessage::createMethodCall(mutter, mutterPath, mutter, name);
        request.setArguments(arguments);
        const QDBusReply<Answer> reply = m_bus.call(request, QDBus::Block, answerTime);
        return reply.isValid() ? std::optional(reply.value()) : std::nullopt;
    }

    void remove(const uint watch)
    {
        if (watch == 0)
            return;
        auto request = QDBusMessage::createMethodCall(mutter, mutterPath, mutter, QStringLiteral("RemoveWatch"));
        request.setArguments({ QVariant::fromValue(watch) });
        (void)m_bus.call(request, QDBus::NoBlock);
    }

    void setQuiet(const bool quiet)
    {
        if (quiet && m_activeWatch == 0)
            m_activeWatch = call<uint>(QStringLiteral("AddUserActiveWatch")).value_or(0);
        m_quiet(quiet);
    }
};

}

std::unique_ptr<IdleWatch> waylandIdleWatch(IdleWatch::Quiet quiet)
{
    auto* wayland = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
    if (!QGuiApplication::platformName().startsWith(QStringLiteral("wayland")) || !wayland || !wayland->seat())
        return nullptr;
    auto notifier = std::make_unique<IdleNotifier>();
    if (!notifier->isActive())
        return nullptr;
    qCInfo(kodosiPresence).noquote() << "The Wayland compositor gives ext-idle-notify version"
                                     << notifier->QtWayland::ext_idle_notifier_v1::version();
    return std::make_unique<WaylandIdleWatch>(std::move(notifier), wayland->seat(), std::move(quiet));
}

std::unique_ptr<IdleWatch> mutterIdleWatch(IdleWatch::Quiet quiet)
{
    const auto bus = QDBusConnection::sessionBus();
    if (!bus.isConnected() || !bus.interface() || !bus.interface()->isServiceRegistered(mutter))
        return nullptr;
    return std::make_unique<MutterIdleWatch>(bus, std::move(quiet));
}

LinuxPresenceSource::LinuxPresenceSource(QObject* parent)
    : PresenceSource(parent)
    , m_system(QDBusConnection::systemBus())
{
    const auto quiet = [this](const bool value) { set(m_quiet, value); };
    const char* from = "the Wayland compositor";
    m_idle = waylandIdleWatch(quiet);
    if (!m_idle) {
        from = "GNOME";
        m_idle = mutterIdleWatch(quiet);
    }
    if (!m_idle) {
        from = "the X server";
        m_idle = x11IdleWatch(quiet);
    }
    if (m_idle)
        qCInfo(kodosiPresence).noquote() << "Input times come from" << from;
    else
        qCInfo(kodosiPresence).noquote() << "The desktop gives no input times; only input into Kodosi counts";
    watchSession();
}

LinuxPresenceSource::~LinuxPresenceSource() = default;

void LinuxPresenceSource::setAwayTime(const std::chrono::milliseconds time)
{
    if (m_idle)
        m_idle->watch(time);
}

void LinuxPresenceSource::watchSession()
{
    if (!m_system.isConnected())
        return;
    const auto manager = QStringLiteral("/org/freedesktop/login1");
    const auto managerInterface = QStringLiteral("org.freedesktop.login1.Manager");
    (void)m_system.connect(
        login, manager, managerInterface, QStringLiteral("PrepareForSleep"), this, SLOT(prepareForSleep(bool)));
    auto request = QDBusMessage::createMethodCall(login, manager, managerInterface, QStringLiteral("GetSession"));
    request.setArguments({ QStringLiteral("auto") });
    const QDBusReply<QDBusObjectPath> session = m_system.call(request, QDBus::Block, answerTime);
    if (!session.isValid())
        return;
    m_session = session.value().path();
    (void)m_system.connect(login, m_session, properties, QStringLiteral("PropertiesChanged"), this,
        SLOT(sessionChanged(QString, QVariantMap, QStringList)));
    readSession();
}

// Read logind session state instead of Lock/Unlock signals: some lockers ignore those signals,
// and password unlocks do not emit Unlock. Lockers either set LockedHint or, like light-locker,
// switch to the greeter, which makes the session inactive.
void LinuxPresenceSource::readSession()
{
    auto request = QDBusMessage::createMethodCall(login, m_session, properties, QStringLiteral("GetAll"));
    request.setArguments({ loginSession });
    const QDBusReply<QVariantMap> facts = m_system.call(request, QDBus::Block, answerTime);
    if (facts.isValid())
        follow(facts.value());
}

void LinuxPresenceSource::follow(const QVariantMap& facts)
{
    if (const auto hint = facts.constFind(lockedHint); hint != facts.cend())
        set(m_locked, hint.value().toBool());
    if (const auto shown = facts.constFind(active); shown != facts.cend())
        set(m_inactive, !shown.value().toBool());
}

void LinuxPresenceSource::prepareForSleep(const bool start)
{
    set(m_sleeping, start);
}

void LinuxPresenceSource::sessionChanged(
    const QString& interface, const QVariantMap& changed, const QStringList& invalidated)
{
    if (interface != loginSession)
        return;
    follow(changed);
    if (invalidated.contains(lockedHint) || invalidated.contains(active))
        readSession();
}

void LinuxPresenceSource::set(bool& fact, const bool value)
{
    if (std::exchange(fact, value) != value)
        emit changed();
}

}

#include "LinuxPresenceSource.moc"
