#include "presentation/ChatWords.hpp"
#include "presentation/DosiWords.hpp"

#include <QHash>
#include <QJsonArray>
#include <QLocale>
#include <QSet>

namespace kodosi {
namespace {
QString action(const QString& kind)
{
    static const QHash<QString, QString> actions { { QStringLiteral("command"), QStringLiteral("run") },
        { QStringLiteral("read"), QStringLiteral("read") }, { QStringLiteral("edit"), QStringLiteral("edit") },
        { QStringLiteral("write"), QStringLiteral("write") }, { QStringLiteral("search"), QStringLiteral("search") },
        { QStringLiteral("web"), QStringLiteral("fetch") }, { QStringLiteral("agent"), QStringLiteral("delegate") },
        { QStringLiteral("plan"), QStringLiteral("plan") } };
    return actions.value(kind, QStringLiteral("other"));
}

bool open(const QJsonObject& tool)
{
    const auto state = tool.value(QStringLiteral("state")).toString();
    return state == QStringLiteral("preparing") || state == QStringLiteral("asking") || state == QStringLiteral("running");
}

QString clause(const QString& kind, int count)
{
    if (kind == QStringLiteral("command"))
        return ChatWords::tr("ran %n commands", "", count);
    if (kind == QStringLiteral("read"))
        return ChatWords::tr("read %n files", "", count);
    if (kind == QStringLiteral("edit"))
        return ChatWords::tr("changed %n files", "", count);
    if (kind == QStringLiteral("write"))
        return ChatWords::tr("wrote %n files", "", count);
    if (kind == QStringLiteral("search"))
        return ChatWords::tr("made %n searches", "", count);
    if (kind == QStringLiteral("web"))
        return ChatWords::tr("fetched %n pages", "", count);
    if (kind == QStringLiteral("agent"))
        return ChatWords::tr("asked %n helpers", "", count);
    if (kind == QStringLiteral("skill"))
        return ChatWords::tr("used %n skills", "", count);
    if (kind == QStringLiteral("question"))
        return ChatWords::tr("asked %n questions", "", count);
    if (kind == QStringLiteral("plan"))
        return ChatWords::tr("made %n plans", "", count);
    return ChatWords::tr("used %n tools", "", count);
}
}

QString ChatWords::icon(const QString& kind)
{
    if (kind == QStringLiteral("question"))
        return QStringLiteral("question");
    if (kind == QStringLiteral("skill"))
        return QStringLiteral("sparkle");
    return DosiWords::deed(action(kind)).icon;
}

QString ChatWords::step(const QJsonObject& tool)
{
    const auto kind = tool.value(QStringLiteral("kind")).toString();
    const auto detail = tool.value(QStringLiteral("detail")).toObject();
    const auto questions = detail.value(QStringLiteral("questions")).toArray();
    if (kind == QStringLiteral("question") && detail.value(QStringLiteral("kind")).toString() == QStringLiteral("question")
        && !questions.isEmpty()) {
        const auto first = questions.first().toObject().value(QStringLiteral("question")).toString();
        const auto answers = detail.value(QStringLiteral("answers")).toArray();
        return answers.isEmpty() ? first : first + QStringLiteral(" · ") + answers.first().toString();
    }
    auto subject = tool.value(QStringLiteral("target")).toString().section(u'\n', 0, 0);
    if (subject.isEmpty() && (kind == QStringLiteral("skill") || kind == QStringLiteral("other")))
        subject = tool.value(QStringLiteral("name")).toString();
    const auto deed = DosiWords::deed(action(kind));
    return open(tool) ? deed.doing(subject) : deed.done(subject);
}

QString ChatWords::summary(const QList<QJsonObject>& tools)
{
    QStringList order;
    QHash<QString, int> counts;
    QHash<QString, QSet<QString>> files;
    for (const auto& tool : tools) {
        const auto kind = tool.value(QStringLiteral("kind")).toString();
        const auto target = tool.value(QStringLiteral("target")).toString();
        if (!counts.contains(kind))
            order.append(kind);
        if ((kind == QStringLiteral("read") || kind == QStringLiteral("edit") || kind == QStringLiteral("write"))
            && !target.isEmpty()) {
            files[kind].insert(target);
            counts[kind] = static_cast<int>(files[kind].size());
        } else {
            ++counts[kind];
        }
    }
    QStringList clauses;
    for (const auto& kind : order)
        clauses.append(clause(kind, counts.value(kind)));
    auto sentence = QLocale().createSeparatedList(clauses);
    if (!sentence.isEmpty())
        sentence[0] = sentence[0].toUpper();
    return sentence;
}

QVariantList ChatWords::setbacks(const QList<QJsonObject>& tools)
{
    const auto count = [&tools](const QString& state) {
        return static_cast<int>(std::ranges::count_if(
            tools, [&state](const QJsonObject& tool) { return tool.value(QStringLiteral("state")).toString() == state; }));
    };
    QVariantList words;
    const auto add = [&](const QString& state, const QString& said) {
        words.append(QVariantMap { { QStringLiteral("state"), state }, { QStringLiteral("words"), said } });
    };
    if (const auto failed = count(QStringLiteral("failed")))
        add(QStringLiteral("failed"), tr("%n failed", "", failed));
    if (const auto denied = count(QStringLiteral("denied")))
        add(QStringLiteral("denied"), tr("%n denied", "", denied));
    if (const auto stopped = count(QStringLiteral("stopped")))
        add(QStringLiteral("stopped"), tr("%n stopped", "", stopped));
    return words;
}

QString ChatWords::mode(const QString& mode)
{
    if (mode == QStringLiteral("ask"))
        return tr("Ask");
    if (mode == QStringLiteral("auto"))
        return tr("Auto");
    if (mode == QStringLiteral("unrestricted"))
        return tr("Unrestricted");
    if (mode == QStringLiteral("edits"))
        return tr("Edits");
    if (mode == QStringLiteral("plan"))
        return tr("Plan");
    if (mode == QStringLiteral("rules"))
        return tr("Rules");
    return {};
}

QString ChatWords::modeIcon(const QString& mode)
{
    static const QHash<QString, QString> icons { { QStringLiteral("ask"), QStringLiteral("hand") },
        { QStringLiteral("auto"), QStringLiteral("sparkle") }, { QStringLiteral("unrestricted"), QStringLiteral("bolt") },
        { QStringLiteral("edits"), QStringLiteral("pencil") }, { QStringLiteral("plan"), icon(QStringLiteral("plan")) },
        { QStringLiteral("rules"), QStringLiteral("lock") } };
    return icons.value(mode);
}

QString ChatWords::duration(qint64 ms)
{
    const auto seconds = ms / 1000;
    QStringList parts;
    if (seconds >= 3600)
        parts.append(tr("%1h").arg(seconds / 3600));
    if (seconds % 3600 >= 60)
        parts.append(tr("%1m").arg(seconds / 60 % 60));
    if (seconds % 60 > 0 || seconds == 0)
        parts.append(tr("%1s").arg(seconds % 60));
    auto words = parts.takeFirst();
    for (const auto& part : parts)
        words = tr("%1 %2", "the parts of a duration").arg(words, part);
    return words;
}

QString ChatWords::cost(double dollars)
{
    return QLocale().toCurrencyString(dollars, tr("$", "the sign of US dollars"), 2);
}

QString ChatWords::modelName(const QString& model, const QVariantMap& models)
{
    const auto known = models.value(QStringLiteral("models")).toList();
    for (const auto& entry : known) {
        const auto option = entry.toMap();
        if (option.value(QStringLiteral("id")).toString() == model)
            return option.value(QStringLiteral("label")).toString();
    }
    const auto parts = model.section(u'[', 0, 0).split(u'-');
    for (const auto& entry : known) {
        const auto option = entry.toMap();
        const auto at = parts.indexOf(option.value(QStringLiteral("id")).toString());
        if (at < 0)
            continue;
        QStringList version;
        for (auto part = at + 1; part < parts.size(); ++part) {
            const auto& piece = parts.at(part);
            if (piece.isEmpty() || piece.size() > 2 || !std::ranges::all_of(piece, [](QChar c) { return c.isDigit(); }))
                break;
            version.append(piece);
        }
        const auto label = option.value(QStringLiteral("label")).toString();
        return version.isEmpty() ? label : label + u' ' + version.join(u'.');
    }
    return model;
}

}
