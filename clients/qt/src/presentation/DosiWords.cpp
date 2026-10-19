#include "presentation/DosiWords.hpp"

#include <QLocale>
#include <QStringList>

namespace kodosi {
namespace {
bool isRunning(const QVariantMap& tool)
{
    return tool.value(QStringLiteral("status")).toString() == QStringLiteral("running");
}

QString targetOf(const QVariantMap& tool)
{
    return DosiWords::target(tool.value(QStringLiteral("target")).toMap());
}

DosiWords::Deed deedOf(QString done, QString doing, QString icon, QString saying = {}, QString said = {})
{
    return { std::move(done), std::move(doing), std::move(icon), std::move(saying), std::move(said) };
}
}

DosiWords::Deed DosiWords::deed(const QString& action)
{
    if (action == QStringLiteral("look"))
        return deedOf(tr("Looked at"), tr("looking at"), QStringLiteral("eye"));
    if (action == QStringLiteral("read"))
        return deedOf(tr("Read"), tr("reading"), QStringLiteral("document"));
    if (action == QStringLiteral("type"))
        return deedOf(tr("Typed in"), tr("typing in"), QStringLiteral("prompt"),
            tr("typing"), tr("typed"));
    if (action == QStringLiteral("press"))
        return deedOf(tr("Pressed keys in"), tr("pressing keys in"), QStringLiteral("command"),
            tr("pressing"), tr("pressed"));
    if (action == QStringLiteral("start"))
        return deedOf(tr("Started"), tr("starting"), QStringLiteral("play"));
    if (action == QStringLiteral("interrupt"))
        return deedOf(tr("Interrupted"), tr("interrupting"), QStringLiteral("stop"));
    if (action == QStringLiteral("close"))
        return deedOf(tr("Closed"), tr("closing"), QStringLiteral("close"));
    if (action == QStringLiteral("show"))
        return deedOf(tr("Showed"), tr("showing"), QStringLiteral("window-restore"));
    if (action == QStringLiteral("wait"))
        return deedOf(tr("Waited for"), tr("waiting for"), QStringLiteral("history"));
    if (action == QStringLiteral("post"))
        return deedOf(tr("Posted in"), tr("posting in"), QStringLiteral("chat"),
            tr("posting"), tr("posted"));
    if (action == QStringLiteral("run"))
        return deedOf(tr("Ran"), tr("running"), QStringLiteral("terminal"));
    if (action == QStringLiteral("write"))
        return deedOf(tr("Wrote"), tr("writing"), QStringLiteral("pencil"));
    if (action == QStringLiteral("edit"))
        return deedOf(tr("Changed"), tr("changing"), QStringLiteral("pencil"));
    if (action == QStringLiteral("search"))
        return deedOf(tr("Searched for"), tr("searching for"), QStringLiteral("search"));
    if (action == QStringLiteral("fetch"))
        return deedOf(tr("Fetched"), tr("fetching"), QStringLiteral("link"));
    if (action == QStringLiteral("plan"))
        return deedOf(tr("Planned"), tr("planning"), QStringLiteral("tasks"));
    if (action == QStringLiteral("delegate"))
        return deedOf(tr("Asked a helper"), tr("asking a helper"), QStringLiteral("people"));
    return deedOf(tr("Used"), tr("using"), QStringLiteral("cube"));
}

QString DosiWords::target(const QVariantMap& where)
{
    const auto kind = where.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("terminals"))
        return tr("the terminals");
    if (kind == QStringLiteral("rooms"))
        return tr("the rooms");
    return where.value(QStringLiteral("value")).toString();
}

QString DosiWords::doing(const QVariantMap& tool)
{
    const auto verb = deed(tool.value(QStringLiteral("action")).toString()).doing;
    const auto where = targetOf(tool);
    return where.isEmpty() ? verb : verb + QLatin1Char(' ') + where;
}

QString DosiWords::receipt(const QVariantMap& tool)
{
    const auto action = deed(tool.value(QStringLiteral("action")).toString());
    const auto where = targetOf(tool);
    if (tool.value(QStringLiteral("text")).toString().trimmed().isEmpty()) {
        const auto verb = isRunning(tool) ? action.doing : action.done.toLower();
        return where.isEmpty() ? verb : verb + QLatin1Char(' ') + where;
    }
    const auto words = said(tool);
    return where.isEmpty() ? words : words + QStringLiteral(" · ") + where;
}

QString DosiWords::said(const QVariantMap& tool)
{
    const auto action = deed(tool.value(QStringLiteral("action")).toString());
    const auto verb = isRunning(tool) ? (action.saying.isEmpty() ? action.doing : action.saying)
                                    : (action.said.isEmpty() ? action.done.toLower() : action.said);
    const auto text = tool.value(QStringLiteral("text")).toString().trimmed();
    return text.isEmpty() ? verb : verb + QLatin1Char(' ') + text.section(QLatin1Char('\n'), 0, 0);
}

QString DosiWords::summary(const QVariantList& tools)
{
    bool active = false;
    QStringList actions;
    QStringList names;
    for (const auto& value : tools) {
        const auto tool = value.toMap();
        active = active || isRunning(tool);
        const auto action = tool.value(QStringLiteral("action")).toString();
        if (!actions.contains(action))
            actions.append(action);
        const auto name = targetOf(tool);
        if (!name.isEmpty() && !names.contains(name))
            names.append(name);
    }
    QString verb;
    if (actions.size() == 1) {
        const auto action = deed(actions.first());
        verb = active ? action.doing : action.done.toLower();
    } else {
        verb = active ? tr("working on") : tr("worked on");
    }
    if (names.isEmpty())
        return verb;
    const auto shown = names.mid(0, 2).join(QStringLiteral(", "));
    if (names.size() > 2)
        return QStringLiteral("%1 %2 +%3").arg(verb, shown).arg(names.size() - 2);
    return verb + QLatin1Char(' ') + shown;
}

QString DosiWords::day(const QDateTime& date, const QDateTime& now)
{
    const auto days = date.date().daysTo(now.date());
    if (days == 0) {
        const auto hour = date.time().hour();
        return hour < 12 ? tr("this morning") : hour < 17 ? tr("this afternoon") : tr("this evening");
    }
    if (days == 1)
        return tr("yesterday");
    if (days > 1 && days < 7)
        return QLocale().dayName(date.date().dayOfWeek()).toLower();
    return QLocale().toString(date.date(), QStringLiteral("d MMM"));
}

QString DosiWords::mood(const QString& name)
{
    if (name == QStringLiteral("listening"))
        return tr("Listening");
    if (name == QStringLiteral("thinking"))
        return tr("Thinking");
    if (name == QStringLiteral("reading"))
        return tr("Reading");
    if (name == QStringLiteral("working"))
        return tr("Working");
    if (name == QStringLiteral("talking"))
        return tr("Writing");
    if (name == QStringLiteral("asking"))
        return tr("Asking you");
    if (name == QStringLiteral("stuck"))
        return tr("Stuck");
    if (name == QStringLiteral("stopping"))
        return tr("Stopping");
    if (name == QStringLiteral("off"))
        return tr("Off");
    return tr("Ready");
}

}
