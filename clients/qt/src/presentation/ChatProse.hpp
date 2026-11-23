#pragma once

#include <QObject>
#include <QString>
#include <QStringView>
#include <QVariantList>

namespace kodosi {

// The Markdown of an agent becomes the rich text of Qt, and the agent's own words never become
// markup.
class ChatProse final : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;

    // Each block is a map with its `key`, its place, and its `kind`: `lines` and `heading` with
    // `html`, `code` with `text`, and `table` with `header` and `rows` of cells as `html`, `sides`
    // and `columns`. A fence that is still open while the agent writes holds the rest.
    Q_INVOKABLE [[nodiscard]] static QVariantList blocks(const QString& text);
    // A link goes only to a secure web address.
    [[nodiscard]] static QString html(QStringView text);
    Q_INVOKABLE [[nodiscard]] static QString text(const QString& text);
    // The cells of a row of a table. A bar after a backslash belongs to the words of its cell, and
    // the bars at the two ends of a row hold no cell.
    [[nodiscard]] static QStringList cells(QStringView line);
};

}
