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

DosiWords::Phrase phrase(QString alone, QString about)
{
    return { std::move(alone), std::move(about) };
}
}

DosiWords::Deed DosiWords::deed(const QString& action)
{
    if (action == QStringLiteral("look"))
        return { phrase(tr("looked at"), tr("looked at %1")), phrase(tr("looking at"), tr("looking at %1")),
            QStringLiteral("eye") };
    if (action == QStringLiteral("read"))
        return { phrase(tr("read"), tr("read %1")), phrase(tr("reading"), tr("reading %1")), QStringLiteral("document") };
    if (action == QStringLiteral("type"))
        return { phrase(tr("typed in"), tr("typed in %1")), phrase(tr("typing in"), tr("typing in %1")),
            QStringLiteral("prompt"), phrase(tr("typed"), tr("typed %1")), phrase(tr("typing"), tr("typing %1")) };
    if (action == QStringLiteral("press"))
        return { phrase(tr("pressed keys in"), tr("pressed keys in %1")),
            phrase(tr("pressing keys in"), tr("pressing keys in %1")), QStringLiteral("command"),
            phrase(tr("pressed"), tr("pressed %1")), phrase(tr("pressing"), tr("pressing %1")) };
    if (action == QStringLiteral("start"))
        return { phrase(tr("started"), tr("started %1")), phrase(tr("starting"), tr("starting %1")),
            QStringLiteral("play") };
    if (action == QStringLiteral("interrupt"))
        return { phrase(tr("interrupted"), tr("interrupted %1")), phrase(tr("interrupting"), tr("interrupting %1")),
            QStringLiteral("stop") };
    if (action == QStringLiteral("close"))
        return { phrase(tr("closed"), tr("closed %1")), phrase(tr("closing"), tr("closing %1")), QStringLiteral("close") };
    if (action == QStringLiteral("show"))
        return { phrase(tr("showed"), tr("showed %1")), phrase(tr("showing"), tr("showing %1")),
            QStringLiteral("window-restore") };
    if (action == QStringLiteral("wait"))
        return { phrase(tr("waited for"), tr("waited for %1")), phrase(tr("waiting for"), tr("waiting for %1")),
            QStringLiteral("history") };
    if (action == QStringLiteral("post"))
        return { phrase(tr("posted in"), tr("posted in %1")), phrase(tr("posting in"), tr("posting in %1")),
            QStringLiteral("chat"), phrase(tr("posted"), tr("posted %1")), phrase(tr("posting"), tr("posting %1")) };
    if (action == QStringLiteral("run"))
        return { phrase(tr("ran"), tr("ran %1")), phrase(tr("running"), tr("running %1")), QStringLiteral("terminal") };
    if (action == QStringLiteral("write"))
        return { phrase(tr("wrote"), tr("wrote %1")), phrase(tr("writing"), tr("writing %1")), QStringLiteral("pencil") };
    if (action == QStringLiteral("edit"))
        return { phrase(tr("changed"), tr("changed %1")), phrase(tr("changing"), tr("changing %1")),
            QStringLiteral("pencil") };
    if (action == QStringLiteral("search"))
        return { phrase(tr("searched for"), tr("searched for %1")), phrase(tr("searching for"), tr("searching for %1")),
            QStringLiteral("search") };
    if (action == QStringLiteral("fetch"))
        return { phrase(tr("fetched"), tr("fetched %1")), phrase(tr("fetching"), tr("fetching %1")),
            QStringLiteral("link") };
    if (action == QStringLiteral("plan"))
        return { phrase(tr("planned"), tr("planned %1")), phrase(tr("planning"), tr("planning %1")),
            QStringLiteral("tasks") };
    if (action == QStringLiteral("delegate"))
        return { phrase(tr("asked a helper"), tr("asked a helper about %1")),
            phrase(tr("asking a helper"), tr("asking a helper about %1")), QStringLiteral("people") };
    return { phrase(tr("used"), tr("used %1")), phrase(tr("using"), tr("using %1")), QStringLiteral("cube") };
}

QString DosiWords::target(const QVariantMap& where)
{
    const auto kind = where.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("terminals"))
        return tr("the terminals");
    if (kind == QStringLiteral("rooms"))
        return tr("the projects");
    return where.value(QStringLiteral("value")).toString();
}

QString DosiWords::receipt(const QVariantMap& tool)
{
    const auto action = deed(tool.value(QStringLiteral("action")).toString());
    const auto where = targetOf(tool);
    if (tool.value(QStringLiteral("text")).toString().trimmed().isEmpty())
        return isRunning(tool) ? action.doing(where) : action.done(where);
    const auto words = said(tool);
    return where.isEmpty() ? words : words + QStringLiteral(" · ") + where;
}

QString DosiWords::said(const QVariantMap& tool)
{
    const auto action = deed(tool.value(QStringLiteral("action")).toString());
    const auto& phrase = isRunning(tool) ? (action.saying.alone.isEmpty() ? action.doing : action.saying)
                                         : (action.said.alone.isEmpty() ? action.done : action.said);
    return phrase(tool.value(QStringLiteral("text")).toString().trimmed().section(QLatin1Char('\n'), 0, 0));
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
    const auto phrase = actions.size() == 1
        ? (active ? deed(actions.first()).doing : deed(actions.first()).done)
        : (active ? Phrase { tr("working on"), tr("working on %1") } : Phrase { tr("worked on"), tr("worked on %1") });
    const auto shown = phrase(names.mid(0, 2).join(QStringLiteral(", ")));
    return names.size() > 2 ? QStringLiteral("%1 +%2").arg(shown).arg(names.size() - 2) : shown;
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
    if (days > 1 && days < 7) {
        const auto weekday = QLocale().dayName(date.date().dayOfWeek());
        return QLocale().language() == QLocale::English ? weekday.toLower() : weekday;
    }
    return QLocale().toString(date.date(), tr("d MMM"));
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
